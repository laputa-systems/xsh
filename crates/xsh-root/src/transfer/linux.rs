use std::collections::VecDeque;
use std::ffi::{OsStr, OsString};
use std::io;
use std::mem::size_of;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Condvar, Mutex};

use rustix::fs::{
    self, AtFlags, FileType, Gid, IFlags, Mode, OFlags, SeekFrom, StatxFlags, Timespec, Timestamps,
    Uid, XattrFlags,
};
use rustix::io::Errno;

pub(super) mod engine;
pub(super) mod journal;
pub(super) mod policy;
pub(super) mod stage;
pub(super) mod sync;

pub(super) use engine::worker_count;
pub(super) use policy::preflight;
pub(super) use stage::{apply_deferred_flags, merge, publish};
pub(super) use sync::sync;

use super::{
    Cancellation, CopyMethod, CopyStats, DeferredFlags, DryRunEntry, EngineOptions, EntryIdentity,
    Metadata, MetadataTarget, MountIdentity, Observation, PolicyOptions, Preservation, Progress,
    Action, LiveProgress, Target, Timestamp, TreeTransfer, Xattr, advance_live, decide,
    TRANSFER_BUFFER_BYTES,
};
use crate::FileKind;
use journal::Session;

const COPY_RANGE_CHUNK: usize = 8 * 1024 * 1024;
const XATTR_CAPTURE_ATTEMPTS: usize = 8;
const METADATA_CAPTURE_BYTES: usize = 16 * 1024 * 1024;
const ALLOCATION_OVERHEAD: usize = 64;
const ZERO_SCAN_WINDOW_BYTES: usize = 4 * 1024;
const POSIX_ACL_ACCESS: &[u8] = b"system.posix_acl_access";
const SECURITY_CAPABILITY: &[u8] = b"security.capability";
const BLOCKING_FLAGS: u32 = IFlags::APPEND.bits() | IFlags::IMMUTABLE.bits();
const STRUCTURAL_FLAGS: u32 = 0x0000_0100
    | 0x0000_0200
    | 0x0000_1000
    | 0x0000_2000
    | 0x0004_0000
    | 0x0008_0000
    | 0x0020_0000
    | 0x0040_0000;
const KNOWN_MUTABLE_FLAGS: u32 = IFlags::APPEND.bits()
    | IFlags::COMPRESSED.bits()
    | IFlags::DIRSYNC.bits()
    | IFlags::IMMUTABLE.bits()
    | IFlags::JOURNALING.bits()
    | IFlags::NOATIME.bits()
    | IFlags::NOCOW.bits()
    | IFlags::NODUMP.bits()
    | IFlags::NOTAIL.bits()
    | IFlags::PROJECT_INHERIT.bits()
    | IFlags::SECURE_REMOVAL.bits()
    | IFlags::SYNC.bits()
    | IFlags::TOPDIR.bits()
    | IFlags::UNRM.bits();

const DIRECTORY_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::CLOEXEC)
    .union(OFlags::NOFOLLOW);
const READ_FILE_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC)
    .union(OFlags::NONBLOCK);
const WRITE_NEW_FLAGS: OFlags = OFlags::RDWR
    .union(OFlags::CREATE)
    .union(OFlags::EXCL)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);

fn observation_from_stat(stat: &fs::Stat) -> Observation {
    let mode = stat.st_mode;
    Observation {
        identity: EntryIdentity {
            device: stat.st_dev,
            inode: stat.st_ino,
            kind: crate::mutate::file_kind(FileType::from_raw_mode(mode as _)),
        },
        mode: mode & 0o7777,
        uid: stat.st_uid,
        gid: stat.st_gid,
        links: stat.st_nlink,
        size: u64::try_from(stat.st_size).unwrap_or(0),
        atime: Timestamp {
            seconds: stat.st_atime,
            nanoseconds: stat.st_atime_nsec as u32,
        },
        mtime: Timestamp {
            seconds: stat.st_mtime,
            nanoseconds: stat.st_mtime_nsec as u32,
        },
        ctime: Timestamp {
            seconds: stat.st_ctime,
            nanoseconds: stat.st_ctime_nsec as u32,
        },
    }
}

pub(super) fn observe(parent: BorrowedFd<'_>, name: &OsStr) -> io::Result<Observation> {
    fs::statat(parent, name, AtFlags::SYMLINK_NOFOLLOW)
        .map(|stat| observation_from_stat(&stat))
        .map_err(Into::into)
}

pub(super) fn observe_fd(fd: BorrowedFd<'_>) -> io::Result<Observation> {
    fs::fstat(fd)
        .map(|stat| observation_from_stat(&stat))
        .map_err(Into::into)
}

pub(super) fn directory_state(
    fd: BorrowedFd<'_>,
) -> io::Result<(EntryIdentity, MountIdentity)> {
    let wanted = StatxFlags::INO | StatxFlags::MNT_ID;
    let metadata = loop {
        match fs::statx(fd, c"", AtFlags::EMPTY_PATH, wanted) {
            Err(Errno::INTR) => continue,
            result => break result?,
        }
    };
    if !StatxFlags::from_bits_retain(metadata.stx_mask).contains(wanted) {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "kernel did not report the directory identity and mount ID",
        ));
    }
    let device = fs::makedev(metadata.stx_dev_major, metadata.stx_dev_minor);
    Ok((
        EntryIdentity {
            device,
            inode: metadata.stx_ino,
            kind: FileKind::Directory,
        },
        MountIdentity {
            device,
            mount_id: metadata.stx_mnt_id,
        },
    ))
}

pub(super) fn mount_identity(fd: BorrowedFd<'_>) -> io::Result<MountIdentity> {
    directory_state(fd).map(|(_, mount)| mount)
}

fn require_unchanged(
    parent: BorrowedFd<'_>,
    name: &OsStr,
    expected: Observation,
) -> io::Result<()> {
    if expected.same_version(observe(parent, name)?) {
        Ok(())
    } else {
        Err(io::Error::other("source identity or version changed"))
    }
}

fn require_unchanged_fd(expected: Observation, fd: BorrowedFd<'_>) -> io::Result<()> {
    if expected.same_version(observe_fd(fd)?) {
        Ok(())
    } else {
        Err(io::Error::other("source identity or version changed"))
    }
}

fn metadata_from(
    observed: Observation,
    xattrs: Vec<Xattr>,
    acl: Option<Vec<u8>>,
    flags: u32,
) -> Metadata {
    Metadata {
        kind: observed.identity.kind,
        mode: observed.mode,
        uid: observed.uid,
        gid: observed.gid,
        links: observed.links,
        size: observed.size,
        atime: observed.atime,
        mtime: observed.mtime,
        ctime: observed.ctime,
        xattrs,
        acl,
        flags,
    }
}

pub(super) fn capture(target: MetadataTarget<'_>) -> io::Result<Metadata> {
    match target {
        MetadataTarget::Fd(fd) => {
            let observed = observe_fd(fd)?;
            let (xattrs, acl) = read_fd_xattrs(fd)?;
            let flags = capture_flags(fd)?;
            Ok(metadata_from(observed, xattrs, acl, flags))
        }
        MetadataTarget::Symlink { parent, name } => {
            let before = observe(parent, name)?;
            let path = proc_fd_child(parent, name)?;
            let (xattrs, acl) = read_path_xattrs(&path)?;
            let after = observe(parent, name)?;
            ensure_stable_link_metadata(before, after)?;
            Ok(metadata_from(before, xattrs, acl, 0))
        }
    }
}

pub(super) fn apply(
    target: MetadataTarget<'_>,
    metadata: &Metadata,
    preservation: Preservation,
) -> io::Result<()> {
    match target {
        MetadataTarget::Fd(fd) => apply_fd(fd, metadata, preservation),
        MetadataTarget::Symlink { parent, name } => {
            apply_symlink(parent, name, metadata, preservation)
        }
    }
}

pub(super) fn apply_final(target: MetadataTarget<'_>, metadata: &Metadata) -> io::Result<()> {
    let flags = metadata.flags & BLOCKING_FLAGS;
    if flags == 0 {
        return Ok(());
    }
    match target {
        MetadataTarget::Fd(fd) => set_inode_flags(fd, metadata.flags),
        MetadataTarget::Symlink { .. } => Err(unsupported_metadata(
            "Linux cannot apply inode flags to symlinks",
        )),
    }
}

