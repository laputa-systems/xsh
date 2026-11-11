use std::ffi::{OsStr, OsString};
use std::fs::{self, OpenOptions};
use std::os::fd::AsFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

use rustix::fs::{self as rfs, IFlags, XattrFlags};
use rustix::io::Errno;
use xsh_root::transfer::{Metadata, MetadataTarget, Preservation};

use super::support::{Fixture, access_acl, set_times, try_set_user_xattr, unsupported_errno};

fn acl_xattr() -> Vec<u8> {
    let entries = [
        (0x01_u16, 0x07_u16, u32::MAX),
        (0x02, 0x04, 1234),
        (0x04, 0x05, u32::MAX),
        (0x10, 0x05, u32::MAX),
        (0x20, 0x00, u32::MAX),
    ];
    let mut bytes = Vec::with_capacity(4 + entries.len() * 8);
    bytes.extend_from_slice(&2_u32.to_le_bytes());
    for (tag, permission, id) in entries {
        bytes.extend_from_slice(&tag.to_le_bytes());
        bytes.extend_from_slice(&permission.to_le_bytes());
        bytes.extend_from_slice(&id.to_le_bytes());
    }
    bytes
}

#[test]
fn metadata_preserves_xattrs_acl_mode_and_timestamps() {
    let fixture = Fixture::new();
    let source_path = fixture.path("source");
    let destination_path = fixture.path("destination");
    fs::write(&source_path, b"metadata payload").expect("write source");
    let source =
        OpenOptions::new().read(true).write(true).open(&source_path).expect("open source");
    let user_xattr = b"user.ish-fs-native";
    if !try_set_user_xattr(source.as_fd(), user_xattr, b"source-value") {
        return;
    }
    let acl = acl_xattr();
    match rfs::fsetxattr(
        source.as_fd(),
        OsStr::from_bytes(b"system.posix_acl_access"),
        &acl,
        XattrFlags::empty(),
    ) {
        Ok(()) => {}
        Err(error) if unsupported_errno(error) || error == Errno::PERM || error == Errno::INVAL => {
            eprintln!("native Linux POSIX access ACL unavailable on fixture filesystem: {error}");
            return;
        }
        Err(error) => panic!("set source ACL: {error}"),
    }
    fs::set_permissions(&source_path, fs::Permissions::from_mode(0o4750))
        .expect("set source mode");
    let captured = Metadata::capture(MetadataTarget::Fd(source.as_fd()))
        .expect("capture source metadata");
    assert_eq!(captured.acl, Some(acl.clone()));

    let destination = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&destination_path)
        .expect("create destination");
    assert!(try_set_user_xattr(
        destination.as_fd(),
        b"user.ish-fs-stale",
        b"remove-this",
    ));
    captured
        .apply(MetadataTarget::Fd(destination.as_fd()), Preservation::Copy)
        .expect("apply copied metadata");
    let applied = Metadata::capture(MetadataTarget::Fd(destination.as_fd()))
        .expect("capture destination metadata");
    assert_eq!(applied.mode & 0o7777, captured.mode & 0o7777);
    assert_eq!(applied.atime, captured.atime);
    assert_eq!(applied.mtime, captured.mtime);
    assert!(
        applied
            .xattrs
            .iter()
            .any(|xattr| xattr.name == OsStr::from_bytes(user_xattr)
                && xattr.value == b"source-value")
    );
    assert!(
        !applied
            .xattrs
            .iter()
            .any(|xattr| xattr.name == OsStr::from_bytes(b"user.ish-fs-stale"))
    );
    assert_eq!(applied.acl, Some(acl));

    let mut wrong_owner = captured.clone();
    wrong_owner.uid = wrong_owner.uid.wrapping_add(1);
    wrong_owner.mode = 0o4750;
    wrong_owner.acl = None;
    wrong_owner.xattrs = vec![xsh_root::transfer::Xattr {
        name: OsString::from("security.capability"),
        value: vec![1],
    }];
    let restricted_path = fixture.path("restricted");
    let restricted = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&restricted_path)
        .expect("create restricted destination");
    wrong_owner
        .apply(MetadataTarget::Fd(restricted.as_fd()), Preservation::Copy)
        .expect("apply normalized copy metadata");
    let restricted_mode = fs::metadata(&restricted_path).expect("stat restricted").mode() & 0o7777;
    assert_eq!(restricted_mode, 0o0750);
    let mut value = [0; 64];
    assert_eq!(
        rfs::fgetxattr(
            restricted.as_fd(),
            OsStr::from_bytes(b"security.capability"),
            &mut value,
        )
        .unwrap_err(),
        Errno::NODATA,
    );
}

