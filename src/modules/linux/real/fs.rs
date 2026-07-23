use super::common::{error_value, io_error, ok_unit};
use super::mount::read_mounts;
use super::{FS_APPEND_FL, FS_COMPR_FL, FS_DIRSYNC_FL, FS_IMMUTABLE_FL, FS_INDEX_FL};
use super::{FS_JOURNAL_DATA_FL, FS_NOATIME_FL, FS_NODUMP_FL, FS_NOTAIL_FL};
use super::{FS_SECRM_FL, FS_SYNC_FL, FS_TOPDIR_FL, FS_UNRM_FL, MountEntry};
use crate::modules::linux::str_value;
use crate::runtime::value::{LiveStream, RuntimeError, StreamValue, Value};
use crate::source::Span;
use rustix::fs as rfs;
use std::fs;
use std::io;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(crate) fn is_mountpoint(path: &Path, span: Span) -> Result<Value, RuntimeError> {
    match is_mountpoint_impl(path) {
        Ok(value) => Ok(Value::ok(Value::Bool(value))),
        Err(error) => Ok(io_error("linux-mountpoint", error, span)),
    }
}

pub(crate) fn disk_usage(path: Option<&Path>, span: Span) -> Result<Value, RuntimeError> {
    let mounts = match read_mounts("/proc/mounts") {
        Ok(mounts) => mounts,
        Err(error) => return Ok(io_error("linux-disk-usage", error, span)),
    };
    let selected = match path {
        Some(path) => {
            let Some(mount) = mount_for_path(&mounts, path) else {
                return Ok(error_value("linux-disk-usage", "mount not found", span));
            };
            (vec![mount.clone()], Some(path.to_path_buf()))
        }
        None => (mounts, None),
    };
    Ok(Value::ok(Value::stream(StreamValue::from_live(
        "linux.disk_usage",
        DiskUsageStream {
            mounts: selected.0.into_iter(),
            stat_path: selected.1,
        },
    ))))
}

struct DiskUsageStream {
    mounts: std::vec::IntoIter<MountEntry>,
    stat_path: Option<PathBuf>,
}

impl LiveStream for DiskUsageStream {
    fn next(&mut self, span: Span) -> Result<Option<Value>, RuntimeError> {
        let Some(mount) = self.mounts.next() else {
            return Ok(None);
        };
        let path = self
            .stat_path
            .as_deref()
            .unwrap_or_else(|| Path::new(&mount.target));
        disk_usage_record(&mount, path, span).map(Some)
    }
}

pub(crate) fn sysctl_get(key: &str, span: Span) -> Result<Value, RuntimeError> {
    let path = match sysctl_proc_path(key, span) {
        Ok(path) => path,
        Err(error) => return Ok(Value::err(Value::Error(Box::new(error)))),
    };
    match fs::read_to_string(&path) {
        Ok(value) => Ok(Value::ok(str_value(value.trim_end().to_string()))),
        Err(error) => Ok(io_error("linux-sysctl", error, span)),
    }
}

pub(crate) fn sysctl_set(key: &str, value: &str, span: Span) -> Result<Value, RuntimeError> {
    let path = match sysctl_proc_path(key, span) {
        Ok(path) => path,
        Err(error) => return Ok(Value::err(Value::Error(Box::new(error)))),
    };
    match fs::write(&path, value) {
        Ok(()) => Ok(ok_unit()),
        Err(error) => Ok(io_error("linux-sysctl", error, span)),
    }
}

pub(crate) fn file_attrs(path: &Path, span: Span) -> Result<Value, RuntimeError> {
    match ioctl_get_u32(path, fs_ioc_getflags(), "linux-file-attrs", span) {
        Ok(flags) => Ok(Value::ok(file_attrs_record(flags))),
        Err(error) => Ok(Value::err(Value::Error(Box::new(error)))),
    }
}

pub(crate) fn set_file_attrs(path: &Path, flags: i64, span: Span) -> Result<Value, RuntimeError> {
    if !(0..=u32::MAX as i64).contains(&flags) {
        return Ok(error_value(
            "linux-file-attrs",
            "flags must be between 0 and 4294967295",
            span,
        ));
    }
    match ioctl_set_u32(
        path,
        fs_ioc_setflags(),
        flags as u32,
        "linux-file-attrs",
        span,
    ) {
        Ok(()) => Ok(ok_unit()),
        Err(error) => Ok(Value::err(Value::Error(Box::new(error)))),
    }
}