fn apply_fd(
    fd: BorrowedFd<'_>,
    metadata: &Metadata,
    preservation: Preservation,
) -> io::Result<()> {
    let destination = observe_fd(fd)?;
    let copy_owner_matches = destination.uid == metadata.uid && destination.gid == metadata.gid;
    if preservation == Preservation::Move && !copy_owner_matches {
        fs::fchown(
            fd,
            Some(Uid::from_raw(metadata.uid)),
            Some(Gid::from_raw(metadata.gid)),
        )?;
    }

    remove_unpreserved_fd_xattrs(fd, metadata, preservation, copy_owner_matches)?;

    for xattr in &metadata.xattrs {
        if preservation == Preservation::Copy
            && !copy_owner_matches
            && xattr.name.as_bytes() == SECURITY_CAPABILITY
        {
            continue;
        }
        fs::fsetxattr(fd, &xattr.name, &xattr.value, XattrFlags::empty())?;
    }
    if let Some(acl) = &metadata.acl {
        fs::fsetxattr(fd, POSIX_ACL_ACCESS, acl, XattrFlags::empty())?;
    }

    let mut mode = metadata.mode & 0o7777;
    if preservation == Preservation::Copy && !copy_owner_matches {
        mode &= !0o6000;
    }
    fs::fchmod(fd, Mode::from_raw_mode(mode as _))?;
    fs::futimens(
        fd,
        &Timestamps {
            last_access: timespec(metadata.atime),
            last_modification: timespec(metadata.mtime),
        },
    )?;
    apply_prepublication_flags(fd, metadata.flags)
}

fn apply_symlink(
    parent: BorrowedFd<'_>,
    name: &OsStr,
    metadata: &Metadata,
    preservation: Preservation,
) -> io::Result<()> {
    let before = observe(parent, name)?;
    if metadata.flags != 0 {
        return Err(unsupported_metadata(
            "Linux cannot preserve inode flags on symlinks",
        ));
    }
    if preservation == Preservation::Move
        && (before.uid != metadata.uid || before.gid != metadata.gid)
    {
        fs::chownat(
            parent,
            name,
            Some(Uid::from_raw(metadata.uid)),
            Some(Gid::from_raw(metadata.gid)),
            AtFlags::SYMLINK_NOFOLLOW,
        )?;
    }
    if metadata.acl.is_some() {
        return Err(unsupported_metadata(
            "Linux does not support POSIX ACLs on symlinks",
        ));
    }
    let copy_owner_matches = before.uid == metadata.uid && before.gid == metadata.gid;
    let path = proc_fd_child(parent, name)?;
    remove_unpreserved_path_xattrs(&path, metadata, preservation, copy_owner_matches)?;
    for xattr in &metadata.xattrs {
        if preservation == Preservation::Copy
            && !copy_owner_matches
            && xattr.name.as_bytes() == SECURITY_CAPABILITY
        {
            continue;
        }
        fs::lsetxattr(&path, &xattr.name, &xattr.value, XattrFlags::empty())?;
        ensure_link_identity(before, observe(parent, name)?)?;
    }
    fs::utimensat(
        parent,
        name,
        &Timestamps {
            last_access: timespec(metadata.atime),
            last_modification: timespec(metadata.mtime),
        },
        AtFlags::SYMLINK_NOFOLLOW,
    )?;
    ensure_link_identity(before, observe(parent, name)?)
}

fn timespec(timestamp: Timestamp) -> Timespec {
    Timespec {
        tv_sec: timestamp.seconds,
        tv_nsec: timestamp.nanoseconds.into(),
    }
}

fn ensure_stable_link_metadata(before: Observation, after: Observation) -> io::Result<()> {
    if before.same_version(after) {
        Ok(())
    } else {
        Err(io::Error::other("symlink changed while capturing metadata"))
    }
}

fn ensure_link_identity(before: Observation, after: Observation) -> io::Result<()> {
    if before.same_identity(after) {
        Ok(())
    } else {
        Err(io::Error::other(
            "symlink identity changed while applying metadata",
        ))
    }
}

