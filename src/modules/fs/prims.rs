//! Typed filesystem primitives for file utilities: complete metadata with
//! no-follow control, ownership, mode and nanosecond timestamps, device nodes,
//! raw statvfs, sparse and efficient copy, and atomic no-clobber rename.
//! Every failure is a host error that carries its errno.

use super::{gid_value_opt, name_error_path, uid_value_opt};
use crate::runtime::value::{RecordMap, RuntimeError, Value};
use crate::source::Span;
use rustix::fs::{self as rfs, AtFlags, CWD, Mode, StatVfsMountFlags, Timespec, Timestamps};
use rustix::fs::{UTIME_NOW, UTIME_OMIT};
use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::fs::{FileExt, FileTypeExt, MetadataExt, OpenOptionsExt};
use std::path::PathBuf;
use std::sync::Arc;

const NANOS: i64 = 1_000_000_000;

fn key(name: &str) -> Arc<str> {
    Arc::from(name)
}

fn time_ns(seconds: i64, nanos: i64) -> Value {
    Value::Int(seconds.saturating_mul(NANOS).saturating_add(nanos))
}

fn kind_name(file_type: std::fs::FileType) -> &'static str {
    if file_type.is_dir() {
        "dir"
    } else if file_type.is_file() {
        "file"
    } else if file_type.is_symlink() {
        "symlink"
    } else if file_type.is_fifo() {
        "fifo"
    } else if file_type.is_socket() {
        "socket"
    } else if file_type.is_block_device() {
        "block"
    } else if file_type.is_char_device() {
        "char"
    } else {
        "other"
    }
}

pub(crate) fn stat(path: PathBuf, follow: bool, span: Span) -> Result<Value, RuntimeError> {
    let shown = path.display().to_string();
    name_error_path(&shown, stat_unnamed(path, follow, span))
}

