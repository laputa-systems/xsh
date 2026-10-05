use super::name_error_path;
use crate::runtime::value::{RecordMap, RuntimeError, Value};
use crate::source::Span;
use rustix::fs::{self as rfs, CWD, Mode, OFlags};
use std::path::PathBuf;
use std::sync::Arc;

/// O_NONBLOCK keeps a FIFO without a writer from hanging before the kernel
/// can reject synchronization. Synchronization itself waits for writeback.
pub(crate) fn sync_path(path: PathBuf, mode: &str, span: Span) -> Result<(), RuntimeError> {
    if !matches!(mode, "all" | "data" | "filesystem") {
        return Err(RuntimeError::new("fs-sync-path", "mode must be `all`, `data`, or `filesystem`").with_span(span));
    }
    let shown = path.display().to_string();
    let result = (|| -> rustix::io::Result<()> {
        let file = rfs::openat(CWD, &path, OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC, Mode::empty())?;
        match mode {
            "all" => rfs::fsync(&file),
            #[cfg(target_os = "linux")]
            "data" => rfs::fdatasync(&file),
            #[cfg(target_os = "linux")]
            "filesystem" => rfs::syncfs(&file),
            _ => Err(rustix::io::Errno::NOTSUP),
        }
    })();
    name_error_path(&shown, result.map_err(|error| RuntimeError::host("fs-sync-path", &error).with_span(span)))
}

/// Both directory entries are exchanged in one kernel operation. Unsupported
/// filesystems fail rather than using a temporary name with observable gaps.
pub(crate) fn rename_exchange(source: PathBuf, dest: PathBuf, span: Span) -> Result<(), RuntimeError> {
    let shown = format!("{} <-> {}", source.display(), dest.display());
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let result = rfs::renameat_with(CWD, &source, CWD, &dest, rfs::RenameFlags::EXCHANGE);
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let result: rustix::io::Result<()> = {
        let _ = (source, dest);
        Err(rustix::io::Errno::NOTSUP)
    };
    name_error_path(&shown, result.map_err(|error| RuntimeError::host("fs-rename-exchange", &error).with_span(span)))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn path_limit(path: &std::ffi::CStr, selector: libc::c_int) -> std::io::Result<i64> {
    // POSIX uses -1 for both an error and an indeterminate limit. Clearing
    // errno lets us report the latter explicitly instead of inventing a bound.
    unsafe {
        #[cfg(target_os = "linux")]
        let errno = libc::__errno_location();
        #[cfg(target_os = "macos")]
        let errno = libc::__error();
        *errno = 0;
        let limit = libc::pathconf(path.as_ptr(), selector);
        if limit == -1 {
            if *errno == 0 {
                return Err(std::io::Error::other("path limit is indeterminate"));
            }
            return Err(std::io::Error::from_raw_os_error(*errno));
        }
        Ok(limit as i64)
    }
}

pub(crate) fn path_limits(path: PathBuf, span: Span) -> Result<Value, RuntimeError> {
    let shown = path.display().to_string();
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let result = (|| -> std::io::Result<Value> {
        use std::os::unix::ffi::OsStrExt;
        let path = std::ffi::CString::new(path.as_os_str().as_bytes())
            .map_err(|_| std::io::Error::from_raw_os_error(libc::EINVAL))?;
        let name_max = path_limit(&path, libc::_PC_NAME_MAX)?;
        let path_max = path_limit(&path, libc::_PC_PATH_MAX)?;
        Ok(Value::Record(RecordMap::from([
            (Arc::from("name_max"), Value::Int(name_max)),
            (Arc::from("path_max"), Value::Int(path_max)),
        ])))
    })();
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let result: std::io::Result<Value> = {
        let _ = path;
        Err(std::io::Error::from_raw_os_error(libc::ENOTSUP))
    };
    name_error_path(&shown, result.map_err(|error| RuntimeError::host("fs-path-limits", &error).with_span(span)))
}

/// Effective credentials include supplementary groups, ACLs, and host privilege
/// rules. This is an observation: a later open must still enforce permissions.
pub(crate) fn access(
    path: PathBuf,
    read: bool,
    write: bool,
    execute: bool,
    follow_symlinks: bool,
    span: Span,
) -> Result<bool, RuntimeError> {
    use rustix::fs::{Access, AtFlags};
    let mut requested = Access::empty();
    if read {
        requested |= Access::READ_OK;
    }
    if write {
        requested |= Access::WRITE_OK;
    }
    if execute {
        requested |= Access::EXEC_OK;
    }
    let mut flags = AtFlags::EACCESS;
    if !follow_symlinks {
        flags |= AtFlags::SYMLINK_NOFOLLOW;
    }
    let shown = path.display().to_string();
    let result = match rfs::accessat(CWD, &path, requested, flags) {
        Ok(()) => Ok(true),
        Err(rustix::io::Errno::ACCESS | rustix::io::Errno::PERM) => Ok(false),
        Err(error) => Err(RuntimeError::host("fs-access", &error).with_span(span)),
    };
    name_error_path(&shown, result)
}