fn remove_unpreserved_fd_xattrs(
    fd: BorrowedFd<'_>,
    metadata: &Metadata,
    preservation: Preservation,
    owner_matches: bool,
) -> io::Result<()> {
    let mut budget = CaptureBudget::new(METADATA_CAPTURE_BYTES);
    let names = list_xattr_names(|buffer| fd_list_xattrs(fd, buffer), &mut budget)?;
    visit_xattr_names(&names, |name| {
        let preserved = metadata
            .xattrs
            .iter()
            .any(|xattr| xattr.name.as_bytes() == name)
            && !(preservation == Preservation::Copy
                && !owner_matches
                && name == SECURITY_CAPABILITY);
        if !preserved {
            match fs::fremovexattr(fd, OsStr::from_bytes(name)) {
                Ok(()) | Err(Errno::NODATA) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    })
}

fn remove_unpreserved_path_xattrs(
    path: &OsStr,
    metadata: &Metadata,
    preservation: Preservation,
    owner_matches: bool,
) -> io::Result<()> {
    let mut budget = CaptureBudget::new(METADATA_CAPTURE_BYTES);
    let names = list_xattr_names(|buffer| path_list_xattrs(path, buffer), &mut budget)?;
    visit_xattr_names(&names, |name| {
        let preserved = metadata
            .xattrs
            .iter()
            .any(|xattr| xattr.name.as_bytes() == name)
            && !(preservation == Preservation::Copy
                && !owner_matches
                && name == SECURITY_CAPABILITY);
        if !preserved {
            match fs::lremovexattr(path, OsStr::from_bytes(name)) {
                Ok(()) | Err(Errno::NODATA) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    })
}

struct CaptureBudget {
    used: usize,
    limit: usize,
}

impl CaptureBudget {
    fn new(limit: usize) -> Self {
        Self { used: 0, limit }
    }

    fn allocation_charge(capacity: usize) -> io::Result<usize> {
        if capacity == 0 {
            return Ok(0);
        }
        capacity.checked_add(ALLOCATION_OVERHEAD).ok_or_else(too_large)
    }

    fn check(&self, capacity: usize) -> io::Result<()> {
        let charge = Self::allocation_charge(capacity)?;
        if self.used.checked_add(charge).is_none_or(|used| used > self.limit) {
            return Err(too_large());
        }
        Ok(())
    }

    fn record(&mut self, capacity: usize) -> io::Result<()> {
        self.check(capacity)?;
        self.used += Self::allocation_charge(capacity)?;
        Ok(())
    }
}

fn too_large() -> io::Error {
    io::Error::new(
        io::ErrorKind::FileTooLarge,
        "extended metadata exceeds the platform capture limit",
    )
}

fn allocation_error(error: std::collections::TryReserveError) -> io::Error {
    io::Error::new(io::ErrorKind::OutOfMemory, error)
}

fn fd_list_xattrs(fd: BorrowedFd<'_>, buffer: &mut [u8]) -> rustix::io::Result<usize> {
    if buffer.is_empty() {
        let mut empty: [u8; 0] = [];
        fs::flistxattr(fd, &mut empty[..])
    } else {
        fs::flistxattr(fd, buffer)
    }
}

fn fd_get_xattr(
    fd: BorrowedFd<'_>,
    name: &OsStr,
    buffer: &mut [u8],
) -> rustix::io::Result<usize> {
    if buffer.is_empty() {
        let mut empty: [u8; 0] = [];
        fs::fgetxattr(fd, name, &mut empty[..])
    } else {
        fs::fgetxattr(fd, name, buffer)
    }
}

fn path_list_xattrs(path: &OsStr, buffer: &mut [u8]) -> rustix::io::Result<usize> {
    if buffer.is_empty() {
        let mut empty: [u8; 0] = [];
        fs::llistxattr(path, &mut empty[..])
    } else {
        fs::llistxattr(path, buffer)
    }
}

fn path_get_xattr(
    path: &OsStr,
    name: &OsStr,
    buffer: &mut [u8],
) -> rustix::io::Result<usize> {
    if buffer.is_empty() {
        let mut empty: [u8; 0] = [];
        fs::lgetxattr(path, name, &mut empty[..])
    } else {
        fs::lgetxattr(path, name, buffer)
    }
}

fn read_fd_xattrs(fd: BorrowedFd<'_>) -> io::Result<(Vec<Xattr>, Option<Vec<u8>>)> {
    read_xattrs(
        |buffer| fd_list_xattrs(fd, buffer),
        |name, buffer| fd_get_xattr(fd, name, buffer),
    )
}

fn read_path_xattrs(path: &OsStr) -> io::Result<(Vec<Xattr>, Option<Vec<u8>>)> {
    read_xattrs(
        |buffer| path_list_xattrs(path, buffer),
        |name, buffer| path_get_xattr(path, name, buffer),
    )
}

fn read_xattrs(
    mut list: impl FnMut(&mut [u8]) -> rustix::io::Result<usize>,
    mut get: impl FnMut(&OsStr, &mut [u8]) -> rustix::io::Result<usize>,
) -> io::Result<(Vec<Xattr>, Option<Vec<u8>>)> {
    let mut budget = CaptureBudget::new(METADATA_CAPTURE_BYTES);
    let names = list_xattr_names(&mut list, &mut budget)?;
    let mut xattr_count = 0usize;
    visit_xattr_names(&names, |name| {
        if name != POSIX_ACL_ACCESS {
            xattr_count = xattr_count.checked_add(1).ok_or_else(too_large)?;
        }
        Ok(())
    })?;

    let xattr_storage = xattr_count
        .checked_mul(size_of::<Xattr>())
        .ok_or_else(too_large)?;
    budget.check(xattr_storage)?;
    let mut xattrs = Vec::new();
    xattrs.try_reserve_exact(xattr_count).map_err(allocation_error)?;
    budget.record(
        xattrs
            .capacity()
            .checked_mul(size_of::<Xattr>())
            .ok_or_else(too_large)?,
    )?;

    let mut acl = None;
    visit_xattr_names(&names, |raw_name| {
        let name = OsStr::from_bytes(raw_name);
        let value = read_xattr_value(|buffer| get(name, buffer), &mut budget)?;
        if raw_name == POSIX_ACL_ACCESS {
            acl = Some(value);
        } else {
            xattrs.push(Xattr {
                name: copy_xattr_name(raw_name, &mut budget)?,
                value,
            });
        }
        Ok(())
    })?;
    Ok((xattrs, acl))
}

fn list_xattr_names(
    mut list: impl FnMut(&mut [u8]) -> rustix::io::Result<usize>,
    budget: &mut CaptureBudget,
) -> io::Result<Vec<u8>> {
    for _ in 0..XATTR_CAPTURE_ATTEMPTS {
        let capacity = match list(&mut []) {
            Ok(capacity) => capacity,
            Err(Errno::NOTSUP) => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        if capacity == 0 {
            return Ok(Vec::new());
        }
        budget.check(capacity)?;
        let mut buffer = Vec::new();
        buffer.try_reserve_exact(capacity).map_err(allocation_error)?;
        buffer.resize(capacity, 0);
        match list(&mut buffer) {
            Ok(length) if length <= capacity => {
                buffer.truncate(length);
                budget.record(buffer.capacity())?;
                return Ok(buffer);
            }
            Ok(_) | Err(Errno::RANGE) => {}
            Err(Errno::NOTSUP) => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::WouldBlock,
        "xattr names changed during repeated capture attempts",
    ))
}

fn read_xattr_value(
    mut get: impl FnMut(&mut [u8]) -> rustix::io::Result<usize>,
    budget: &mut CaptureBudget,
) -> io::Result<Vec<u8>> {
    for _ in 0..XATTR_CAPTURE_ATTEMPTS {
        let capacity = get(&mut [])?;
        if capacity == 0 {
            return Ok(Vec::new());
        }
        budget.check(capacity)?;
        let mut value = Vec::new();
        value.try_reserve_exact(capacity).map_err(allocation_error)?;
        value.resize(capacity, 0);
        match get(&mut value) {
            Ok(length) if length <= capacity => {
                value.truncate(length);
                budget.record(value.capacity())?;
                return Ok(value);
            }
            Ok(_) | Err(Errno::RANGE) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::WouldBlock,
        "xattr value changed during repeated capture attempts",
    ))
}

fn copy_xattr_name(name: &[u8], budget: &mut CaptureBudget) -> io::Result<OsString> {
    budget.check(name.len())?;
    let mut owned = Vec::new();
    owned.try_reserve_exact(name.len()).map_err(allocation_error)?;
    owned.extend_from_slice(name);
    budget.record(owned.capacity())?;
    Ok(OsString::from_vec(owned))
}

fn visit_xattr_names(
    names: &[u8],
    mut visit: impl FnMut(&[u8]) -> io::Result<()>,
) -> io::Result<()> {
    let mut start = 0;
    while start < names.len() {
        let end = names[start..]
            .iter()
            .position(|byte| *byte == 0)
            .map(|relative| start + relative)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "filesystem returned malformed xattr names",
                )
            })?;
        if end == start {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "filesystem returned an empty xattr name",
            ));
        }
        visit(&names[start..end])?;
        start = end + 1;
    }
    Ok(())
}

fn proc_fd_child(parent: BorrowedFd<'_>, name: &OsStr) -> io::Result<OsString> {
    let bytes = name.as_bytes();
    if bytes.is_empty()
        || bytes.contains(&0)
        || bytes.contains(&b'/')
        || bytes == b"."
        || bytes == b".."
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "symlink metadata name is not one directory entry",
        ));
    }
    let mut path = format!("/proc/self/fd/{}/", parent.as_raw_fd()).into_bytes();
    path.extend_from_slice(bytes);
    Ok(OsString::from_vec(path))
}

fn capture_flags(fd: BorrowedFd<'_>) -> io::Result<u32> {
    match fs::ioctl_getflags(fd) {
        Ok(flags) => Ok(flags.bits()),
        Err(error) if flags_unsupported(error) => Ok(0),
        Err(error) => Err(error.into()),
    }
}

fn apply_prepublication_flags(fd: BorrowedFd<'_>, source_flags: u32) -> io::Result<()> {
    validate_flag_set(source_flags)?;
    let flags = source_flags & !BLOCKING_FLAGS & !STRUCTURAL_FLAGS;
    set_inode_flags(fd, flags)
}

fn set_inode_flags(fd: BorrowedFd<'_>, source_flags: u32) -> io::Result<()> {
    validate_flag_set(source_flags)?;
    let current = match fs::ioctl_getflags(fd) {
        Ok(flags) => flags.bits(),
        Err(error) if flags_unsupported(error) => {
            if source_flags & !STRUCTURAL_FLAGS == 0 {
                return Ok(());
            }
            return Err(unsupported_metadata(
                "destination filesystem does not support inode flags",
            ));
        }
        Err(error) => return Err(error.into()),
    };
    let flags = (current & STRUCTURAL_FLAGS) | (source_flags & !STRUCTURAL_FLAGS);
    match fs::ioctl_setflags(fd, IFlags::from_bits_retain(flags)) {
        Ok(()) => Ok(()),
        Err(error) if flags_unsupported(error) => Err(unsupported_metadata(
            "destination filesystem cannot represent source inode flags",
        )),
        Err(error) => Err(error.into()),
    }
}

fn validate_flag_set(flags: u32) -> io::Result<()> {
    if flags & !(KNOWN_MUTABLE_FLAGS | STRUCTURAL_FLAGS) != 0 {
        return Err(unsupported_metadata(
            "source has inode flags that Linux cannot preserve",
        ));
    }
    Ok(())
}

fn flags_unsupported(error: Errno) -> bool {
    matches!(error, Errno::NOSYS | Errno::NOTSUP | Errno::NOTTY | Errno::INVAL)
}

fn unsupported_metadata(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, message)
}

enum AccelerationError {
    Unsupported,
    Io(Errno),
}

enum SparseError {
    Unsupported,
    Io(io::Error),
}

fn try_reflink(source: BorrowedFd<'_>, destination: BorrowedFd<'_>) -> Result<(), AccelerationError> {
    loop {
        match fs::ioctl_ficlone(destination, source) {
            Ok(()) => return Ok(()),
            Err(Errno::INTR) => continue,
            Err(error) if reflink_unsupported(error) => {
                return Err(AccelerationError::Unsupported);
            }
            Err(error) => return Err(AccelerationError::Io(error)),
        }
    }
}

fn reflink_unsupported(error: Errno) -> bool {
    matches!(
        error,
        Errno::XDEV | Errno::NOSYS | Errno::NOTSUP | Errno::NOTTY | Errno::INVAL
    )
}

fn copy_range_unsupported(error: Errno) -> bool {
    matches!(error, Errno::XDEV | Errno::NOSYS | Errno::NOTSUP | Errno::INVAL)
}

fn sparse_seek_unsupported(error: Errno) -> bool {
    matches!(error, Errno::NOSYS | Errno::NOTSUP | Errno::INVAL)
}

