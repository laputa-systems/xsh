use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use xsh_root::{OpenOptions, Root};

static NEXT_TEMP_ID: AtomicUsize = AtomicUsize::new(0);

struct TempTree {
    path: PathBuf,
}

impl TempTree {
    fn new() -> io::Result<Self> {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time is after the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "xsh-root-{}-{nanos}-{id}",
            std::process::id()
        ));
        fs::create_dir(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct Tree {
    temp: TempTree,
    root_path: PathBuf,
    outside_path: PathBuf,
    outside_sentinel: PathBuf,
    root: Root,
}

impl Tree {
    fn new() -> io::Result<Self> {
        let temp = TempTree::new()?;
        let root_path = temp.path.join("root");
        let outside_path = temp.path.join("outside-dir");
        let outside_file = temp.path.join("outside.txt");
        let outside_sentinel = outside_path.join("secret.txt");
        fs::create_dir(&root_path)?;
        fs::create_dir(&outside_path)?;
        fs::write(root_path.join("inside.txt"), "inside")?;
        fs::create_dir(root_path.join("dir"))?;
        fs::write(root_path.join("dir/nested.txt"), "nested")?;
        fs::write(&outside_file, "outside")?;
        fs::write(&outside_sentinel, "outside secret")?;
        symlink("dir/nested.txt", root_path.join("inside-link"))?;
        symlink("../outside.txt", root_path.join("escape-link"))?;
        symlink("../outside-dir", root_path.join("escape-dir"))?;
        symlink(&outside_file, root_path.join("absolute-link"))?;
        let root = Root::open(&root_path)?;
        Ok(Self {
            temp,
            root_path,
            outside_path,
            outside_sentinel,
            root,
        })
    }
}

fn read(root: &Root, path: impl AsRef<Path>) -> io::Result<String> {
    let mut text = String::new();
    root.open_file(path)?.read_to_string(&mut text)?;
    Ok(text)
}

#[test]
fn opens_in_root_paths_and_relative_symlinks() -> io::Result<()> {
    let tree = Tree::new()?;
    assert_eq!(read(&tree.root, "inside.txt")?, "inside");
    assert_eq!(read(&tree.root, "dir/nested.txt")?, "nested");
    assert_eq!(read(&tree.root, "./dir/./nested.txt")?, "nested");
    assert_eq!(read(&tree.root, "dir/../inside.txt")?, "inside");
    assert_eq!(read(&tree.root, "inside-link")?, "nested");
    tree.root.create("created.txt")?.write_all(b"created")?;
    assert_eq!(read(&tree.root, "created.txt")?, "created");
    Ok(())
}

#[test]
fn rejects_escapes_and_absolute_paths() -> io::Result<()> {
    let tree = Tree::new()?;
    for path in [
        "../outside.txt",
        "../../../../outside.txt",
        "/etc/passwd",
        "escape-link",
        "escape-dir/secret.txt",
        "absolute-link",
    ] {
        assert!(
            tree.root.open_file(path).is_err(),
            "escaping path unexpectedly opened: {path}"
        );
    }
    symlink("escape-link", tree.root_path.join("chain-one"))?;
    symlink("chain-one", tree.root_path.join("chain-two"))?;
    assert!(tree.root.open_file("chain-two").is_err());
    Ok(())
}

#[test]
fn escaping_create_and_truncate_cannot_change_outside_file() -> io::Result<()> {
    let tree = Tree::new()?;
    let outside_file = tree.temp.path.join("outside.txt");
    let mut write = OpenOptions::new();
    write.write(true).create(true).truncate(true);
    assert!(tree.root.open_with("escape-link", &write).is_err());
    assert_eq!(fs::read_to_string(&outside_file)?, "outside");

    let mut create_new = OpenOptions::new();
    create_new.write(true).create_new(true);
    assert!(
        tree.root
            .open_with("escape-dir/new-outside.txt", &create_new)
            .is_err()
    );
    assert!(!tree.outside_path.join("new-outside.txt").exists());
    Ok(())
}

#[test]
fn root_descriptor_survives_rename() -> io::Result<()> {
    let tree = Tree::new()?;
    let renamed = tree.temp.path.join("renamed-root");
    fs::rename(&tree.root_path, renamed)?;
    assert_eq!(read(&tree.root, "inside.txt")?, "inside");
    Ok(())
}

#[test]
fn embedded_nul_is_an_input_error() -> io::Result<()> {
    let tree = Tree::new()?;
    let path = PathBuf::from(OsString::from_vec(b"nul\0path".to_vec()));
    let error = tree.root.open_file(path).expect_err("embedded NUL must fail");
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    Ok(())
}

#[test]
fn concurrent_rename_and_symlink_switch_never_returns_outside_sentinel() -> io::Result<()> {
    let tree = Tree::new()?;
    let safe = tree.root_path.join("safe");
    let switch = tree.root_path.join("switch");
    fs::create_dir(&safe)?;
    fs::write(safe.join("sentinel"), "inside sentinel")?;

    let root = Arc::new(tree.root);
    let done = Arc::new(AtomicBool::new(false));
    let reader_root = Arc::clone(&root);
    let reader_done = Arc::clone(&done);
    let reader = thread::spawn(move || -> io::Result<()> {
        while !reader_done.load(Ordering::Acquire) {
            match read(&reader_root, "switch/sentinel") {
                Ok(contents) => assert_eq!(contents, "inside sentinel"),
                Err(_) => {}
            }
        }
        Ok(())
    });

    for _ in 0..10_000 {
        fs::rename(&safe, &switch)?;
        fs::rename(&switch, &safe)?;
        symlink("../outside-dir", &switch)?;
        fs::remove_file(&switch)?;
    }
    done.store(true, Ordering::Release);
    reader.join().expect("reader thread panicked")?;
    assert_eq!(fs::read_to_string(&tree.outside_sentinel)?, "outside secret");
    Ok(())
}
