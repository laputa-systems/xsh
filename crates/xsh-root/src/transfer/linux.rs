use std::ffi::{OsStr, OsString};
use std::io;
use std::mem::size_of;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf};

use rustix::fs::{
    self, AtFlags, FileType, Gid, IFlags, Mode, OFlags, SeekFrom, StatxFlags, Timespec, Timestamps,
    Uid, XattrFlags,
};
use rustix::io::Errno;

pub(super) mod journal;
pub(super) mod policy;
pub(super) mod stage;

pub(super) use policy::preflight;
pub(super) use stage::{merge, publish};

use super::{
    Cancellation, CopyMethod, CopyStats, EntryIdentity, Metadata, MetadataTarget, Observation,
    Preservation, Progress, Timestamp, TreeTransfer, Xattr, TRANSFER_BUFFER_BYTES,
};
use crate::FileKind;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct MountIdentity {
    pub device: u64,
    pub mount_id: u64,
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

struct HardlinkGroup {
    source_device: u64,
    source_inode: u64,
    published_device: u64,
    published_inode: u64,
    relative: PathBuf,
    parent_device: u64,
    parent_inode: u64,
    name: OsString,
}

struct DirectoryRecord {
    relative: PathBuf,
    metadata: Metadata,
    device: u64,
    inode: u64,
}

struct TransferState<'a> {
    cancellation: &'a Cancellation,
    progress: &'a mut Progress,
    hardlinks: Vec<HardlinkGroup>,
    directories: Vec<DirectoryRecord>,
    buffer: Vec<u8>,
}