pub(super) fn copy_data(
    source: BorrowedFd<'_>,
    destination: BorrowedFd<'_>,
    length: u64,
    scratch: &mut [u8],
    cancellation: &Cancellation,
) -> io::Result<CopyStats> {
    let source_offset = seek_retry(source, SeekFrom::Current(0))?;
    let result = copy_data_at(source, destination, length, scratch, cancellation);
    let restore = seek_retry(source, SeekFrom::Start(source_offset));
    match (result, restore) {
        (Ok(stats), Ok(_)) => Ok(stats),
        (Err(error), Ok(_)) => Err(error),
        (Ok(_), Err(error)) => Err(error.into()),
        (Err(error), Err(restore_error)) => Err(io::Error::new(
            error.kind(),
            format!("{error}; restoring the source offset also failed: {restore_error}"),
        )),
    }
}

fn copy_stats(method: CopyMethod, length: u64, streamed: u64) -> CopyStats {
    CopyStats {
        method,
        logical_bytes: length,
        streamed_bytes: streamed,
        hole_bytes: length.saturating_sub(streamed),
    }
}

fn copy_data_at(
    source: BorrowedFd<'_>,
    destination: BorrowedFd<'_>,
    length: u64,
    scratch: &mut [u8],
    cancellation: &Cancellation,
) -> io::Result<CopyStats> {
    if scratch.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "copy buffer is empty",
        ));
    }
    cancellation.check()?;

    let source_stat = fs::fstat(source)?;
    let destination_stat = fs::fstat(destination)?;
    if source_stat.st_dev == destination_stat.st_dev
        && source_stat.st_ino == destination_stat.st_ino
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "source and destination refer to the same file",
        ));
    }
    if source_stat.st_size < 0 || (source_stat.st_size as u64) < length {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "source is shorter than the captured length",
        ));
    }

    if length == 0 {
        fs::ftruncate(destination, 0)?;
        return Ok(copy_stats(CopyMethod::Native, 0, 0));
    }

    match try_reflink(source, destination) {
        Ok(()) => {
            cancellation.check()?;
            let cloned_size = fs::fstat(destination)?.st_size;
            if cloned_size < 0 || (cloned_size as u64) < length {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "reflink source ended before the captured length",
                ));
            }
            fs::ftruncate(destination, length)?;
            return Ok(copy_stats(CopyMethod::Clone, length, 0));
        }
        Err(AccelerationError::Unsupported) => fs::ftruncate(destination, 0)?,
        Err(AccelerationError::Io(error)) => return Err(error.into()),
    }

    match copy_sparse_native(source, destination, length, cancellation) {
        Ok(streamed) => {
            verify_source_length(source, length)?;
            fs::ftruncate(destination, length)?;
            return Ok(copy_stats(CopyMethod::Native, length, streamed));
        }
        Err(SparseError::Unsupported) => fs::ftruncate(destination, 0)?,
        Err(SparseError::Io(error)) => return Err(error),
    }

    let streamed = match copy_sparse_buffered(source, destination, length, scratch, cancellation) {
        Ok(streamed) => streamed,
        Err(SparseError::Unsupported) => {
            fs::ftruncate(destination, 0)?;
            copy_zero_scanning(source, destination, length, scratch, cancellation)?
        }
        Err(SparseError::Io(error)) => return Err(error),
    };
    verify_source_length(source, length)?;
    fs::ftruncate(destination, length)?;
    Ok(copy_stats(CopyMethod::Buffered, length, streamed))
}

fn verify_source_length(source: BorrowedFd<'_>, length: u64) -> io::Result<()> {
    let stat = fs::fstat(source)?;
    if stat.st_size < 0 || (stat.st_size as u64) < length {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "source is shorter than the captured length",
        ));
    }
    Ok(())
}

fn seek_retry(fd: BorrowedFd<'_>, position: SeekFrom) -> rustix::io::Result<u64> {
    loop {
        match fs::seek(fd, position) {
            Err(Errno::INTR) => continue,
            result => return result,
        }
    }
}

fn copy_sparse_native(
    source: BorrowedFd<'_>,
    destination: BorrowedFd<'_>,
    length: u64,
    cancellation: &Cancellation,
) -> Result<u64, SparseError> {
    let mut offset = 0;
    let mut streamed = 0;
    while offset < length {
        cancellation.check().map_err(SparseError::Io)?;
        let data = match seek_sparse(source, SeekFrom::Data(offset)) {
            Ok(data) => data,
            Err(SparseSeekError::End) => break,
            Err(SparseSeekError::Unsupported) => return Err(SparseError::Unsupported),
            Err(SparseSeekError::Io(error)) => return Err(SparseError::Io(error)),
        };
        if data >= length {
            break;
        }
        let hole = match seek_sparse(source, SeekFrom::Hole(data)) {
            Ok(hole) => hole.min(length),
            Err(SparseSeekError::End) => length,
            Err(SparseSeekError::Unsupported) => return Err(SparseError::Unsupported),
            Err(SparseSeekError::Io(error)) => return Err(SparseError::Io(error)),
        };
        if hole <= data {
            return Err(SparseError::Io(io::Error::new(
                io::ErrorKind::InvalidData,
                "sparse extent did not advance",
            )));
        }
        let mut input_offset = data;
        let mut output_offset = data;
        while output_offset < hole {
            cancellation.check().map_err(SparseError::Io)?;
            let count = usize::try_from((hole - output_offset).min(COPY_RANGE_CHUNK as u64))
                .expect("copy range is bounded by its chunk size");
            match copy_file_range(
                source,
                &mut input_offset,
                destination,
                &mut output_offset,
                count,
            ) {
                Ok(0) => {
                    return Err(SparseError::Io(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "source ended before the captured length",
                    )));
                }
                Ok(copied) => streamed += copied as u64,
                Err(error) if copy_range_unsupported(error) => {
                    return Err(SparseError::Unsupported);
                }
                Err(error) => return Err(SparseError::Io(error.into())),
            }
        }
        offset = hole;
    }
    Ok(streamed)
}

fn copy_file_range(
    source: BorrowedFd<'_>,
    input_offset: &mut u64,
    destination: BorrowedFd<'_>,
    output_offset: &mut u64,
    count: usize,
) -> Result<usize, Errno> {
    loop {
        match fs::copy_file_range(
            source,
            Some(input_offset),
            destination,
            Some(output_offset),
            count,
        ) {
            Err(Errno::INTR) => continue,
            result => return result,
        }
    }
}

enum SparseSeekError {
    End,
    Unsupported,
    Io(io::Error),
}

fn seek_sparse(fd: BorrowedFd<'_>, position: SeekFrom) -> Result<u64, SparseSeekError> {
    loop {
        match fs::seek(fd, position) {
            Ok(offset) => return Ok(offset),
            Err(Errno::NXIO) => return Err(SparseSeekError::End),
            Err(error) if sparse_seek_unsupported(error) => {
                return Err(SparseSeekError::Unsupported);
            }
            Err(Errno::INTR) => continue,
            Err(error) => return Err(SparseSeekError::Io(error.into())),
        }
    }
}

fn copy_sparse_buffered(
    source: BorrowedFd<'_>,
    destination: BorrowedFd<'_>,
    length: u64,
    scratch: &mut [u8],
    cancellation: &Cancellation,
) -> Result<u64, SparseError> {
    let mut offset = 0;
    let mut streamed = 0;
    while offset < length {
        cancellation.check().map_err(SparseError::Io)?;
        let data = match seek_sparse(source, SeekFrom::Data(offset)) {
            Ok(data) => data,
            Err(SparseSeekError::End) => break,
            Err(SparseSeekError::Unsupported) => return Err(SparseError::Unsupported),
            Err(SparseSeekError::Io(error)) => return Err(SparseError::Io(error)),
        };
        if data >= length {
            break;
        }
        let hole = match seek_sparse(source, SeekFrom::Hole(data)) {
            Ok(hole) => hole.min(length),
            Err(SparseSeekError::End) => length,
            Err(SparseSeekError::Unsupported) => return Err(SparseError::Unsupported),
            Err(SparseSeekError::Io(error)) => return Err(SparseError::Io(error)),
        };
        if hole <= data {
            return Err(SparseError::Io(io::Error::new(
                io::ErrorKind::InvalidData,
                "sparse extent did not advance",
            )));
        }
        streamed += copy_buffered_range(source, destination, data, hole, scratch, cancellation)
            .map_err(SparseError::Io)?;
        offset = hole;
    }
    Ok(streamed)
}

