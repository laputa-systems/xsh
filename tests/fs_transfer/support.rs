use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::fd::AsFd;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use rustix::fs::{self as rfs, XattrFlags};
use rustix::io::Errno;
use xsh_root::transfer::{
    Action, Cancellation, EngineOptions, Observation, Progress, PublishRequest, TreeTransfer,
    observe_entry, publish_tree, transfer_tree,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

pub struct Fixture {
    root: PathBuf,
}

impl Fixture {
    pub fn new() -> Self {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join(".work/tmp");
        fs::create_dir_all(&parent).expect("create shared test scratch");
        let root = loop {
            let index = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!("fs-transfer-{}-{index}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => break path,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("create fixture: {error}"),
            }
        };
        let fixture = Self { root };
        fs::create_dir(fixture.path("work")).expect("create fixture work directory");
        fixture
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub fn transfer(
    source_parent: &Path,
    source_name: &OsStr,
    destination_parent: &Path,
    destination_name: &OsStr,
) -> io::Result<Progress> {
    let source = File::open(source_parent)?;
    let destination = File::open(destination_parent)?;
    let cancellation = Cancellation::default();
    let mut progress = Progress::default();
    let mut deferred = Vec::new();
    transfer_tree(TreeTransfer {
        source_parent: source.as_fd(),
        source_name,
        destination_parent: destination.as_fd(),
        destination_name,
        engine: EngineOptions::default(),
        cancellation: &cancellation,
        progress: &mut progress,
        deferred_flags: &mut deferred,
    })?;
    Ok(progress)
}

pub fn transfer_named(
    source_parent: &Path,
    source_name: &str,
    destination_parent: &Path,
    destination_name: &str,
) -> io::Result<Progress> {
    transfer(
        source_parent,
        OsStr::new(source_name),
        destination_parent,
        OsStr::new(destination_name),
    )
}

pub fn open_directory(path: &Path) -> io::Result<File> {
    let path = if path.as_os_str().is_empty() {
        Path::new(".")
    } else {
        path
    };
    File::open(path)
}

pub fn observe_path(path: &Path) -> Observation {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path.file_name().expect("fixture path has a final name");
    let directory = open_directory(parent).expect("open fixture parent");
    observe_entry(directory.as_fd(), name).expect("observe fixture entry")
}

pub fn publish(
    source_parent: &Path,
    source_name: &OsStr,
    destination_parent: &Path,
    destination_name: &OsStr,
    action: Action,
    expected_source: Observation,
    expected_destination: Option<Observation>,
    progress: &mut Progress,
) -> io::Result<()> {
    publish_durable(
        source_parent,
        source_name,
        destination_parent,
        destination_name,
        action,
        expected_source,
        expected_destination,
        false,
        progress,
    )
}

pub fn publish_durable(
    source_parent: &Path,
    source_name: &OsStr,
    destination_parent: &Path,
    destination_name: &OsStr,
    action: Action,
    expected_source: Observation,
    expected_destination: Option<Observation>,
    durable: bool,
    progress: &mut Progress,
) -> io::Result<()> {
    let source = open_directory(source_parent)?;
    let destination = open_directory(destination_parent)?;
    publish_at(
        source.as_fd(),
        source_name,
        destination.as_fd(),
        destination_name,
        action,
        expected_source,
        expected_destination,
        durable,
        progress,
    )
}

pub fn publish_at(
    source_parent: std::os::fd::BorrowedFd<'_>,
    source_name: &OsStr,
    destination_parent: std::os::fd::BorrowedFd<'_>,
    destination_name: &OsStr,
    action: Action,
    expected_source: Observation,
    expected_destination: Option<Observation>,
    durable: bool,
    progress: &mut Progress,
) -> io::Result<()> {
    let cancellation = Cancellation::default();
    publish_tree(
        PublishRequest {
            source_parent,
            source_name,
            destination_parent,
            destination_name,
            expected_source,
            expected_destination,
            action,
            engine: EngineOptions {
                durable,
                ..EngineOptions::default()
            },
            cancellation: &cancellation,
            progress,
        },
        |_| Ok(()),
    )
}

pub fn set_times(path: &Path, seconds: i64, nanoseconds: i64) {
    let file = if path.is_dir() {
        File::open(path).expect("open fixture directory")
    } else {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .expect("open fixture file")
    };
    rfs::futimens(
        &file,
        &rfs::Timestamps {
            last_access: rfs::Timespec {
                tv_sec: seconds,
                tv_nsec: nanoseconds,
            },
            last_modification: rfs::Timespec {
                tv_sec: seconds,
                tv_nsec: nanoseconds,
            },
        },
    )
    .expect("set fixture timestamps");
}

pub fn set_xattr(path: &Path, name: &[u8], value: &[u8]) {
    let file = if path.is_dir() {
        File::open(path).expect("open fixture directory")
    } else {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .expect("open fixture file")
    };
    rfs::fsetxattr(
        &file,
        OsStr::from_bytes(name),
        value,
        XattrFlags::empty(),
    )
    .expect("set fixture xattr");
}

pub fn get_xattr(path: &Path, name: &[u8]) -> Vec<u8> {
    let file = File::open(path).expect("open fixture file");
    let mut value = vec![0; 4096];
    let length = rfs::fgetxattr(&file, OsStr::from_bytes(name), &mut value)
        .expect("read fixture xattr");
    value.truncate(length);
    value
}

pub fn access_acl(owner: u16, named_user: u16, group: u16, mask: u16, other: u16) -> Vec<u8> {
    let entries = [
        (0x01_u16, owner, u32::MAX),
        (0x02, named_user, 1234),
        (0x04, group, u32::MAX),
        (0x10, mask, u32::MAX),
        (0x20, other, u32::MAX),
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

pub fn unsupported_errno(error: Errno) -> bool {
    matches!(
        error,
        Errno::NOSYS | Errno::NOTSUP | Errno::XDEV | Errno::INVAL | Errno::NOTTY
    )
}

pub fn try_set_user_xattr(fd: std::os::fd::BorrowedFd<'_>, name: &[u8], value: &[u8]) -> bool {
    match rfs::fsetxattr(fd, OsStr::from_bytes(name), value, XattrFlags::empty()) {
        Ok(()) => true,
        Err(error) if unsupported_errno(error) || error == Errno::PERM => {
            eprintln!("native Linux xattr unavailable on fixture filesystem: {error}");
            false
        }
        Err(error) => panic!("set source xattr: {error}"),
    }
}