#[test]
fn symlink_metadata_uses_parent_fd_and_nofollow_operations() {
    let fixture = Fixture::new();
    let source_path = fixture.path("source-link");
    let destination_path = fixture.path("destination-link");
    symlink("missing-target", &source_path).expect("create source symlink");
    let parent = fs::File::open(fixture.root()).expect("open fixture directory");
    let source_name = std::ffi::OsStr::new("source-link");
    let link_xattr_supported = match rfs::lsetxattr(
        &source_path,
        OsStr::from_bytes(b"user.ish-fs-link"),
        b"link-value",
        XattrFlags::empty(),
    ) {
        Ok(()) => true,
        Err(error) if unsupported_errno(error) || error == Errno::PERM => {
            eprintln!("native Linux symlink xattrs unavailable on fixture filesystem: {error}");
            false
        }
        Err(error) => panic!("set symlink xattr: {error}"),
    };
    let captured = Metadata::capture(MetadataTarget::Symlink {
        parent: parent.as_fd(),
        name: source_name,
    })
    .expect("capture source symlink metadata");
    if link_xattr_supported {
        assert!(
            captured
                .xattrs
                .iter()
                .any(|xattr| xattr.name == OsStr::from_bytes(b"user.ish-fs-link"))
        );
    }
    symlink("temporary", &destination_path).expect("create destination symlink");
    captured
        .apply(
            MetadataTarget::Symlink {
                parent: parent.as_fd(),
                name: std::ffi::OsStr::new("destination-link"),
            },
            Preservation::Copy,
        )
        .expect("apply destination symlink metadata");
    let destination =
        fs::symlink_metadata(&destination_path).expect("stat destination symlink");
    assert_eq!(destination.atime(), captured.atime.seconds);
    assert_eq!(destination.atime_nsec(), i64::from(captured.atime.nanoseconds));
    assert_eq!(destination.mtime(), captured.mtime.seconds);
    assert_eq!(destination.mtime_nsec(), i64::from(captured.mtime.nanoseconds));
    assert_eq!(
        fs::read_link(&destination_path).expect("read destination link"),
        std::path::PathBuf::from("temporary")
    );
    if link_xattr_supported {
        let mut value = [0; 64];
        let length = rfs::lgetxattr(
            &destination_path,
            OsStr::from_bytes(b"user.ish-fs-link"),
            &mut value,
        )
        .expect("read destination symlink xattr");
        assert_eq!(&value[..length], b"link-value");
    }
}

#[test]
fn finalization_rejects_inode_flags_on_symlinks() {
    let fixture = Fixture::new();
    let link_path = fixture.path("link");
    symlink("target", &link_path).expect("create symlink");
    let parent = fs::File::open(fixture.root()).expect("open fixture directory");
    let metadata = Metadata {
        flags: IFlags::IMMUTABLE.bits(),
        ..Metadata::default()
    };
    let error = metadata
        .apply_final(MetadataTarget::Symlink {
            parent: parent.as_fd(),
            name: std::ffi::OsStr::new("link"),
        })
        .expect_err("symlink flags are unrepresentable");
    assert_eq!(error.kind(), std::io::ErrorKind::Unsupported);
}

#[test]
fn copy_preserves_file_metadata_and_caller_ownership() {
        let fixture = Fixture::new();
    let work = fixture.path("work");
    let source = work.join("source");
    let destination = work.join("destination");
    fs::write(&source, b"metadata payload").expect("write source");
    fs::set_permissions(&source, fs::Permissions::from_mode(0o640)).expect("set mode");
    let acl = access_acl(6, 4, 4, 4, 0);
    let source_file =
        OpenOptions::new().read(true).write(true).open(&source).expect("open source");
    super::support::set_xattr(&source, b"user.ish-fs-preservation", b"source value");
    match rfs::fsetxattr(
        source_file.as_fd(),
        OsStr::from_bytes(b"system.posix_acl_access"),
        &acl,
        XattrFlags::empty(),
    ) {
        Ok(()) => {}
        Err(error) if unsupported_errno(error) || error == Errno::PERM || error == Errno::INVAL => {
            eprintln!("native Linux POSIX access ACL unavailable on fixture filesystem: {error}");
            return;
        }
        Err(error) => panic!("set source ACL: {error}"),
    }
    set_times(&source, 1_700_000_100, 123_000_000);
    let source_metadata = fs::metadata(&source).expect("stat source");

    let progress = super::support::transfer_named(&work, "source", &work, "destination")
        .expect("copy source");
    let copied = fs::metadata(&destination).expect("stat destination");
    assert_eq!(fs::read(&destination).expect("read destination"), b"metadata payload");
    assert_eq!(copied.uid(), source_metadata.uid());
    assert_eq!(copied.gid(), source_metadata.gid());
    assert_eq!(
        copied.permissions().mode() & 0o7777,
        source_metadata.permissions().mode() & 0o7777
    );
    assert_eq!((copied.mtime(), copied.mtime_nsec()), (1_700_000_100, 123_000_000));
    assert_eq!(
        super::support::get_xattr(&destination, b"user.ish-fs-preservation"),
        b"source value"
    );
    assert_eq!(
        super::support::get_xattr(&destination, b"system.posix_acl_access"),
        acl
    );
    assert_eq!(progress.logical_bytes, 16);
    assert_eq!(progress.completed_files, 1);
}