fn copy_zero_scanning(
    source: BorrowedFd<'_>,
    destination: BorrowedFd<'_>,
    length: u64,
    scratch: &mut [u8],
    cancellation: &Cancellation,
) -> io::Result<u64> {
    let mut offset = 0;
    let mut streamed = 0;
    while offset < length {
        cancellation.check()?;
        let wanted = usize::try_from((length - offset).min(scratch.len() as u64))
            .expect("buffered read is bounded by its buffer");
        let read = pread_retry(source, &mut scratch[..wanted], offset, cancellation)?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "source ended before the captured length",
            ));
        }
        visit_nonzero_ranges(&scratch[..read], |range| {
            pwrite_all(
                destination,
                &scratch[range.clone()],
                offset + range.start as u64,
                cancellation,
            )?;
            streamed += range.len() as u64;
            Ok(())
        })?;
        offset += read as u64;
    }
    Ok(streamed)
}

fn copy_buffered_range(
    source: BorrowedFd<'_>,
    destination: BorrowedFd<'_>,
    start: u64,
    end: u64,
    scratch: &mut [u8],
    cancellation: &Cancellation,
) -> io::Result<u64> {
    let mut offset = start;
    while offset < end {
        cancellation.check()?;
        let wanted = usize::try_from((end - offset).min(scratch.len() as u64))
            .expect("buffered read is bounded by its buffer");
        let read = pread_retry(source, &mut scratch[..wanted], offset, cancellation)?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "source ended before the captured length",
            ));
        }
        pwrite_all(destination, &scratch[..read], offset, cancellation)?;
        offset += read as u64;
    }
    Ok(end - start)
}

fn pread_retry(
    fd: BorrowedFd<'_>,
    buffer: &mut [u8],
    offset: u64,
    cancellation: &Cancellation,
) -> io::Result<usize> {
    loop {
        cancellation.check()?;
        match rustix::io::pread(fd, &mut *buffer, offset) {
            Err(Errno::INTR) => continue,
            result => return result.map_err(Into::into),
        }
    }
}

fn pwrite_all(
    fd: BorrowedFd<'_>,
    mut buffer: &[u8],
    mut offset: u64,
    cancellation: &Cancellation,
) -> io::Result<()> {
    while !buffer.is_empty() {
        cancellation.check()?;
        match rustix::io::pwrite(fd, buffer, offset) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "destination write made no progress",
                ));
            }
            Ok(written) => {
                buffer = &buffer[written..];
                offset += written as u64;
            }
            Err(Errno::INTR) => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn visit_nonzero_ranges(
    bytes: &[u8],
    mut visit: impl FnMut(std::ops::Range<usize>) -> io::Result<()>,
) -> io::Result<()> {
    let mut range_start = None;
    let mut range_end = 0;
    for window_start in (0..bytes.len()).step_by(ZERO_SCAN_WINDOW_BYTES) {
        let window_end = (window_start + ZERO_SCAN_WINDOW_BYTES).min(bytes.len());
        let window = &bytes[window_start..window_end];
        if let Some(first) = window.iter().position(|byte| *byte != 0) {
            if range_start.is_none() {
                range_start = Some(window_start + first);
            }
            let last = window
                .iter()
                .rposition(|byte| *byte != 0)
                .expect("a nonzero window has a final nonzero byte");
            range_end = window_start + last + 1;
        } else if let Some(start) = range_start.take() {
            visit(start..range_end)?;
        }
    }
    if let Some(start) = range_start {
        visit(start..range_end)?;
    }
    Ok(())
}

struct DirectoryRecord {
    relative: PathBuf,
    metadata: Metadata,
    published: EntryIdentity,
}

struct PublishedGroup {
    identity: EntryIdentity,
    relative: PathBuf,
}

#[derive(Default)]
struct GroupRegistry {
    groups: Mutex<Vec<((u64, u64), PublishedGroup)>>,
    ready: Condvar,
}

struct LeafWork {
    parent: PathBuf,
    source_name: OsString,
    destination_name: OsString,
    observed: Observation,
    copy: bool,
}

pub(super) fn check_recovery(fd: BorrowedFd<'_>) -> io::Result<()> {
    if Session::is_staging_root(fd)? {
        return Ok(());
    }
    Session::check_recovery(fd)
}

struct TransferContext<'a> {
    cancellation: &'a Cancellation,
    live: Option<&'a LiveProgress>,
    engine: EngineOptions,
    stop: AtomicBool,
    groups: GroupRegistry,
    source_root: OwnedFd,
    destination_root: OwnedFd,
    directories: Mutex<Vec<(PathBuf, EntryIdentity)>>,
    source_mount: MountIdentity,
    deferred: Mutex<Vec<DeferredFlags>>,
    failure: Mutex<Option<io::Error>>,
}

impl TransferContext<'_> {
    fn record_failure(&self, error: io::Error) {
        self.stop.store(true, Ordering::Release);
        let mut failure = self.failure.lock().unwrap();
        if failure.is_none() {
            *failure = Some(error);
        }
        drop(failure);
        self.groups.ready.notify_all();
    }

    fn checkpoint(&self) -> io::Result<()> {
        self.cancellation.check()?;
        if self.stop.load(Ordering::Acquire) {
            return Err(io::Error::other("transfer stopped after an error"));
        }
        Ok(())
    }

    fn transfer_leaf(
        &self,
        work: &LeafWork,
        buffer: &mut [u8],
        progress: &mut Progress,
    ) -> io::Result<()> {
        self.checkpoint()?;
        let source_parent = open_verified_directory(
            self.source_root.as_fd(),
            &work.parent,
            &self.directories,
        )?;
        let destination_parent = open_relative_directory(
            self.destination_root.as_fd(),
            &work.parent,
        )?;
        if !work.copy {
            link_member(
                self,
                work,
                source_parent.as_fd(),
                destination_parent.as_fd(),
            )?;
            progress.completed_files += 1;
            return Ok(());
        }
        let published = match work.observed.identity.kind {
            FileKind::File => copy_file_leaf(
                self,
                source_parent.as_fd(),
                &work.source_name,
                destination_parent.as_fd(),
                &work.destination_name,
                &work.parent.join(&work.destination_name),
                work.observed,
                buffer,
                progress,
            )?,
            FileKind::Symlink => copy_symlink_leaf(
                source_parent.as_fd(),
                &work.source_name,
                destination_parent.as_fd(),
                &work.destination_name,
                work.observed,
            )?,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "special objects cannot be transferred",
                ));
            }
        };
        if work.observed.links > 1 {
            let key = (work.observed.identity.device, work.observed.identity.inode);
            let mut groups = self.groups.groups.lock().unwrap();
            if let Some(entry) = groups.iter_mut().find(|(existing, _)| *existing == key) {
                entry.1 = PublishedGroup {
                    identity: published.identity,
                    relative: work.parent.join(&work.destination_name),
                };
            } else {
                groups.push((
                    key,
                    PublishedGroup {
                        identity: published.identity,
                        relative: work.parent.join(&work.destination_name),
                    },
                ));
            }
            drop(groups);
            self.groups.ready.notify_all();
        }
        progress.completed_files += 1;
        Ok(())
    }
}

fn link_member(
    context: &TransferContext<'_>,
    work: &LeafWork,
    source_parent: BorrowedFd<'_>,
    destination_parent: BorrowedFd<'_>,
) -> io::Result<()> {
    let key = (work.observed.identity.device, work.observed.identity.inode);
    let group = {
        let mut groups = context.groups.groups.lock().unwrap();
        loop {
            if let Some((_, group)) = groups.iter().find(|(existing, _)| *existing == key) {
                break PublishedGroup {
                    identity: group.identity,
                    relative: group.relative.clone(),
                };
            }
            context.cancellation.check()?;
            if context.stop.load(Ordering::Acquire) {
                return Err(io::Error::other(
                    "transfer stopped before its hardlink group",
                ));
            }
            let (next, _) = context
                .groups
                .ready
                .wait_timeout(groups, std::time::Duration::from_millis(20))
                .unwrap();
            groups = next;
        }
    };
    let representative_parent = group
        .relative
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let representative_name = group
        .relative
        .file_name()
        .ok_or_else(|| io::Error::other("hardlink representative has no name"))?
        .to_os_string();
    let parent = open_relative_directory(context.destination_root.as_fd(), representative_parent)?;
    let existing = observe(parent.as_fd(), &representative_name)?;
    if existing.identity != group.identity {
        return Err(io::Error::other("hardlink representative changed"));
    }
    context.checkpoint()?;
    fs::linkat(
        parent.as_fd(),
        &representative_name,
        destination_parent,
        &work.destination_name,
        AtFlags::empty(),
    )?;
    let linked = observe(destination_parent, &work.destination_name)?;
    if linked.identity != group.identity {
        let cleanup = fs::unlinkat(destination_parent, &work.destination_name, AtFlags::empty());
        return Err(with_cleanup(
            io::Error::other("hardlink identity changed while linking"),
            cleanup,
        ));
    }
    require_unchanged(source_parent, &work.source_name, work.observed)
}