pub(crate) fn file_version(path: &Path, span: Span) -> Result<Value, RuntimeError> {
    match ioctl_get_u32(path, fs_ioc_getversion(), "linux-file-version", span) {
        Ok(version) => Ok(Value::ok(Value::Int(version as i64))),
        Err(error) => Ok(Value::err(Value::Error(Box::new(error)))),
    }
}

pub(crate) fn set_file_version(
    path: &Path,
    version: i64,
    span: Span,
) -> Result<Value, RuntimeError> {
    if !(0..=u32::MAX as i64).contains(&version) {
        return Ok(error_value(
            "linux-file-version",
            "version must be between 0 and 4294967295",
            span,
        ));
    }
    match ioctl_set_u32(
        path,
        fs_ioc_setversion(),
        version as u32,
        "linux-file-version",
        span,
    ) {
        Ok(()) => Ok(ok_unit()),
        Err(error) => Ok(Value::err(Value::Error(Box::new(error)))),
    }
}

pub(crate) fn sysctl_load_dirs(
    dirs: &[PathBuf],
    fallback: Option<&Path>,
    span: Span,
) -> Result<Value, RuntimeError> {
    for path in sysctl_config_paths(dirs, fallback) {
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => return Ok(io_error("linux-sysctl", error, span)),
        };
        for line in text.lines() {
            let Some((key, value)) = parse_sysctl_line(line) else {
                continue;
            };
            let proc_path = match sysctl_proc_path(&key, span) {
                Ok(path) => path,
                Err(error) => return Ok(Value::err(Value::Error(Box::new(error)))),
            };
            if let Err(error) = fs::write(&proc_path, value) {
                return Ok(io_error("linux-sysctl", error, span));
            }
        }
    }
    Ok(ok_unit())
}

fn is_mountpoint_impl(path: &Path) -> io::Result<bool> {
    let metadata = fs::metadata(path)?;
    let parent = match path.parent() {
        Some(parent) if parent.as_os_str().is_empty() => Path::new("."),
        Some(parent) => parent,
        None => {
            return Ok(true);
        }
    };
    let parent_metadata = fs::metadata(parent)?;
    Ok(metadata.dev() != parent_metadata.dev()
        || (metadata.dev() == parent_metadata.dev() && metadata.ino() == parent_metadata.ino()))
}

fn mount_for_path<'a>(mounts: &'a [MountEntry], path: &Path) -> Option<&'a MountEntry> {
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    mounts
        .iter()
        .filter(|mount| target.starts_with(Path::new(&mount.target)))
        .max_by_key(|mount| Path::new(&mount.target).components().count())
}

fn disk_usage_record(
    mount: &MountEntry,
    stat_path: &Path,
    span: Span,
) -> Result<Value, RuntimeError> {
    let stats = rfs::statvfs(stat_path).map_err(|error| {
        RuntimeError::new("linux-disk-usage", error.to_string()).with_span(span)
    })?;
    let block_size = stats.f_bsize as u128;
    let total = blocks_to_i64(stats.f_blocks as u128, block_size);
    let used_blocks = stats.f_blocks.saturating_sub(stats.f_bfree);
    let used = blocks_to_i64(used_blocks as u128, block_size);
    let available = blocks_to_i64(stats.f_bavail as u128, block_size);
    Ok(Value::Record(crate::runtime::value::RecordMap::from([
        (Arc::from("device"), str_value(mount.source.clone())),
        (Arc::from("mount"), str_value(mount.target.clone())),
        (Arc::from("fstype"), str_value(mount.fstype.clone())),
        (Arc::from("total"), Value::Int(total)),
        (Arc::from("used"), Value::Int(used)),
        (Arc::from("available"), Value::Int(available)),
    ])))
}

fn blocks_to_i64(blocks: u128, block_size: u128) -> i64 {
    blocks.saturating_mul(block_size).min(i64::MAX as u128) as i64
}