fn stat_unnamed(path: PathBuf, follow: bool, span: Span) -> Result<Value, RuntimeError> {
    let metadata = if follow {
        std::fs::metadata(&path)
    } else {
        std::fs::symlink_metadata(&path)
    }
    .map_err(|error| RuntimeError::host("fs-stat", &error).with_span(span))?;
    let birth = metadata
        .created()
        .ok()
        .and_then(|created| created.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(Value::Null, |since| {
            time_ns(since.as_secs() as i64, i64::from(since.subsec_nanos()))
        });
    Ok(Value::Record(RecordMap::from([
        (
            key("kind"),
            Value::Str(kind_name(metadata.file_type()).into()),
        ),
        (key("mode"), Value::Int(i64::from(metadata.mode()))),
        (key("size"), Value::Int(metadata.len() as i64)),
        (key("blocks_512"), Value::Int(metadata.blocks() as i64)),
        (key("blksize"), Value::Int(metadata.blksize() as i64)),
        (key("uid"), Value::Int(i64::from(metadata.uid()))),
        (key("gid"), Value::Int(i64::from(metadata.gid()))),
        (key("nlink"), Value::Int(metadata.nlink() as i64)),
        (key("dev"), Value::Int(metadata.dev() as i64)),
        (key("ino"), Value::Int(metadata.ino() as i64)),
        (key("rdev"), Value::Int(metadata.rdev() as i64)),
        (
            key("atime_ns"),
            time_ns(metadata.atime(), metadata.atime_nsec()),
        ),
        (
            key("mtime_ns"),
            time_ns(metadata.mtime(), metadata.mtime_nsec()),
        ),
        (
            key("ctime_ns"),
            time_ns(metadata.ctime(), metadata.ctime_nsec()),
        ),
        (key("birth_ns"), birth),
    ])))
}

pub(crate) fn set_owner(
    path: PathBuf,
    uid: Option<i64>,
    gid: Option<i64>,
    follow_symlinks: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    let uid = uid_value_opt(uid, "fs-set-owner", span)?;
    let gid = gid_value_opt(gid, "fs-set-owner", span)?;
    let flags = if follow_symlinks {
        AtFlags::empty()
    } else {
        AtFlags::SYMLINK_NOFOLLOW
    };
    let shown = path.display().to_string();
    name_error_path(
        &shown,
        rfs::chownat(CWD, &path, uid, gid, flags)
            .map_err(|error| RuntimeError::host("fs-set-owner", &error).with_span(span)),
    )
}

/// `chmod` that can refuse to follow a final symlink. Linux cannot change a
/// symlink's mode, so the nofollow form fails on one with `EOPNOTSUPP`.
pub(crate) fn chmod(
    path: PathBuf,
    mode: i64,
    follow_symlinks: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    if !(0..=0o7777).contains(&mode) {
        return Err(RuntimeError::new("fs-chmod", "mode is out of range").with_span(span));
    }
    let shown = path.display().to_string();
    let result = if follow_symlinks {
        std::fs::set_permissions(
            &path,
            std::os::unix::fs::PermissionsExt::from_mode(mode as u32),
        )
        .map_err(|error| RuntimeError::host("fs-chmod", &error).with_span(span))
    } else {
        chmod_nofollow(&path, Mode::from_raw_mode(mode as _))
            .map_err(|error| RuntimeError::host("fs-chmod", &error).with_span(span))
    };
    name_error_path(&shown, result)
}

/// The kernel has no no-follow `chmod`: pin the final component with an
/// `O_PATH` descriptor, refuse a symlink as glibc does, and chmod through
/// the descriptor so the check and the change see the same file.
#[cfg(target_os = "linux")]
fn chmod_nofollow(path: &std::path::Path, mode: Mode) -> rustix::io::Result<()> {
    use rustix::fs::OFlags;
    use std::os::fd::AsRawFd;
    let pinned = rfs::openat(
        CWD,
        path,
        OFlags::PATH | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    if rfs::FileType::from_raw_mode(rfs::fstat(&pinned)?.st_mode as _) == rfs::FileType::Symlink {
        return Err(rustix::io::Errno::OPNOTSUPP);
    }
    rfs::chmod(format!("/proc/self/fd/{}", pinned.as_raw_fd()), mode)
}

#[cfg(not(target_os = "linux"))]
fn chmod_nofollow(path: &std::path::Path, mode: Mode) -> rustix::io::Result<()> {
    rfs::chmodat(CWD, path, mode, AtFlags::SYMLINK_NOFOLLOW)
}

fn timespec(ns: Option<i64>, now: bool, field: &str, span: Span) -> Result<Timespec, RuntimeError> {
    match (ns, now) {
        (Some(_), true) => Err(RuntimeError::new(
            "fs-set-times",
            format!("{field}_ns and {field}_now are mutually exclusive"),
        )
        .with_span(span)),
        (None, true) => Ok(Timespec {
            tv_sec: 0,
            tv_nsec: UTIME_NOW as _,
        }),
        (None, false) => Ok(Timespec {
            tv_sec: 0,
            tv_nsec: UTIME_OMIT as _,
        }),
        (Some(ns), false) => Ok(Timespec {
            tv_sec: ns.div_euclid(NANOS) as _,
            tv_nsec: ns.rem_euclid(NANOS) as _,
        }),
    }
}

pub(crate) struct SetTimes {
    pub(crate) atime_ns: Option<i64>,
    pub(crate) mtime_ns: Option<i64>,
    pub(crate) atime_now: bool,
    pub(crate) mtime_now: bool,
    pub(crate) follow_symlinks: bool,
}

pub(crate) fn set_times(path: PathBuf, times: SetTimes, span: Span) -> Result<(), RuntimeError> {
    let timestamps = Timestamps {
        last_access: timespec(times.atime_ns, times.atime_now, "atime", span)?,
        last_modification: timespec(times.mtime_ns, times.mtime_now, "mtime", span)?,
    };
    let flags = if times.follow_symlinks {
        AtFlags::empty()
    } else {
        AtFlags::SYMLINK_NOFOLLOW
    };
    let shown = path.display().to_string();
    name_error_path(
        &shown,
        rfs::utimensat(CWD, &path, &timestamps, flags)
            .map_err(|error| RuntimeError::host("fs-set-times", &error).with_span(span)),
    )
}

pub(crate) fn mknod(
    path: PathBuf,
    kind: &str,
    mode: i64,
    major: i64,
    minor: i64,
    span: Span,
) -> Result<(), RuntimeError> {
    let fail = |message: &str| RuntimeError::new("fs-mknod", message).with_span(span);
    if !(0..=0o7777).contains(&mode) {
        return Err(fail("mode is out of range"));
    }
    let (file_type, device) = match kind {
        "file" => (rfs::FileType::RegularFile, false),
        "fifo" => (rfs::FileType::Fifo, false),
        "socket" => (rfs::FileType::Socket, false),
        "char" => (rfs::FileType::CharacterDevice, true),
        "block" => (rfs::FileType::BlockDevice, true),
        _ => {
            return Err(fail(
                "kind must be `file`, `fifo`, `socket`, `char`, or `block`",
            ));
        }
    };
    if !(0..=i64::from(u32::MAX)).contains(&major) || !(0..=i64::from(u32::MAX)).contains(&minor) {
        return Err(fail("major and minor must be between 0 and 4294967295"));
    }
    if !device && (major != 0 || minor != 0) {
        return Err(fail("major and minor apply only to `char` and `block`"));
    }
    let dev = rfs::makedev(major as u32, minor as u32);
    let shown = path.display().to_string();
    // Only Linux exposes `mknodat` here. Elsewhere the host lacks the
    // facility and the call fails with that errno, like any other missing one.
    #[cfg(target_os = "linux")]
    let made = rfs::mknodat(CWD, &path, file_type, Mode::from_raw_mode(mode as _), dev);
    #[cfg(not(target_os = "linux"))]
    let made: rustix::io::Result<()> = {
        let _ = (file_type, dev, mode);
        Err(rustix::io::Errno::NOTSUP)
    };
    name_error_path(
        &shown,
        made.map_err(|error| RuntimeError::host("fs-mknod", &error).with_span(span)),
    )
}

pub(crate) fn makedev(major: i64, minor: i64) -> i64 {
    rfs::makedev(major as u32, minor as u32) as i64
}

pub(crate) fn dev_major(dev: i64) -> i64 {
    i64::from(rfs::major(dev as rfs::Dev))
}

pub(crate) fn dev_minor(dev: i64) -> i64 {
    i64::from(rfs::minor(dev as rfs::Dev))
}

pub(crate) fn link(
    source: PathBuf,
    dest: PathBuf,
    follow_symlinks: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    let flags = if follow_symlinks {
        AtFlags::SYMLINK_FOLLOW
    } else {
        AtFlags::empty()
    };
    rfs::linkat(CWD, &source, CWD, &dest, flags)
        .map_err(|error| RuntimeError::host("fs-hardlink", &error).with_span(span))
}

/// The current process umask, read without changing it where the host exposes
/// it. The fallback briefly sets the mask to zero.
pub(crate) fn umask() -> i64 {
    #[cfg(target_os = "linux")]
    if let Ok(status) = std::fs::read_to_string("/proc/self/status")
        && let Some(mask) = status
            .lines()
            .find_map(|line| line.strip_prefix("Umask:"))
            .and_then(|mask| i64::from_str_radix(mask.trim(), 8).ok())
    {
        return mask;
    }
    let previous = rustix::process::umask(Mode::empty());
    rustix::process::umask(previous);
    i64::from(previous.bits())
}

pub(crate) fn statvfs_record(path: PathBuf, span: Span) -> Result<Value, RuntimeError> {
    let shown = path.display().to_string();
    let stats = name_error_path(
        &shown,
        rfs::statvfs(&path)
            .map_err(|error| RuntimeError::host("fs-statvfs", &error).with_span(span)),
    )?;
    #[cfg(target_os = "linux")]
    let type_magic = rfs::statfs(&path).map_or(Value::Null, |fs| Value::Int(fs.f_type as i64));
    #[cfg(not(target_os = "linux"))]
    let type_magic = Value::Null;
    let flag = |flag: StatVfsMountFlags| Value::Bool(stats.f_flag.contains(flag));
    Ok(Value::Record(RecordMap::from([
        (key("block_size"), Value::Int(stats.f_bsize as i64)),
        (key("fragment_size"), Value::Int(stats.f_frsize as i64)),
        (key("blocks"), Value::Int(stats.f_blocks as i64)),
        (key("blocks_free"), Value::Int(stats.f_bfree as i64)),
        (key("blocks_available"), Value::Int(stats.f_bavail as i64)),
        (key("files"), Value::Int(stats.f_files as i64)),
        (key("files_free"), Value::Int(stats.f_ffree as i64)),
        (key("files_available"), Value::Int(stats.f_favail as i64)),
        (key("fsid"), Value::Int(stats.f_fsid as i64)),
        (key("name_max"), Value::Int(stats.f_namemax as i64)),
        (key("type_magic"), type_magic),
        (key("flags"), Value::Int(stats.f_flag.bits() as i64)),
        (key("readonly"), flag(StatVfsMountFlags::RDONLY)),
        (key("nosuid"), flag(StatVfsMountFlags::NOSUID)),
        // `statvfs` reports these two only on Linux.
        #[cfg(target_os = "linux")]
        (key("nodev"), flag(StatVfsMountFlags::NODEV)),
        #[cfg(target_os = "linux")]
        (key("noexec"), flag(StatVfsMountFlags::NOEXEC)),
        #[cfg(not(target_os = "linux"))]
        (key("nodev"), Value::Bool(false)),
        #[cfg(not(target_os = "linux"))]
        (key("noexec"), Value::Bool(false)),
    ])))
}

/// Atomically renames without replacing an existing destination
/// (`RENAME_NOREPLACE`). A filesystem that cannot do this fails with its
/// errno instead of falling back to a racy check.
pub(crate) fn rename_noreplace(
    source: PathBuf,
    dest: PathBuf,
    span: Span,
) -> Result<(), RuntimeError> {
    let shown = format!("{} -> {}", source.display(), dest.display());
    #[cfg(target_os = "linux")]
    let result = rfs::renameat_with(CWD, &source, CWD, &dest, rfs::RenameFlags::NOREPLACE)
        .map_err(|error| RuntimeError::host("fs-rename", &error).with_span(span));
    #[cfg(not(target_os = "linux"))]
    let result = Err(RuntimeError::new(
        "fs-rename",
        "rename_noreplace is unsupported on this platform",
    )
    .with_span(span));
    name_error_path(&shown, result)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn data_extents(file: &File, len: u64) -> std::io::Result<Vec<(u64, u64)>> {
    use rustix::io::Errno;
    let mut extents = Vec::new();
    let mut position = 0;
    while position < len {
        let start = match rfs::seek(file, rfs::SeekFrom::Data(position)) {
            Ok(start) => start,
            Err(Errno::NXIO) => break,
            Err(Errno::INVAL | Errno::OPNOTSUPP | Errno::NOSYS) if extents.is_empty() => {
                return Ok(vec![(0, len)]);
            }
            Err(error) => return Err(error.into()),
        };
        let end = rfs::seek(file, rfs::SeekFrom::Hole(start))?.min(len);
        if start >= len {
            break;
        }
        extents.push((start, end - start));
        position = end;
    }
    Ok(extents)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn data_extents(_file: &File, len: u64) -> std::io::Result<Vec<(u64, u64)>> {
    Ok(if len == 0 { Vec::new() } else { vec![(0, len)] })
}

/// The data extents of a file as `{offset, length}` records; everything else
/// up to `size` is a hole. A filesystem without hole reporting is one extent.
pub(crate) fn data_ranges(path: PathBuf, span: Span) -> Result<Value, RuntimeError> {
    let shown = path.display().to_string();
    let result = (|| {
        let file = File::open(&path)?;
        let len = file.metadata()?.len();
        data_extents(&file, len)
    })();
    let extents = name_error_path(
        &shown,
        result.map_err(|error| RuntimeError::host("fs-data-ranges", &error).with_span(span)),
    )?;
    Ok(Value::List(
        extents
            .into_iter()
            .map(|(offset, length)| {
                Value::Record(RecordMap::from([
                    (key("offset"), Value::Int(offset as i64)),
                    (key("length"), Value::Int(length as i64)),
                ]))
            })
            .collect(),
    ))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Policy {
    Auto,
    Always,
    Never,
}

impl Policy {
    pub(crate) fn parse(text: &str, option: &str, span: Span) -> Result<Self, RuntimeError> {
        match text {
            "auto" => Ok(Self::Auto),
            "always" => Ok(Self::Always),
            "never" => Ok(Self::Never),
            _ => Err(RuntimeError::new(
                "fs-copy",
                format!("{option} must be `auto`, `always`, or `never`"),
            )
            .with_span(span)),
        }
    }
}

pub(crate) struct CopyFile {
    pub(crate) sparse: Policy,
    pub(crate) reflink: Policy,
    pub(crate) overwrite: bool,
    pub(crate) mode: Option<i64>,
    pub(crate) force: bool,
}

/// How bytes reached the destination, strongest first.
const METHOD_CLONE: &str = "clone";
const METHOD_KERNEL: &str = "copy_file_range";
const METHOD_USER: &str = "read_write";

pub(crate) fn copy_file(
    source: PathBuf,
    dest: PathBuf,
    options: CopyFile,
    span: Span,
) -> Result<Value, RuntimeError> {
    let shown = format!("{} -> {}", source.display(), dest.display());
    name_error_path(&shown, copy_file_unnamed(source, dest, options, span))
}

fn copy_file_unnamed(
    source: PathBuf,
    dest: PathBuf,
    options: CopyFile,
    span: Span,
) -> Result<Value, RuntimeError> {
    let host = |error: std::io::Error| RuntimeError::host("fs-copy", &error).with_span(span);
    let fail = |message: &str| RuntimeError::new("fs-copy", message).with_span(span);
    if let Some(mode) = options.mode
        && !(0..=0o7777).contains(&mode)
    {
        return Err(fail("mode is out of range"));
    }
    // Check identity before a blocking FIFO open, and again on opened files
    // before truncation so a destination replacement cannot clobber the source.
    let source_metadata = std::fs::metadata(&source).map_err(host)?;
    let kind = source_metadata.file_type();
    if !(kind.is_file() || kind.is_fifo() || kind.is_char_device() || kind.is_block_device()) {
        return Err(fail("source is not a regular file, FIFO, or device"));
    }
    let mut existed = match std::fs::metadata(&dest) {
        Ok(existing) => {
            if existing.dev() == source_metadata.dev() && existing.ino() == source_metadata.ino() {
                return Err(fail("source and destination are the same file"));
            }
            let kind = existing.file_type();
            if !(kind.is_file() || kind.is_fifo() || kind.is_char_device() || kind.is_block_device()) {
                return Err(fail("destination is not a regular file, FIFO, or device"));
            }
            if !kind.is_file() && options.reflink == Policy::Always {
                return Err(host(std::io::Error::from_raw_os_error(libc::ENOTSUP)));
            }
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => match std::fs::symlink_metadata(&dest) {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(host(error)),
        },
        Err(error) if error.raw_os_error() == Some(libc::ELOOP) && options.force && options.overwrite => {
            // lstat can prove the final link even when following it loops;
            // an ancestor loop also prevents lstat and remains an error.
            if !std::fs::symlink_metadata(&dest).map_err(host)?.file_type().is_symlink() {
                return Err(host(error));
            }
            true
        }
        Err(error) => return Err(host(error)),
    };
    if !kind.is_file() && options.reflink == Policy::Always {
        return Err(host(std::io::Error::from_raw_os_error(libc::ENOTSUP)));
    }
    let input = File::open(&source).map_err(host)?;
    let metadata = input.metadata().map_err(host)?;
    let opened_kind = metadata.file_type();
    if !(opened_kind.is_file() || opened_kind.is_fifo() || opened_kind.is_char_device() || opened_kind.is_block_device()) {
        return Err(fail("source is not a regular file, FIFO, or device"));
    }
    let len = metadata.len();
    let mode = options
        .mode
        .map_or(metadata.mode() & 0o777, |mode| mode as u32);
    let open_destination = || {
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .create_new(!options.overwrite)
            .truncate(false)
            .mode(mode)
            .open(&dest)
    };
    let output = match open_destination() {
        Ok(output) => output,
        Err(_) if options.force && options.overwrite && existed => {
            // The source is already open, so its read failure cannot delete a
            // destination. Recheck identity against that pinned source before
            // replacing a destination whose open failed; never retry transfer
            // failures or override exclusive creation.
            match std::fs::metadata(&dest) {
                Ok(existing) if existing.dev() == metadata.dev() && existing.ino() == metadata.ino() => {
                    return Err(fail("source and destination are the same file"));
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) if error.raw_os_error() == Some(libc::ELOOP) => {
                    if !std::fs::symlink_metadata(&dest).map_err(host)?.file_type().is_symlink() {
                        return Err(host(error));
                    }
                }
                Err(error) => return Err(host(error)),
            }
            std::fs::remove_file(&dest).map_err(host)?;
            existed = false;
            open_destination().map_err(host)?
        }
        Err(error) => return Err(host(error)),
    };
    let destination_metadata = output.metadata().map_err(host)?;
    if destination_metadata.dev() == metadata.dev() && destination_metadata.ino() == metadata.ino() {
        return Err(fail("source and destination are the same file"));
    }
    let destination_kind = destination_metadata.file_type();
    if !(destination_kind.is_file() || destination_kind.is_fifo() || destination_kind.is_char_device() || destination_kind.is_block_device()) {
        return Err(fail("destination is not a regular file, FIFO, or device"));
    }
    let outcome = if destination_kind.is_file() {
        output.set_len(0).map_err(host)?;
        transfer(&input, &output, &metadata, len, &options, span)
    } else if options.reflink == Policy::Always {
        Err(host(std::io::Error::from_raw_os_error(libc::ENOTSUP)))
    } else {
        copy_special_destination(&input, &output)
            .map(|bytes| (METHOD_USER, bytes, 0))
            .map_err(host)
    };
    if outcome.is_err() && !existed {
        let _ = std::fs::remove_file(&dest);
    }
    let (method, bytes, hole_bytes) = outcome?;
    Ok(Value::Record(RecordMap::from([
        (key("bytes"), Value::Int(bytes as i64)),
        (key("hole_bytes"), Value::Int(hole_bytes as i64)),
        (key("method"), Value::Str(method.into())),
    ])))
}

fn transfer(
    input: &File,
    output: &File,
    metadata: &std::fs::Metadata,
    len: u64,
    options: &CopyFile,
    span: Span,
) -> Result<(&'static str, u64, u64), RuntimeError> {
    let host = |error: std::io::Error| RuntimeError::host("fs-copy", &error).with_span(span);
    #[cfg(target_os = "linux")]
    if options.reflink != Policy::Never {
        match rfs::ioctl_ficlone(output, input) {
            Ok(()) => return Ok((METHOD_CLONE, len, 0)),
            Err(error) if options.reflink == Policy::Always => return Err(host(error.into())),
            Err(_) => {}
        }
    }
    #[cfg(not(target_os = "linux"))]
    if options.reflink == Policy::Always {
        return Err(host(std::io::Error::from_raw_os_error(libc::ENOTSUP)));
    }
    if stream_source(input, metadata).map_err(host)? || options.sparse == Policy::Always {
        let (bytes, written) = copy_stream(input, output, options.sparse == Policy::Always).map_err(host)?;
        return Ok((METHOD_USER, bytes, bytes - written));
    }
    let mut method = METHOD_USER;
    let mut copied = 0;
    let mut bytes = len;
    {
        // Allocated blocks below the length mean holes worth scanning for.
        let extents = if options.sparse == Policy::Auto && metadata.blocks() * 512 < len {
            data_extents(input, len).map_err(host)?
        } else if len == 0 {
            Vec::new()
        } else {
            vec![(0, len)]
        };
        for (offset, length) in extents {
            let (used, count) = copy_range(input, output, offset, length).map_err(host)?;
            if used == METHOD_KERNEL {
                method = METHOD_KERNEL;
            }
            copied += count;
            if count < length {
                bytes = offset + count;
                break;
            }
        }
    }
    // Trailing holes exist only once the length is set.
    output.set_len(bytes).map_err(host)?;
    Ok((method, bytes, bytes - copied))
}

/// Copies one extent, preferring the kernel and falling back to `pread`/`pwrite`
/// when it cannot (cross-device, unsupported filesystem, old kernel).
fn copy_range(
    input: &File,
    output: &File,
    offset: u64,
    length: u64,
) -> std::io::Result<(&'static str, u64)> {
    let mut done = 0;
    let mut method = METHOD_USER;
    #[cfg(target_os = "linux")]
    {
        let (mut from, mut to) = (offset, offset);
        while done < length {
            let chunk = (length - done).min(1 << 30) as usize;
            match rfs::copy_file_range(input, Some(&mut from), output, Some(&mut to), chunk) {
                Ok(0) => break,
                Ok(count) => {
                    done += count as u64;
                    method = METHOD_KERNEL;
                }
                Err(_) => break,
            }
        }
    }
    let mut buffer = vec![0; 1 << 16];
    while done < length {
        let want = ((length - done) as usize).min(buffer.len());
        let read = input.read_at(&mut buffer[..want], offset + done)?;
        if read == 0 {
            break;
        }
        output.write_all_at(&buffer[..read], offset + done)?;
        done += read as u64;
    }
    Ok((method, done))
}

/// Virtual filesystem lengths describe an interface, rather than the readable
/// byte count. Never use those lengths or hole maps to bound a transfer.
fn stream_source(input: &File, metadata: &std::fs::Metadata) -> std::io::Result<bool> {
    if !metadata.is_file() || metadata.len() == 0 {
        return Ok(true);
    }
    #[cfg(target_os = "linux")]
    {
        let kind = rfs::fstatfs(input)?.f_type;
        if kind == 0x9fa0 || kind == 0x62656572 {
            return Ok(true);
        }
    }
    Ok(false)
}

/// A fixed buffer bounds memory for devices and FIFOs. Zero blocks become
/// holes only when explicitly requested; the final length includes trailing
/// zeros even when no write materializes them.
fn copy_stream(mut input: &File, output: &File, sparse: bool) -> std::io::Result<(u64, u64)> {
    const BLOCK: usize = 4096;
    let mut buffer = vec![0; 1 << 16];
    let (mut offset, mut written) = (0u64, 0u64);
    loop {
        let read = match input.read(&mut buffer) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if read == 0 {
            break;
        }
        for (index, block) in buffer[..read].chunks(BLOCK).enumerate() {
            if !sparse || block.iter().any(|byte| *byte != 0) {
                output.write_all_at(block, offset + (index * BLOCK) as u64)?;
                written += block.len() as u64;
            }
        }
        offset = offset.checked_add(read as u64).ok_or_else(|| {
            std::io::Error::other("copied byte count exceeds the host file size limit")
        })?;
    }
    output.set_len(offset)?;
    Ok((offset, written))
}

/// Devices and FIFOs receive every byte, including source holes and zero
/// blocks: seeking would lose data or fail, and truncation could damage the
/// destination's identity. Sparse policies therefore never claim holes here.
fn copy_special_destination(mut input: &File, mut output: &File) -> std::io::Result<u64> {
    let mut buffer = vec![0; 1 << 16];
    let mut copied = 0u64;
    loop {
        let read = match input.read(&mut buffer) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if read == 0 {
            return Ok(copied);
        }
        output.write_all(&buffer[..read])?;
        copied = copied.checked_add(read as u64).ok_or_else(|| {
            std::io::Error::other("copied byte count exceeds the host file size limit")
        })?;
    }
}
