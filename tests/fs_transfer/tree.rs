use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

use super::support::Fixture;

#[test]
fn tree_preserves_bytes_symlinks_and_hardlink_groups() {
    let fixture = Fixture::new();
    let work = fixture.path("work");
    let source = work.join("source");
    fs::create_dir(&source).expect("create source");
    fs::write(source.join("file"), b"ordinary bytes\0\xff").expect("write file");
    fs::hard_link(source.join("file"), source.join("alias")).expect("link alias");
    symlink("file", source.join("link")).expect("create link");
    let raw_name = OsStr::from_bytes(b"raw\xff\n");
    let raw_name_supported = match fs::write(source.join(raw_name), b"raw bytes\0\xff") {
        Ok(()) => {
            fs::hard_link(source.join(raw_name), source.join("raw-alias"))
                .expect("link raw alias");
            symlink(raw_name, source.join("raw-link")).expect("create raw link");
            true
        }
        Err(error) if error.raw_os_error() == Some(libc::EILSEQ) => {
            eprintln!("raw-name path leg excluded: filesystem rejected malformed UTF-8");
            false
        }
        Err(error) => panic!("create raw-name source: {error}"),
    };

    let progress = super::support::transfer_named(&work, "source", &work, "destination")
        .expect("transfer source tree");
    let destination = work.join("destination");
    assert_eq!(
        fs::read(destination.join("file")).expect("read destination file"),
        b"ordinary bytes\0\xff"
    );
    assert_eq!(
        fs::read_link(destination.join("link")).expect("read destination link"),
        std::path::Path::new("file")
    );
    assert_eq!(
        fs::metadata(destination.join("file")).expect("stat file").ino(),
        fs::metadata(destination.join("alias")).expect("stat alias").ino()
    );
    assert_eq!(
        fs::metadata(destination.join("file")).expect("stat file").nlink(),
        2
    );
    if raw_name_supported {
        assert_eq!(
            fs::read(destination.join(raw_name)).expect("read raw destination file"),
            b"raw bytes\0\xff"
        );
        assert_eq!(
            fs::read_link(destination.join("raw-link")).expect("read raw destination link"),
            std::path::Path::new(raw_name)
        );
        assert_eq!(
            fs::metadata(destination.join(raw_name)).expect("stat raw file").ino(),
            fs::metadata(destination.join("raw-alias")).expect("stat raw alias").ino()
        );
    }
    assert_eq!(progress.failed_files, 0);
    assert_eq!(progress.completed_files, progress.discovered_files);
    assert_eq!(progress.discovered_files, if raw_name_supported { 7 } else { 4 });
    assert_eq!(progress.logical_bytes, if raw_name_supported { 27 } else { 16 });
}

#[test]
fn copy_normalizes_privilege_bits_for_a_supported_supplementary_group() {
    let fixture = Fixture::new();
    let work = fixture.path("work");
    let parent_gid = fs::metadata(&work).expect("stat work").gid();
    let source_group = rustix::process::getgroups()
        .expect("read supplementary groups")
        .into_iter()
        .map(|group| group.as_raw())
        .find(|group| *group != parent_gid);
    let Some(source_group) = source_group else {
        eprintln!(
            "privilege normalization fixture unavailable: process has no supplementary group distinct from its working directory"
        );
        return;
    };
    let source = work.join("source");
    fs::create_dir(&source).expect("create source");
    std::os::unix::fs::chown(&source, None, Some(source_group)).expect("assign source group");
    fs::set_permissions(&source, fs::Permissions::from_mode(0o2755))
        .expect("set source directory mode");
    fs::write(source.join("file"), b"grouped source").expect("write source file");
    fs::set_permissions(source.join("file"), fs::Permissions::from_mode(0o6750))
        .expect("set source file mode");
    let source_directory = fs::metadata(&source).expect("stat source directory");
    let source_file = fs::metadata(source.join("file")).expect("stat source file");
    assert_ne!(source_directory.gid(), parent_gid);
    assert_eq!(source_directory.permissions().mode() & 0o7777, 0o2755);
    assert_eq!(source_file.gid(), source_group);
    assert_eq!(source_file.permissions().mode() & 0o7777, 0o6750);

    super::support::transfer_named(&work, "source", &work, "destination")
        .expect("transfer grouped source");

    let copied = fs::metadata(work.join("destination/file")).expect("stat copied file");
    assert_eq!(copied.uid(), source_file.uid());
    assert_eq!(copied.uid(), fs::metadata(&work).expect("stat work").uid());
    assert_eq!(copied.gid(), parent_gid);
    assert_eq!(copied.permissions().mode() & 0o7777, 0o0750);
    assert_eq!(
        fs::metadata(work.join("destination"))
            .expect("stat copied directory")
            .permissions()
            .mode()
            & 0o7777,
        0o0755
    );
}

#[test]
fn transfer_rejects_a_destination_inside_the_source_tree() {
    let fixture = Fixture::new();
    let work = fixture.path("work");
    fs::create_dir(work.join("source")).expect("create source");
    fs::create_dir(work.join("source/nested")).expect("create nested source");
    let error =
        super::support::transfer_named(&work, "source", &work.join("source"), "copy")
            .expect_err("a destination inside the source must be refused");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert!(!work.join("source/copy").exists());
}
