use std::ffi::OsStr;
use std::fs::{self, File};
use std::os::fd::AsFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};

use rustix::fs::{FileType, Mode};

use xsh_root::transfer::{
    Action, Cancellation, Collision, MergeRequest, Operands, Operation, PolicyOptions,
    PolicyRequest, Progress, merge_tree, preflight,
};

use super::support::{
    Fixture, access_acl, get_xattr, open_directory, publish, set_times, set_xattr,
};

#[test]
fn merge_retains_existing_directory_metadata_and_finalizes_new_directory_metadata() {
    let fixture = Fixture::new();
    let cwd = fixture.path("work");
    let source = cwd.join("source");
    let destination = cwd.join("destination");
    fs::create_dir_all(source.join("new/nested")).expect("create source tree");
    fs::write(source.join("new/nested/file"), b"new subtree").expect("write nested file");
    fs::create_dir(&destination).expect("create destination");
    let source_acl = access_acl(7, 4, 5, 5, 0);
    let destination_acl = access_acl(7, 1, 0, 1, 1);
    set_xattr(&source, b"user.xsh-fs-container", b"source container");
    let source_fd = File::open(&source).expect("open source");
    rustix::fs::fsetxattr(
        &source_fd,
        c"system.posix_acl_access",
        &source_acl,
        rustix::fs::XattrFlags::empty(),
    )
    .expect("set source ACL");
    set_xattr(
        &destination,
        b"user.xsh-fs-container",
        b"destination container",
    );
    let destination_fd = File::open(&destination).expect("open destination");
    fs::set_permissions(&source, fs::Permissions::from_mode(0o750)).expect("set source mode");
    fs::set_permissions(&destination, fs::Permissions::from_mode(0o711))
        .expect("set destination mode");
    rustix::fs::fsetxattr(
        &destination_fd,
        c"system.posix_acl_access",
        &destination_acl,
        rustix::fs::XattrFlags::empty(),
    )
    .expect("set destination ACL");
    fs::set_permissions(source.join("new"), fs::Permissions::from_mode(0o710))
        .expect("set new directory mode");
    set_xattr(
        &source.join("new"),
        b"user.xsh-fs-new-directory",
        b"new directory",
    );
    set_times(&source.join("new"), 1_700_000_200, 456_000_000);
    set_times(&source, 1_700_000_300, 789_000_000);
    set_times(&destination, 1_700_000_400, 987_000_000);

    let operands = Operands::Exact {
        source: "source".into(),
        destination: "destination".into(),
    };
    let cwd_fd = open_directory(&cwd).expect("open fixture cwd");
    let plan = preflight(
        cwd_fd.as_fd(),
        &PolicyRequest {
            operation: Operation::Copy,
            operands: &operands,
            options: PolicyOptions {
                collision: Collision::Error,
                merge: true,
                dry_run: false,
            },
            home: None,
        },
    )
    .expect("preflight merge");
    assert_eq!(plan.operands[0].action, Action::Merge);
    let source_parent = open_directory(&cwd).expect("open source parent");
    let destination_parent = open_directory(&cwd).expect("open destination parent");
    let mut progress = Progress::default();
    merge_tree(MergeRequest {
        source_parent: source_parent.as_fd(),
        source_name: OsStr::new("source"),
        destination_parent: destination_parent.as_fd(),
        destination_name: OsStr::new("destination"),
        expected_source: plan.operands[0].source_observed,
        expected_destination: plan.operands[0]
            .destination_observed
            .expect("merge destination observed"),
        options: PolicyOptions {
            collision: Collision::Error,
            merge: true,
            dry_run: false,
        },
        engine: xsh_root::transfer::EngineOptions::default(),
        cancellation: &Cancellation::default(),
        progress: &mut progress,
        live: None,
    })
    .expect("merge trees");

    let existing = fs::metadata(&destination).expect("destination metadata");
    assert_eq!(existing.permissions().mode() & 0o7777, 0o711);
    assert_eq!(
        get_xattr(&destination, b"user.xsh-fs-container"),
        b"destination container"
    );
    assert_eq!(
        get_xattr(&destination, b"system.posix_acl_access"),
        destination_acl
    );
    let created = destination.join("new");
    let created_metadata = fs::metadata(&created).expect("created directory metadata");
    assert_eq!(created_metadata.permissions().mode() & 0o7777, 0o710);
    assert_eq!(
        (created_metadata.mtime(), created_metadata.mtime_nsec()),
        (1_700_000_200, 456_000_000)
    );
    assert_eq!(
        get_xattr(&created, b"user.xsh-fs-new-directory"),
        b"new directory"
    );
    assert_eq!(
        fs::read(created.join("nested/file")).expect("read merged file"),
        b"new subtree"
    );
}

#[test]
fn unsupported_fifo_fails_without_publishing_a_partial_tree_with_readonly_files() {
    let fixture = Fixture::new();
    let cwd = fixture.path("work");
    let source = cwd.join("source");
    fs::create_dir(&source).expect("create source directory");
    fs::write(source.join("readonly"), b"read-only bytes").expect("write read-only file");
    fs::set_permissions(source.join("readonly"), fs::Permissions::from_mode(0o444))
        .expect("set read-only mode");
    let source_fd = open_directory(&source).expect("open source");
    rustix::fs::mknodat(
        source_fd.as_fd(),
        "pipe",
        FileType::Fifo,
        Mode::RUSR | Mode::WUSR,
        0,
    )
    .expect("create fifo");
    let operands = Operands::Exact {
        source: "source".into(),
        destination: "destination".into(),
    };
    let cwd_fd = open_directory(&cwd).expect("open fixture cwd");
    let plan = preflight(
        cwd_fd.as_fd(),
        &PolicyRequest {
            operation: Operation::Copy,
            operands: &operands,
            options: PolicyOptions::default(),
            home: None,
        },
    )
    .expect("preflight fifo source");
    assert_eq!(plan.operands[0].action, Action::Transfer);
    let mut progress = Progress::default();
    let error = publish(
        &cwd,
        OsStr::new("source"),
        &cwd,
        OsStr::new("destination"),
        Action::Transfer,
        plan.operands[0].source_observed,
        None,
        &mut progress,
    )
    .expect_err("a fifo must fail the transfer");
    assert_eq!(error.kind(), std::io::ErrorKind::Unsupported);
    assert!(!cwd.join("destination").exists());
    assert_eq!(
        fs::read(source.join("readonly")).expect("read source"),
        b"read-only bytes"
    );
    assert!(
        fs::symlink_metadata(source.join("pipe"))
            .expect("fifo metadata")
            .file_type()
            .is_fifo()
    );
    assert_eq!(fs::read_dir(&cwd).expect("read cwd").count(), 1);
}