fn open_verified_directory(
    root: BorrowedFd<'_>,
    relative: &Path,
    directories: &Mutex<Vec<(PathBuf, EntryIdentity)>>,
) -> io::Result<OwnedFd> {
    let directory = open_relative_directory(root, relative)?;
    let expected = directories
        .lock()
        .unwrap()
        .iter()
        .find(|(path, _)| path == relative)
        .map(|(_, identity)| *identity)
        .ok_or_else(|| io::Error::other("traversal directory has no identity record"))?;
    if observe_fd(directory.as_fd())?.identity != expected {
        return Err(io::Error::other("traversal directory identity changed"));
    }
    Ok(directory)
}

fn copy_file_leaf(
    context: &TransferContext<'_>,
    source_parent: BorrowedFd<'_>,
    source_name: &OsStr,
    destination_parent: BorrowedFd<'_>,
    destination_name: &OsStr,
    relative: &Path,
    observed: Observation,
    buffer: &mut [u8],
    progress: &mut Progress,
) -> io::Result<Observation> {
    context.checkpoint()?;
    let source = fs::openat(source_parent, source_name, READ_FILE_FLAGS, Mode::empty())?;
    if !observed.same_version(observe_fd(source.as_fd())?) {
        return Err(io::Error::other("source changed while opening"));
    }
    if !context.engine.cross_file_systems
        && mount_identity(source.as_fd())? != context.source_mount
    {
        return Err(io::Error::other(
            "nested source file mount requires --cross-file-systems",
        ));
    }
    let metadata = capture(MetadataTarget::Fd(source.as_fd()))?;
    let destination = fs::openat(
        destination_parent,
        destination_name,
        WRITE_NEW_FLAGS,
        Mode::from_raw_mode(0o600),
    )?;
    let result = (|| {
        let stats = copy_data(
            source.as_fd(),
            destination.as_fd(),
            observed.size,
            buffer,
            context.cancellation,
        )?;
        progress.record(stats);
        require_unchanged(source_parent, source_name, observed)?;
        apply(
            MetadataTarget::Fd(destination.as_fd()),
            &metadata,
            Preservation::Copy,
        )?;
        if context.engine.durable {
            sync_fd(destination.as_fd())?;
            progress.synced_files += 1;
        }
        observe_fd(destination.as_fd())
    })();
    match result {
        Ok(published) => {
            defer_flags(context, relative, &metadata);
            Ok(published)
        }
        Err(error) => {
            let cleanup = fs::unlinkat(destination_parent, destination_name, AtFlags::empty());
            Err(with_cleanup(error, cleanup))
        }
    }
}

fn copy_symlink_leaf(
    source_parent: BorrowedFd<'_>,
    source_name: &OsStr,
    destination_parent: BorrowedFd<'_>,
    destination_name: &OsStr,
    observed: Observation,
) -> io::Result<Observation> {
    let metadata = capture(MetadataTarget::Symlink {
        parent: source_parent,
        name: source_name,
    })?;
    let target = fs::readlinkat(source_parent, source_name, Vec::new())?;
    require_unchanged(source_parent, source_name, observed)?;
    fs::symlinkat(target.as_c_str(), destination_parent, destination_name)?;
    let result = apply(
        MetadataTarget::Symlink {
            parent: destination_parent,
            name: destination_name,
        },
        &metadata,
        Preservation::Copy,
    )
    .and_then(|()| require_unchanged(source_parent, source_name, observed));
    if let Err(error) = result {
        let cleanup = fs::unlinkat(destination_parent, destination_name, AtFlags::empty());
        return Err(with_cleanup(error, cleanup));
    }
    observe(destination_parent, destination_name)
}

fn defer_flags(context: &TransferContext<'_>, relative: &Path, metadata: &Metadata) {
    if metadata.flags & BLOCKING_FLAGS == 0 {
        return;
    }
    let mut deferred = context.deferred.lock().unwrap();
    if let Some(entry) = deferred.iter_mut().find(|entry| entry.relative == relative) {
        entry.flags |= metadata.flags;
    } else {
        deferred.push(DeferredFlags {
            relative: relative.to_path_buf(),
            flags: metadata.flags,
        });
    }
}

pub(super) fn sync_fd(fd: BorrowedFd<'_>) -> io::Result<()> {
    loop {
        match fs::fsync(fd) {
            Err(Errno::INTR) => continue,
            result => return result.map_err(Into::into),
        }
    }
}

fn with_cleanup(primary: io::Error, cleanup: rustix::io::Result<()>) -> io::Error {
    match cleanup {
        Ok(()) => primary,
        Err(error) => io::Error::new(
            primary.kind(),
            format!("{primary}; cleaning the destination also failed: {error}"),
        ),
    }
}

pub(super) fn transfer_tree(request: TreeTransfer<'_>) -> io::Result<()> {
    let TreeTransfer {
        source_parent,
        source_name,
        destination_parent,
        destination_name,
        engine,
        cancellation,
        progress,
        live,
        deferred_flags,
    } = request;
    validate_entry_name(source_name, "source")?;
    validate_entry_name(destination_name, "destination")?;
    cancellation.check()?;
    let result = run_tree(
        source_parent,
        source_name,
        destination_parent,
        destination_name,
        engine,
        cancellation,
        progress,
        live,
        deferred_flags,
    );
    if result.is_err() {
        progress.failed_files += 1;
    }
    result
}

fn run_tree(
    source_parent: BorrowedFd<'_>,
    source_name: &OsStr,
    destination_parent: BorrowedFd<'_>,
    destination_name: &OsStr,
    engine: EngineOptions,
    cancellation: &Cancellation,
    progress: &mut Progress,
    live: Option<&LiveProgress>,
    deferred_flags: &mut Vec<DeferredFlags>,
) -> io::Result<()> {
    check_recovery(source_parent)?;
    check_recovery(destination_parent)?;
    let source = observe(source_parent, source_name)?;
    let before = *progress;
    progress.discovered_files += 1;
    advance_live(live, &before, progress);
    match source.identity.kind {
        FileKind::Directory => run_directory_tree(
            source_parent,
            source_name,
            destination_parent,
            destination_name,
            source,
            engine,
            cancellation,
            progress,
            live,
            deferred_flags,
        ),
        FileKind::File | FileKind::Symlink => run_leaf_tree(
            source_parent,
            source_name,
            destination_parent,
            destination_name,
            source,
            engine,
            cancellation,
            progress,
            live,
            deferred_flags,
        ),
        _ => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "special objects cannot be transferred",
        )),
    }
}

fn run_leaf_tree(
    source_parent: BorrowedFd<'_>,
    source_name: &OsStr,
    destination_parent: BorrowedFd<'_>,
    destination_name: &OsStr,
    source: Observation,
    engine: EngineOptions,
    cancellation: &Cancellation,
    progress: &mut Progress,
    live: Option<&LiveProgress>,
    deferred_flags: &mut Vec<DeferredFlags>,
) -> io::Result<()> {
    let context = TransferContext {
        cancellation,
        live,
        engine,
        stop: AtomicBool::new(false),
        groups: GroupRegistry::default(),
        source_root: fs::openat(source_parent, c".", DIRECTORY_FLAGS, Mode::empty())?,
        destination_root: fs::openat(destination_parent, c".", DIRECTORY_FLAGS, Mode::empty())?,
        directories: Mutex::new(Vec::new()),
        source_mount: mount_identity(source_parent)?,
        deferred: Mutex::new(Vec::new()),
        failure: Mutex::new(None),
    };
    let source_root_identity = observe_fd(context.source_root.as_fd())?.identity;
    context
        .directories
        .lock()
        .unwrap()
        .push((PathBuf::new(), source_root_identity));
    let work = LeafWork {
        parent: PathBuf::new(),
        source_name: source_name.to_os_string(),
        destination_name: destination_name.to_os_string(),
        observed: source,
        copy: true,
    };
    let mut buffer = vec![0; TRANSFER_BUFFER_BYTES];
    let before = *progress;
    let transferred = context.transfer_leaf(&work, &mut buffer, progress);
    advance_live(live, &before, progress);
    transferred?;
    deferred_flags.append(&mut context.deferred.lock().unwrap().clone());
    if engine.durable {
        sync_fd(destination_parent)?;
        progress.synced_directories += 1;
    }
    Ok(())
}