fn file_attrs_record(flags: u32) -> Value {
    Value::Record(crate::runtime::value::RecordMap::from([
        (Arc::from("flags"), Value::Int(flags as i64)),
        (
            Arc::from("indexed_directory"),
            Value::Bool(flags & FS_INDEX_FL != 0),
        ),
        (
            Arc::from("secure_deletion"),
            Value::Bool(flags & FS_SECRM_FL != 0),
        ),
        (Arc::from("undelete"), Value::Bool(flags & FS_UNRM_FL != 0)),
        (Arc::from("sync"), Value::Bool(flags & FS_SYNC_FL != 0)),
        (
            Arc::from("dirsync"),
            Value::Bool(flags & FS_DIRSYNC_FL != 0),
        ),
        (
            Arc::from("immutable"),
            Value::Bool(flags & FS_IMMUTABLE_FL != 0),
        ),
        (
            Arc::from("append_only"),
            Value::Bool(flags & FS_APPEND_FL != 0),
        ),
        (Arc::from("no_dump"), Value::Bool(flags & FS_NODUMP_FL != 0)),
        (
            Arc::from("no_atime"),
            Value::Bool(flags & FS_NOATIME_FL != 0),
        ),
        (
            Arc::from("compression_requested"),
            Value::Bool(flags & FS_COMPR_FL != 0),
        ),
        (
            Arc::from("journaled_data"),
            Value::Bool(flags & FS_JOURNAL_DATA_FL != 0),
        ),
        (
            Arc::from("no_tailmerging"),
            Value::Bool(flags & FS_NOTAIL_FL != 0),
        ),
        (
            Arc::from("top_of_directory_hierarchies"),
            Value::Bool(flags & FS_TOPDIR_FL != 0),
        ),
    ]))
}

fn ioctl_get_u32(
    path: &Path,
    request: libc::c_ulong,
    kind: &str,
    span: Span,
) -> Result<u32, RuntimeError> {
    let fd = open_path(path, kind, span)?;
    let mut value = 0u32;
    // SAFETY: `fd` is open and `value` points to writable storage.
    let rc = unsafe { libc::ioctl(fd.as_raw_fd(), request as _, &mut value) };
    let error = io::Error::last_os_error();
    if rc == 0 {
        Ok(value)
    } else {
        Err(RuntimeError::new(kind, error.to_string()).with_span(span))
    }
}

fn ioctl_set_u32(
    path: &Path,
    request: libc::c_ulong,
    value: u32,
    kind: &str,
    span: Span,
) -> Result<(), RuntimeError> {
    let fd = open_path(path, kind, span)?;
    let mut value = value;
    // SAFETY: `fd` is open and `value` points to readable storage.
    let rc = unsafe { libc::ioctl(fd.as_raw_fd(), request as _, &mut value) };
    let error = io::Error::last_os_error();
    if rc == 0 {
        Ok(())
    } else {
        Err(RuntimeError::new(kind, error.to_string()).with_span(span))
    }
}

fn open_path(path: &Path, kind: &str, span: Span) -> Result<OwnedFd, RuntimeError> {
    rfs::open(
        path,
        rfs::OFlags::RDONLY | rfs::OFlags::NONBLOCK | rfs::OFlags::CLOEXEC,
        rfs::Mode::empty(),
    )
    .map_err(|error| RuntimeError::new(kind, io::Error::from(error).to_string()).with_span(span))
}

#[cfg(any(target_arch = "powerpc", target_arch = "mips"))]
const fn fs_ioc_getflags() -> libc::c_ulong {
    0x4004_6601
}

#[cfg(any(target_arch = "powerpc", target_arch = "mips"))]
const fn fs_ioc_setflags() -> libc::c_ulong {
    0x8004_6602
}

#[cfg(any(target_arch = "powerpc", target_arch = "mips"))]
const fn fs_ioc_getversion() -> libc::c_ulong {
    0x4004_7601
}

#[cfg(any(target_arch = "powerpc", target_arch = "mips"))]
const fn fs_ioc_setversion() -> libc::c_ulong {
    0x8004_7602
}

