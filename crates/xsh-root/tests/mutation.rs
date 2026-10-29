use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use xsh_root::{FileKind, OpenOptions, RenameMode, Root};

struct Fixture {
    base: tempfile::TempDir,
    root_path: PathBuf,
    outside: PathBuf,
    root: Root,
}

impl Fixture {
    fn new() -> io::Result<Self> {
        let base = tempfile::tempdir()?;
        let root_path = base.path().join("root");
        let outside = base.path().join("outside");
        fs::create_dir(&root_path)?;
        fs::create_dir(&outside)?;
        fs::write(outside.join("sentinel"), "outside")?;
        let root = Root::open(&root_path)?;
        Ok(Self {
            base,
            root_path,
            outside,
            root,
        })
    }

    fn outside_untouched(&self) {
        let mut names: Vec<_> = fs::read_dir(&self.outside)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        names.sort();
        assert_eq!(names, vec![std::ffi::OsString::from("sentinel")]);
        assert_eq!(fs::read_to_string(self.outside.join("sentinel")).unwrap(), "outside");
        let _ = &self.base;
    }
}

#[test]
fn creates_directories_links_and_renames_beneath_the_root() -> io::Result<()> {
    let fixture = Fixture::new()?;
    let root = &fixture.root;
    root.mkdir("dir", 0o755)?;
    root.mkdir("dir/sub", 0o700)?;
    let mut file = root.open_with(
        "dir/sub/file",
        OpenOptions::new().write(true).create_new(true).no_symlinks(true).mode(0o600),
    )?;
    file.write_all(b"payload")?;
    root.hard_link("dir/sub/file", "dir/hard")?;
    root.symlink("sub/file", "dir/soft")?;
    root.rename("dir/hard", "dir/renamed", RenameMode::NoReplace)?;

    assert_eq!(fs::read_to_string(fixture.root_path.join("dir/renamed"))?, "payload");
    assert_eq!(fs::read_link(fixture.root_path.join("dir/soft"))?, Path::new("sub/file"));
    assert_eq!(fs::metadata(fixture.root_path.join("dir/sub/file"))?.nlink(), 2);
    assert_eq!(root.read_link("dir/soft")?, std::ffi::OsString::from("sub/file"));

    let metadata = root.metadata("dir/soft")?;
    assert_eq!(metadata.kind, FileKind::Symlink);
    assert_eq!(root.metadata("dir/sub")?.kind, FileKind::Directory);
    assert_eq!(root.metadata(".")?.kind, FileKind::Directory);

    root.set_mode("dir/sub", 0o500)?;
    assert_eq!(fs::metadata(fixture.root_path.join("dir/sub"))?.permissions().mode() & 0o777, 0o500);
    root.set_mode("dir/sub", 0o700)?;

    let mut listed: Vec<_> = root
        .read_dir("dir")?
        .into_iter()
        .map(|entry| (entry.name.to_string_lossy().into_owned(), entry.kind))
        .collect();
    listed.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        listed,
        vec![
            ("renamed".to_string(), FileKind::File),
            ("soft".to_string(), FileKind::Symlink),
            ("sub".to_string(), FileKind::Directory),
        ]
    );

    root.remove_file("dir/soft")?;
    root.remove_file("dir/renamed")?;
    root.remove_file("dir/sub/file")?;
    root.remove_dir("dir/sub")?;
    root.remove_dir("dir")?;
    assert!(root.read_dir(".")?.is_empty());
    Ok(())
}

#[test]
fn rename_without_replace_refuses_an_existing_name() -> io::Result<()> {
    let fixture = Fixture::new()?;
    fs::write(fixture.root_path.join("a"), "a")?;
    fs::write(fixture.root_path.join("b"), "b")?;
    let error = fixture.root.rename("a", "b", RenameMode::NoReplace).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read_to_string(fixture.root_path.join("b"))?, "b");
    fixture.root.rename("a", "b", RenameMode::Replace)?;
    assert_eq!(fs::read_to_string(fixture.root_path.join("b"))?, "a");
    Ok(())
}