fn run_directory_tree(
    source_parent: BorrowedFd<'_>,
    source_name: &OsStr,
    destination_parent: BorrowedFd<'_>,
    destination_name: &OsStr,
    source: Observation,
    engine: EngineOptions,
    cancellation: &Cancellation,
    progress: &mut Progress,
    live: Option<&LiveProgress>,
    deferred_flags: &mut Vec<DeferredFlags>,
) -> io::Result<()> {
    if directory_chain_contains(destination_parent, source)? {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "destination is inside the source directory",
        ));
    }
    let jobs = worker_count(&engine)?;
    let source_directory = open_directory(source_parent, source_name, source)?;
    fs::mkdirat(
        destination_parent,
        destination_name,
        Mode::from_raw_mode(0o700),
    )?;
    let destination_directory = fs::openat(
        destination_parent,
        destination_name,
        DIRECTORY_FLAGS,
        Mode::empty(),
    )?;
    let destination_identity = observe_fd(destination_directory.as_fd())?.identity;
    let root_metadata = capture(MetadataTarget::Fd(source_directory.as_fd()))?;
    require_unchanged_fd(source, source_directory.as_fd())?;
    let context = TransferContext {
        cancellation,
        live,
        engine,
        stop: AtomicBool::new(false),
        groups: GroupRegistry::default(),
        source_mount: mount_identity(source_directory.as_fd())?,
        source_root: source_directory,
        destination_root: destination_directory,
        directories: Mutex::new(Vec::new()),
        deferred: Mutex::new(Vec::new()),
        failure: Mutex::new(None),
    };
    context
        .directories
        .lock()
        .unwrap()
        .push((PathBuf::new(), source.identity));
    let result = transfer_directory_tree(&context, source, jobs, root_metadata, progress);
    match &result {
        Ok(()) => {
            deferred_flags.append(&mut context.deferred.lock().unwrap().clone());
            if engine.durable {
                sync_fd(destination_parent)?;
                progress.synced_directories += 1;
            }
        }
        Err(_) => {
            let _ = remove_owned_observed(
                destination_parent,
                destination_name,
                destination_identity,
            );
        }
    }
    result
}

fn transfer_directory_tree(
    context: &TransferContext<'_>,
    source: Observation,
    jobs: usize,
    root_metadata: Metadata,
    progress: &mut Progress,
) -> io::Result<()> {
    let (sender, receiver) = mpsc::sync_channel::<LeafWork>(256);
    let receiver = Mutex::new(receiver);
    let records = Mutex::new(Vec::<DirectoryRecord>::new());
    let worker_progress = Mutex::new(Progress::default());
    let produced = std::thread::scope(|scope| {
        let mut workers = Vec::with_capacity(jobs);
        for _ in 0..jobs {
            let receiver = &receiver;
            let worker_progress = &worker_progress;
            let worker = std::thread::Builder::new()
                .name("xsh-transfer".into())
                .stack_size(512 * 1024)
                .spawn_scoped(scope, move || {
                    let mut buffer = vec![0; TRANSFER_BUFFER_BYTES];
                    let mut local = Progress::default();
                    loop {
                        let work = {
                            let receiver = receiver.lock().unwrap();
                            receiver.recv()
                        };
                        let Ok(work) = work else { break };
                        if context.checkpoint().is_err() {
                            continue;
                        }
                        let before = local;
                        let transferred = context.transfer_leaf(&work, &mut buffer, &mut local);
                        advance_live(context.live, &before, &local);
                        if let Err(error) = transferred {
                            context.record_failure(error);
                        }
                    }
                    let mut aggregate = worker_progress.lock().unwrap();
                    aggregate.completed_files += local.completed_files;
                    aggregate.logical_bytes += local.logical_bytes;
                    aggregate.streamed_bytes += local.streamed_bytes;
                    aggregate.hole_bytes += local.hole_bytes;
                    aggregate.cloned_files += local.cloned_files;
                    aggregate.synced_files += local.synced_files;
                })
                .expect("spawn transfer worker");
            workers.push(worker);
        }
        let produced = produce_directory_work(
            context,
            source,
            &sender,
            &records,
            progress,
        );
        drop(sender);
        for worker in workers {
            if worker.join().is_err() {
                context.record_failure(io::Error::other("transfer worker panicked"));
            }
        }
        produced
    });
    let worker = worker_progress.into_inner().unwrap();
    progress.completed_files += worker.completed_files;
    progress.logical_bytes += worker.logical_bytes;
    progress.streamed_bytes += worker.streamed_bytes;
    progress.hole_bytes += worker.hole_bytes;
    progress.cloned_files += worker.cloned_files;
    progress.synced_files += worker.synced_files;
    if let Some(error) = context.failure.lock().unwrap().take() {
        return Err(error);
    }
    produced?;
    finalize_directories(
        context,
        &records.into_inner().unwrap(),
        root_metadata,
        progress,
    )
}

fn produce_directory_work(
    context: &TransferContext<'_>,
    root: Observation,
    sender: &mpsc::SyncSender<LeafWork>,
    records: &Mutex<Vec<DirectoryRecord>>,
    progress: &mut Progress,
) -> io::Result<()> {
    let mut queue = VecDeque::from([PathBuf::new()]);
    let mut representatives: Vec<((u64, u64), PathBuf)> = Vec::new();
    while let Some(relative) = queue.pop_front() {
        context.checkpoint()?;
        let source_directory = open_verified_directory(
            context.source_root.as_fd(),
            &relative,
            &context.directories,
        )?;
        let destination_directory =
            open_relative_directory(context.destination_root.as_fd(), &relative)?;
        let mut names = Vec::new();
        for entry in fs::Dir::read_from(source_directory.as_fd())? {
            context.checkpoint()?;
            let entry = entry?;
            let bytes = entry.file_name().to_bytes();
            if bytes == b"." || bytes == b".." {
                continue;
            }
            names.push(OsString::from_vec(bytes.to_vec()));
        }
        names.sort();
        for name in names {
            context.checkpoint()?;
            let name = OsStr::from_bytes(name.as_bytes());
            let path = relative.join(name);
            let observed = observe(source_directory.as_fd(), name)?;
            let before = *progress;
            progress.discovered_files += 1;
            advance_live(context.live, &before, progress);
            match observed.identity.kind {
                FileKind::Directory => {
                    let child = open_directory(source_directory.as_fd(), name, observed)?;
                    check_recovery(child.as_fd())?;
                    if !context.engine.cross_file_systems
                        && mount_identity(child.as_fd())? != context.source_mount
                    {
                        return Err(io::Error::other(
                            "nested source mount requires --cross-file-systems",
                        ));
                    }
                    let metadata = capture(MetadataTarget::Fd(child.as_fd()))?;
                    require_unchanged_fd(observed, child.as_fd())?;
                    fs::mkdirat(
                        destination_directory.as_fd(),
                        name,
                        Mode::from_raw_mode(0o700),
                    )?;
                    let published = observe(destination_directory.as_fd(), name)?;
                    context
                        .directories
                        .lock()
                        .unwrap()
                        .push((path.clone(), observed.identity));
                    records.lock().unwrap().push(DirectoryRecord {
                        relative: path.clone(),
                        metadata,
                        published: published.identity,
                    });
                    queue.push_back(path);
                }
                FileKind::File | FileKind::Symlink => {
                    if observed.identity.kind == FileKind::File
                        && !context.engine.cross_file_systems
                    {
                        let file = fs::openat(
                            source_directory.as_fd(),
                            name,
                            READ_FILE_FLAGS,
                            Mode::empty(),
                        )?;
                        if mount_identity(file.as_fd())? != context.source_mount {
                            return Err(io::Error::other(
                                "nested source file mount requires --cross-file-systems",
                            ));
                        }
                    }
                    let copy = observed.links < 2 || {
                        let key = (observed.identity.device, observed.identity.inode);
                        match representatives.iter().find(|(existing, _)| *existing == key) {
                            Some((_, first)) => first == &path,
                            None => {
                                representatives.push((key, path.clone()));
                                true
                            }
                        }
                    };
                    sender
                        .send(LeafWork {
                            parent: relative.clone(),
                            source_name: name.to_os_string(),
                            destination_name: name.to_os_string(),
                            observed,
                            copy,
                        })
                        .map_err(|_| io::Error::other("transfer workers ended early"))?;
                }
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "special objects cannot be transferred",
                    ));
                }
            }
        }
        require_unchanged_fd(root, context.source_root.as_fd())?;
    }
    Ok(())
}

