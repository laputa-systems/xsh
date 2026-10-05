//! Raw extended attributes preserve ACL, label, and capability payloads without
//! decoding or rewriting their binary representation.

use super::name_error_path;
use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;
use std::path::PathBuf;

/// Size and content can change between the host calls. Retry ERANGE a bounded
/// number of times, then report contention rather than spinning indefinitely.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn read_attribute(mut read: impl FnMut(&mut [u8]) -> rustix::io::Result<usize>) -> std::io::Result<Vec<u8>> {
    for _ in 0..8 {
        let size = read(&mut [])?;
        // A zero-sized fetch is a size query, so use at least one byte to read
        // an attribute that becomes nonempty between these calls.
        let mut value = Vec::new();
        value.try_reserve_exact(size.max(1)).map_err(std::io::Error::other)?;
        value.resize(size.max(1), 0);
        match read(&mut value) {
            Ok(count) => {
                value.truncate(count);
                return Ok(value);
            }
            Err(rustix::io::Errno::RANGE) => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(std::io::Error::from_raw_os_error(libc::EAGAIN))
}

pub(crate) fn xattr_list(path: PathBuf, follow_symlinks: bool, span: Span) -> Result<Value, RuntimeError> {
    let shown = path.display().to_string();
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let result = read_attribute(|buffer| {
        if follow_symlinks {
            rustix::fs::listxattr(&path, buffer)
        } else {
            rustix::fs::llistxattr(&path, buffer)
        }
    });
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let result: std::io::Result<Vec<u8>> = {
        let _ = (path, follow_symlinks);
        Err(std::io::Error::from_raw_os_error(libc::ENOTSUP))
    };
    let bytes = name_error_path(&shown, result.map_err(|error| RuntimeError::host("fs-xattr-list", &error).with_span(span)))?;
    let mut names = Vec::new();
    for bytes in bytes.split(|byte| *byte == 0).filter(|bytes| !bytes.is_empty()) {
        let name = std::str::from_utf8(bytes).map_err(|error| {
            RuntimeError::new("fs-xattr-list", format!("attribute name is not UTF-8: {error}")).with_span(span)
        })?;
        names.push(name.to_owned());
    }
    // The host does not promise an order; callers need deterministic names.
    names.sort();
    Ok(Value::List(names.into_iter().map(|name| Value::Str(name.into())).collect()))
}

pub(crate) fn xattr_get(path: PathBuf, name: &str, follow_symlinks: bool, span: Span) -> Result<Value, RuntimeError> {
    let shown = path.display().to_string();
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let result = read_attribute(|buffer| {
        if follow_symlinks {
            rustix::fs::getxattr(&path, name, buffer)
        } else {
            rustix::fs::lgetxattr(&path, name, buffer)
        }
    });
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let result: std::io::Result<Vec<u8>> = {
        let _ = (path, name, follow_symlinks);
        Err(std::io::Error::from_raw_os_error(libc::ENOTSUP))
    };
    name_error_path(&shown, result.map(Value::Bytes).map_err(|error| RuntimeError::host("fs-xattr-get", &error).with_span(span)))
}

pub(crate) fn xattr_set(path: PathBuf, name: &str, value: &[u8], mode: &str, follow_symlinks: bool, span: Span) -> Result<(), RuntimeError> {
    if !matches!(mode, "upsert" | "create" | "replace") {
        return Err(RuntimeError::new("fs-xattr-set", "mode must be `upsert`, `create`, or `replace`").with_span(span));
    }
    let shown = path.display().to_string();
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let result = {
        use rustix::fs::XattrFlags;
        let flags = match mode {
            "create" => XattrFlags::CREATE,
            "replace" => XattrFlags::REPLACE,
            _ => XattrFlags::empty(),
        };
        if follow_symlinks {
            rustix::fs::setxattr(&path, name, value, flags)
        } else {
            rustix::fs::lsetxattr(&path, name, value, flags)
        }
    };
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let result: rustix::io::Result<()> = {
        let _ = (path, name, value, follow_symlinks);
        Err(rustix::io::Errno::NOTSUP)
    };
    name_error_path(&shown, result.map_err(|error| RuntimeError::host("fs-xattr-set", &error).with_span(span)))
}

pub(crate) fn xattr_remove(path: PathBuf, name: &str, follow_symlinks: bool, span: Span) -> Result<(), RuntimeError> {
    let shown = path.display().to_string();
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let result = if follow_symlinks {
        rustix::fs::removexattr(&path, name)
    } else {
        rustix::fs::lremovexattr(&path, name)
    };
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let result: rustix::io::Result<()> = {
        let _ = (path, name, follow_symlinks);
        Err(rustix::io::Errno::NOTSUP)
    };
    name_error_path(&shown, result.map_err(|error| RuntimeError::host("fs-xattr-remove", &error).with_span(span)))
}