#[test]
fn rejects_absolute_dotted_and_empty_mutation_paths() -> io::Result<()> {
    let fixture = Fixture::new()?;
    for path in ["/abs", "../up", "a/../b", "./a", "", "."] {
        let error = fixture.root.mkdir(path, 0o755).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{path:?}");
        assert!(fixture.root.symlink("x", path).is_err(), "{path:?}");
        assert!(fixture.root.remove_file(path).is_err(), "{path:?}");
    }
    fixture.outside_untouched();
    Ok(())
}

#[test]
fn a_symlinked_directory_is_never_traversed_even_inside_the_root() -> io::Result<()> {
    let fixture = Fixture::new()?;
    fixture.root.mkdir("real", 0o755)?;
    symlink("real", fixture.root_path.join("inside-link"))?;
    symlink(&fixture.outside, fixture.root_path.join("outside-link"))?;
    symlink("../outside", fixture.root_path.join("relative-escape"))?;

    for through in ["inside-link", "outside-link", "relative-escape"] {
        let leaf = format!("{through}/created");
        assert!(fixture.root.mkdir(&leaf, 0o755).is_err(), "{leaf}");
        assert!(fixture.root.symlink("x", &leaf).is_err(), "{leaf}");
        assert!(
            fixture
                .root
                .open_with(&leaf, OpenOptions::new().write(true).create_new(true).no_symlinks(true))
                .is_err(),
            "{leaf}"
        );
        assert!(fixture.root.open_dir_strict(through).is_err(), "{through}");
    }
    assert!(!fixture.root_path.join("real/created").exists());
    fixture.outside_untouched();
    Ok(())
}

#[test]
fn creating_over_a_final_symlink_is_refused_without_following_it() -> io::Result<()> {
    let fixture = Fixture::new()?;
    symlink(fixture.outside.join("sentinel"), fixture.root_path.join("trap"))?;
    let result = fixture.root.open_with(
        "trap",
        OpenOptions::new().write(true).truncate(true).no_symlinks(true),
    );
    assert!(result.is_err());
    let created = fixture.root.open_with(
        "trap",
        OpenOptions::new().write(true).create_new(true).no_symlinks(true),
    );
    assert!(created.is_err());
    assert_eq!(fs::read_to_string(fixture.outside.join("sentinel"))?, "outside");
    Ok(())
}

#[test]
fn chmod_and_hard_links_do_not_follow_a_final_symlink() -> io::Result<()> {
    let fixture = Fixture::new()?;
    let target = fixture.outside.join("sentinel");
    fs::set_permissions(&target, fs::Permissions::from_mode(0o644))?;
    symlink(&target, fixture.root_path.join("trap"))?;
    let _ = fixture.root.set_mode("trap", 0o777);
    assert_eq!(fs::metadata(&target)?.permissions().mode() & 0o777, 0o644);
    fixture.root.hard_link("trap", "alias")?;
    assert!(fs::symlink_metadata(fixture.root_path.join("alias"))?.file_type().is_symlink());
    assert_eq!(fs::metadata(&target)?.nlink(), 1);
    Ok(())
}

#[test]
fn concurrent_directory_swaps_never_create_outside_the_root() -> io::Result<()> {
    let fixture = Fixture::new()?;
    let root_path = fixture.root_path.clone();
    let outside = fixture.outside.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let swapper = {
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let dir = root_path.join("d");
                let _ = fs::remove_file(&dir);
                let _ = fs::remove_dir(&dir);
                let _ = fs::create_dir(&dir);
                let _ = fs::remove_dir(&dir);
                let _ = symlink(&outside, &dir);
            }
        })
    };
    for round in 0..4000 {
        let _ = fixture.root.mkdir(format!("d/dir-{round}"), 0o755);
        let _ = fixture.root.symlink("x", format!("d/link-{round}"));
        let _ = fixture.root.open_with(
            format!("d/file-{round}"),
            OpenOptions::new().write(true).create_new(true).no_symlinks(true),
        );
    }
    stop.store(true, Ordering::Relaxed);
    swapper.join().unwrap();
    fixture.outside_untouched();
    Ok(())
}