impl TransferState<'_> {
    fn copy_children(
        &mut self,
        source_directory: BorrowedFd<'_>,
        destination_directory: BorrowedFd<'_>,
        destination_root: BorrowedFd<'_>,
        relative: &Path,
    ) -> io::Result<()> {
        let before = observe_fd(source_directory)?;
        for entry in fs::Dir::read_from(source_directory)? {
            self.cancellation.check()?;
            let entry = entry?;
            let bytes = entry.file_name().to_bytes();
            if bytes == b"." || bytes == b".." {
                continue;
            }
            let name = OsStr::from_bytes(bytes);
            let path = relative.join(name);
            let observed = observe(source_directory, name)?;
            self.progress.discovered_files += 1;
            match observed.identity.kind {
                FileKind::Directory => {
                    let source_child = open_directory(source_directory, name, observed)?;
                    fs::mkdirat(destination_directory, name, Mode::from_raw_mode(0o700))?;
                    let destination_child = fs::openat(
                        destination_directory,
                        name,
                        DIRECTORY_FLAGS,
                        Mode::empty(),
                    )?;
                    self.copy_children(
                        source_child.as_fd(),
                        destination_child.as_fd(),
                        destination_root,
                        &path,
                    )?;
                    let metadata = capture(MetadataTarget::Fd(source_child.as_fd()))?;
                    require_unchanged_fd(observed, source_child.as_fd())?;
                    let published = observe_fd(destination_child.as_fd())?;
                    self.directories.push(DirectoryRecord {
                        relative: path,
                        metadata,
                        device: published.identity.device,
                        inode: published.identity.inode,
                    });
                    self.progress.completed_files += 1;
                }
                FileKind::File | FileKind::Symlink => {
                    self.copy_leaf(
                        source_directory,
                        name,
                        destination_directory,
                        name,
                        destination_root,
                        &path,
                        observed,
                        true,
                    )?;
                }
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "special objects cannot be transferred",
                    ));
                }
            }
        }
        require_unchanged_fd(before, source_directory)
    }

    fn finalize_directories(&mut self, destination_root: BorrowedFd<'_>) -> io::Result<()> {
        for record in std::mem::take(&mut self.directories) {
            self.cancellation.check()?;
            let directory = open_relative_directory(destination_root, &record.relative)?;
            let published = observe_fd(directory.as_fd())?;
            if published.identity.device != record.device
                || published.identity.inode != record.inode
            {
                return Err(io::Error::other(
                    "destination directory identity changed",
                ));
            }
            apply(
                MetadataTarget::Fd(directory.as_fd()),
                &record.metadata,
                Preservation::Copy,
            )?;
            apply_final(MetadataTarget::Fd(directory.as_fd()), &record.metadata)?;
        }
        Ok(())
    }

    fn copy_leaf(
        &mut self,
        source_directory: BorrowedFd<'_>,
        source_name: &OsStr,
        destination_directory: BorrowedFd<'_>,
        destination_name: &OsStr,
        destination_root: BorrowedFd<'_>,
        relative: &Path,
        observed: Observation,
        record_hardlink: bool,
    ) -> io::Result<()> {
        self.cancellation.check()?;
        if record_hardlink
            && observed.links > 1
            && matches!(observed.identity.kind, FileKind::File | FileKind::Symlink)
            && let Some(index) = self
                .hardlinks
                .iter()
                .position(|group| {
                    group.source_device == observed.identity.device
                        && group.source_inode == observed.identity.inode
                })
        {
            self.link_hardlink(index, destination_root, destination_directory, destination_name)?;
            self.progress.completed_files += 1;
            return Ok(());
        }

        let published = match observed.identity.kind {
            FileKind::File => {
                let source = fs::openat(
                    source_directory,
                    source_name,
                    READ_FILE_FLAGS,
                    Mode::empty(),
                )?;
                if !observed.same_version(observe_fd(source.as_fd())?) {
                    return Err(io::Error::other("source changed while opening"));
                }
                let metadata = capture(MetadataTarget::Fd(source.as_fd()))?;
                let destination = fs::openat(
                    destination_directory,
                    destination_name,
                    WRITE_NEW_FLAGS,
                    Mode::from_raw_mode(0o600),
                )?;
                let result = (|| {
                    let stats = copy_data(
                        source.as_fd(),
                        destination.as_fd(),
                        observed.size,
                        &mut self.buffer,
                        self.cancellation,
                    )?;
                    self.progress.record(stats);
                    require_unchanged(source_directory, source_name, observed)?;
                    apply(
                        MetadataTarget::Fd(destination.as_fd()),
                        &metadata,
                        Preservation::Copy,
                    )?;
                    apply_final(MetadataTarget::Fd(destination.as_fd()), &metadata)
                })();
                if let Err(error) = result {
                    let cleanup = fs::unlinkat(
                        destination_directory,
                        destination_name,
                        AtFlags::empty(),
                    );
                    return Err(with_cleanup(error, cleanup));
                }
                observe_fd(destination.as_fd())?
            }
            FileKind::Symlink => {
                let metadata = capture(MetadataTarget::Symlink {
                    parent: source_directory,
                    name: source_name,
                })?;
                let target = fs::readlinkat(source_directory, source_name, Vec::new())?;
                require_unchanged(source_directory, source_name, observed)?;
                fs::symlinkat(target.as_c_str(), destination_directory, destination_name)?;
                let result = apply(
                    MetadataTarget::Symlink {
                        parent: destination_directory,
                        name: destination_name,
                    },
                    &metadata,
                    Preservation::Copy,
                )
                .and_then(|()| {
                    apply_final(
                        MetadataTarget::Symlink {
                            parent: destination_directory,
                            name: destination_name,
                        },
                        &metadata,
                    )
                })
                .and_then(|()| require_unchanged(source_directory, source_name, observed));
                if let Err(error) = result {
                    let cleanup = fs::unlinkat(
                        destination_directory,
                        destination_name,
                        AtFlags::empty(),
                    );
                    return Err(with_cleanup(error, cleanup));
                }
                observe(destination_directory, destination_name)?
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "special objects cannot be transferred",
                ));
            }
        };

        if record_hardlink && observed.links > 1 {
            let parent = observe_fd(destination_directory)?;
            self.hardlinks.push(HardlinkGroup {
                source_device: observed.identity.device,
                source_inode: observed.identity.inode,
                published_device: published.identity.device,
                published_inode: published.identity.inode,
                relative: relative.to_path_buf(),
                parent_device: parent.identity.device,
                parent_inode: parent.identity.inode,
                name: destination_name.to_os_string(),
            });
        }
        self.progress.completed_files += 1;
        Ok(())
    }

    fn link_hardlink(
        &self,
        index: usize,
        destination_root: BorrowedFd<'_>,
        destination_directory: BorrowedFd<'_>,
        destination_name: &OsStr,
    ) -> io::Result<()> {
        let group = &self.hardlinks[index];
        let parent =
            open_relative_directory(destination_root, group.relative.parent().unwrap_or(Path::new("")))?;
        let parent_observation = observe_fd(parent.as_fd())?;
        if parent_observation.identity.device != group.parent_device
            || parent_observation.identity.inode != group.parent_inode
        {
            return Err(io::Error::other("hardlink group parent changed"));
        }
        let existing = observe(parent.as_fd(), &group.name)?;
        if existing.identity.device != group.published_device
            || existing.identity.inode != group.published_inode
        {
            return Err(io::Error::other("hardlink group member changed"));
        }
        self.cancellation.check()?;
        fs::linkat(
            parent.as_fd(),
            &group.name,
            destination_directory,
            destination_name,
            AtFlags::empty(),
        )?;
        let linked = observe(destination_directory, destination_name)?;
        if linked.identity.device != group.published_device
            || linked.identity.inode != group.published_inode
        {
            let cleanup = fs::unlinkat(destination_directory, destination_name, AtFlags::empty());
            return Err(with_cleanup(
                io::Error::other("hardlink identity changed while linking"),
                cleanup,
            ));
        }
        Ok(())
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
        cancellation,
        progress,
    } = request;
    let result = run_tree(
        source_parent,
        source_name,
        destination_parent,
        destination_name,
        cancellation,
        progress,
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
    cancellation: &Cancellation,
    progress: &mut Progress,
) -> io::Result<()> {
    validate_entry_name(source_name, "source")?;
    validate_entry_name(destination_name, "destination")?;
    cancellation.check()?;

    let source = observe(source_parent, source_name)?;
    progress.discovered_files += 1;
    let mut state = TransferState {
        cancellation,
        progress,
        hardlinks: Vec::new(),
        directories: Vec::new(),
        buffer: vec![0; TRANSFER_BUFFER_BYTES],
    };

    match source.identity.kind {
        FileKind::Directory => {
            if directory_chain_contains(destination_parent, source)? {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "destination is inside the source directory",
                ));
            }
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
            state.copy_children(
                source_directory.as_fd(),
                destination_directory.as_fd(),
                destination_directory.as_fd(),
                Path::new(""),
            )?;
            let metadata = capture(MetadataTarget::Fd(source_directory.as_fd()))?;
            require_unchanged_fd(source, source_directory.as_fd())?;
            let published = observe_fd(destination_directory.as_fd())?;
            state.directories.push(DirectoryRecord {
                relative: PathBuf::new(),
                metadata,
                device: published.identity.device,
                inode: published.identity.inode,
            });
            state.finalize_directories(destination_directory.as_fd())?;
            state.progress.completed_files += 1;
            Ok(())
        }
        FileKind::File | FileKind::Symlink => state.copy_leaf(
            source_parent,
            source_name,
            destination_parent,
            destination_name,
            destination_parent,
            Path::new(""),
            source,
            false,
        ),
        _ => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "special objects cannot be transferred",
        )),
    }
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