#[cfg(any(target_arch = "powerpc64", target_arch = "mips64"))]
const fn fs_ioc_getflags() -> libc::c_ulong {
    0x4008_6601
}

#[cfg(any(target_arch = "powerpc64", target_arch = "mips64"))]
const fn fs_ioc_setflags() -> libc::c_ulong {
    0x8008_6602
}

#[cfg(any(target_arch = "powerpc64", target_arch = "mips64"))]
const fn fs_ioc_getversion() -> libc::c_ulong {
    0x4008_7601
}

#[cfg(any(target_arch = "powerpc64", target_arch = "mips64"))]
const fn fs_ioc_setversion() -> libc::c_ulong {
    0x8008_7602
}

#[cfg(all(
    target_pointer_width = "32",
    not(any(target_arch = "powerpc", target_arch = "mips"))
))]
const fn fs_ioc_getflags() -> libc::c_ulong {
    0x8004_6601
}

#[cfg(all(
    target_pointer_width = "32",
    not(any(target_arch = "powerpc", target_arch = "mips"))
))]
const fn fs_ioc_setflags() -> libc::c_ulong {
    0x4004_6602
}

#[cfg(all(
    target_pointer_width = "32",
    not(any(target_arch = "powerpc", target_arch = "mips"))
))]
const fn fs_ioc_getversion() -> libc::c_ulong {
    0x8004_7601
}

#[cfg(all(
    target_pointer_width = "32",
    not(any(target_arch = "powerpc", target_arch = "mips"))
))]
const fn fs_ioc_setversion() -> libc::c_ulong {
    0x4004_7602
}

#[cfg(all(
    target_pointer_width = "64",
    not(any(
        target_arch = "powerpc64",
        target_arch = "mips64",
        target_arch = "powerpc",
        target_arch = "mips"
    ))
))]
const fn fs_ioc_getflags() -> libc::c_ulong {
    0x8008_6601
}

#[cfg(all(
    target_pointer_width = "64",
    not(any(
        target_arch = "powerpc64",
        target_arch = "mips64",
        target_arch = "powerpc",
        target_arch = "mips"
    ))
))]
const fn fs_ioc_setflags() -> libc::c_ulong {
    0x4008_6602
}

#[cfg(all(
    target_pointer_width = "64",
    not(any(
        target_arch = "powerpc64",
        target_arch = "mips64",
        target_arch = "powerpc",
        target_arch = "mips"
    ))
))]
const fn fs_ioc_getversion() -> libc::c_ulong {
    0x8008_7601
}

#[cfg(all(
    target_pointer_width = "64",
    not(any(
        target_arch = "powerpc64",
        target_arch = "mips64",
        target_arch = "powerpc",
        target_arch = "mips"
    ))
))]
const fn fs_ioc_setversion() -> libc::c_ulong {
    0x4008_7602
}

fn sysctl_config_paths(dirs: &[PathBuf], fallback: Option<&Path>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        let mut dir_paths = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "conf"))
            .collect::<Vec<_>>();
        dir_paths.sort_unstable();
        paths.extend(dir_paths);
    }
    if let Some(fallback) = fallback
        && fallback.exists()
    {
        paths.push(fallback.to_path_buf());
    }
    paths
}

fn parse_sysctl_line(line: &str) -> Option<(String, String)> {
    let line = line.split('#').next()?.trim();
    if line.is_empty() {
        return None;
    }
    if let Some((key, value)) = line.split_once('=') {
        return Some((key.trim().to_string(), value.trim().to_string()));
    }
    let mut fields = line.split_whitespace();
    let key = fields.next()?;
    let value = fields.collect::<Vec<_>>().join(" ");
    if value.is_empty() {
        None
    } else {
        Some((key.to_string(), value))
    }
}

fn sysctl_proc_path(key: &str, span: Span) -> Result<PathBuf, RuntimeError> {
    if key.is_empty()
        || key.contains('\0')
        || key.contains("..")
        || key
            .split('.')
            .any(|part| part.is_empty() || part.contains('/'))
    {
        return Err(RuntimeError::new("linux-sysctl", "invalid sysctl key").with_span(span));
    }
    Ok(Path::new("/proc/sys").join(key.replace('.', "/")))
}