fn finalize_directories(
    context: &TransferContext<'_>,
    records: &[DirectoryRecord],
    root_metadata: Metadata,
    progress: &mut Progress,
) -> io::Result<()> {
    for record in records.iter().rev() {
        context.checkpoint()?;
        let directory =
            open_relative_directory(context.destination_root.as_fd(), &record.relative)?;
        if observe_fd(directory.as_fd())?.identity != record.published {
            return Err(io::Error::other("destination directory identity changed"));
        }
        apply(
            MetadataTarget::Fd(directory.as_fd()),
            &record.metadata,
            Preservation::Copy,
        )?;
        if context.engine.durable {
            sync_fd(directory.as_fd())?;
            progress.synced_directories += 1;
        }
        defer_flags(context, &record.relative, &record.metadata);
        let before = *progress;
        progress.completed_files += 1;
        advance_live(context.live, &before, progress);
    }
    apply(
        MetadataTarget::Fd(context.destination_root.as_fd()),
        &root_metadata,
        Preservation::Copy,
    )?;
    if context.engine.durable {
        sync_fd(context.destination_root.as_fd())?;
        progress.synced_directories += 1;
    }
    defer_flags(context, Path::new(""), &root_metadata);
    let before = *progress;
    progress.completed_files += 1;
    advance_live(context.live, &before, progress);
    Ok(())
}

fn remove_owned_observed(
    parent: BorrowedFd<'_>,
    name: &OsStr,
    identity: EntryIdentity,
) -> io::Result<()> {
    let observed = match observe(parent, name) {
        Ok(observed) => observed,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if observed.identity != identity {
        return Err(io::Error::other("transferred entry identity changed"));
    }
    if identity.kind != FileKind::Directory {
        return fs::unlinkat(parent, name, AtFlags::empty()).map_err(Into::into);
    }
    let directory = fs::openat(parent, name, DIRECTORY_FLAGS, Mode::empty())?;
    if observe_fd(directory.as_fd())?.identity != identity {
        return Err(io::Error::other("transferred directory identity changed"));
    }
    let _ = fs::fchmod(directory.as_fd(), Mode::RWXU);
    for child in fs::Dir::read_from(directory.as_fd())? {
        let child = child?;
        let bytes = child.file_name().to_bytes();
        if bytes == b"." || bytes == b".." {
            continue;
        }
        let child_name = OsStr::from_bytes(bytes);
        let child_observed = observe(directory.as_fd(), child_name)?;
        remove_owned_observed(directory.as_fd(), child_name, child_observed.identity)?;
    }
    fs::unlinkat(parent, name, AtFlags::REMOVEDIR).map_err(Into::into)
}

pub(super) fn dry_run(
    source_parent: BorrowedFd<'_>,
    source_name: &OsStr,
    destination_parent: BorrowedFd<'_>,
    destination_name: &OsStr,
    options: &PolicyOptions,
    engine: &EngineOptions,
    cancellation: &Cancellation,
    plan: &mut Vec<DryRunEntry>,
) -> io::Result<()> {
    validate_entry_name(source_name, "source")?;
    validate_entry_name(destination_name, "destination")?;
    cancellation.check()?;
    let source = observe(source_parent, source_name)?;
    let target = match observe(destination_parent, destination_name) {
        Ok(observed) => Target::Existing(observed),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Target::Absent,
        Err(error) => return Err(error),
    };
    let action = decide(source, target, options)?;
    plan.push(DryRunEntry {
        source: PathBuf::from(source_name),
        destination: PathBuf::from(destination_name),
        action,
    });
    if source.identity.kind != FileKind::Directory || matches!(action, Action::Skip | Action::SameEntry)
    {
        return Ok(());
    }
    let source_directory = open_directory(source_parent, source_name, source)?;
    let source_mount = mount_identity(source_directory.as_fd())?;
    let destination_directory = match target {
        Target::Existing(observed) if options.merge => {
            let directory = open_directory(destination_parent, destination_name, observed)?;
            if !engine.cross_file_systems
                && mount_identity(directory.as_fd())? != mount_identity(destination_parent)?
            {
                return Err(io::Error::other(
                    "nested destination mount requires --cross-file-systems",
                ));
            }
            Some(directory)
        }
        _ => None,
    };
    let mut queue = VecDeque::from([(PathBuf::new(), destination_directory.is_some())]);
    while let Some((relative, merged)) = queue.pop_front() {
        cancellation.check()?;
        let directory = open_relative_directory(source_directory.as_fd(), &relative)?;
        let target_directory = match (&destination_directory, merged) {
            (Some(root), true) => Some(open_relative_directory(root.as_fd(), &relative)?),
            _ => None,
        };
        let mut names = Vec::new();
        for entry in fs::Dir::read_from(directory.as_fd())? {
            let entry = entry?;
            let bytes = entry.file_name().to_bytes();
            if bytes == b"." || bytes == b".." {
                continue;
            }
            names.push(OsString::from_vec(bytes.to_vec()));
        }
        names.sort();
        for name in names {
            cancellation.check()?;
            let name = OsStr::from_bytes(name.as_bytes());
            let child_source = relative.join(name);
            let child_destination = relative.join(name);
            let observed = observe(directory.as_fd(), name)?;
            let target = match &target_directory {
                Some(target_directory) => match observe(target_directory.as_fd(), name) {
                    Ok(observed) => Target::Existing(observed),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => Target::Absent,
                    Err(error) => return Err(error),
                },
                None => Target::Absent,
            };
            let action = decide(observed, target, options)?;
            plan.push(DryRunEntry {
                source: Path::new(source_name).join(&child_source),
                destination: Path::new(destination_name).join(&child_destination),
                action,
            });
            if observed.identity.kind == FileKind::Directory
                && !matches!(action, Action::Skip | Action::SameEntry)
            {
                let child = open_directory(directory.as_fd(), name, observed)?;
                if !engine.cross_file_systems
                    && mount_identity(child.as_fd())? != source_mount
                {
                    return Err(io::Error::other(
                        "nested source mount requires --cross-file-systems",
                    ));
                }
                let child_merged = target_directory.is_some() && action == Action::Merge;
                queue.push_back((relative.join(name), child_merged));
            }
        }
    }
    Ok(())
}


fn validate_entry_name(name: &OsStr, role: &'static str) -> io::Result<()> {
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes.contains(&0) || bytes.contains(&b'/') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{role} name is not a single path component"),
        ));
    }
    Ok(())
}

fn open_directory(
    parent: BorrowedFd<'_>,
    name: &OsStr,
    expected: Observation,
) -> io::Result<OwnedFd> {
    let directory = fs::openat(parent, name, DIRECTORY_FLAGS, Mode::empty())?;
    if !expected.same_identity(observe_fd(directory.as_fd())?) {
        return Err(io::Error::other("source directory identity changed"));
    }
    Ok(directory)
}

fn open_relative_directory(root: BorrowedFd<'_>, relative: &Path) -> io::Result<OwnedFd> {
    let mut current = fs::openat(root, ".", DIRECTORY_FLAGS, Mode::empty())?;
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "hardlink group path is not relative",
            ));
        };
        current = fs::openat(current.as_fd(), name, DIRECTORY_FLAGS, Mode::empty())?;
    }
    Ok(current)
}

fn directory_chain_contains(
    start: BorrowedFd<'_>,
    expected: Observation,
) -> io::Result<bool> {
    let mut current = fs::openat(start, ".", DIRECTORY_FLAGS, Mode::empty())?;
    loop {
        let observed = observe_fd(current.as_fd())?;
        if observed.identity.device == expected.identity.device
            && observed.identity.inode == expected.identity.inode
        {
            return Ok(true);
        }
        let parent = fs::openat(current.as_fd(), "..", DIRECTORY_FLAGS, Mode::empty())?;
        let parent_observed = observe_fd(parent.as_fd())?;
        if parent_observed.identity.device == observed.identity.device
            && parent_observed.identity.inode == observed.identity.inode
        {
            return Ok(false);
        }
        current = parent;
    }
}
