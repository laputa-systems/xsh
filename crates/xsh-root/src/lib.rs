//! Kernel-enforced rooted file opening for Linux 5.6+ and macOS 26+.
//!
//! Given a [`Root`] anchored to directory `R`, every successful open through
//! [`Root`] resolves entirely beneath `R`. Untrusted paths cannot escape using
//! `..`, absolute paths, symbolic links, or concurrent pathname manipulation.
//!
//! This is pathname-resolution protection, not a process sandbox. It does not
//! constrain code that opens host paths directly, mount points below `R`, or
//! special files that exist below `R`.

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
compile_error!("xsh-root supports only Linux 5.6+ and macOS 26+");

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

use std::ffi::CString;
use std::fs::File;
use std::io;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// An open directory descriptor that anchors kernel-enforced rooted opens.
pub struct Root {
    fd: OwnedFd,
}

impl Root {
    /// Opens `path` as a trusted root directory.
    ///
    /// Root creation is intentionally ambient: this establishes the trusted
    /// anchor used for later confined opens.
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path_to_cstring(path.as_ref())?;
        platform::open_root(&path).map(|fd| Self { fd })
    }

    /// Opens an existing file beneath this root for reading.
    pub fn open_file(&self, path: impl AsRef<Path>) -> io::Result<File> {
        self.open_with(path, &OpenOptions::new().read(true))
    }

    /// Opens a file beneath this root using the supplied limited options.
    pub fn open_with(&self, path: impl AsRef<Path>, options: &OpenOptions) -> io::Result<File> {
        let path = relative_path_to_cstring(path.as_ref())?;
        let (flags, mode) = options.flags()?;
        platform::open_file(self.fd.as_fd(), &path, flags, mode)
    }

    /// Creates or truncates a file beneath this root for writing.
    pub fn create(&self, path: impl AsRef<Path>) -> io::Result<File> {
        self.open_with(
            path,
            &OpenOptions::new().write(true).create(true).truncate(true),
        )
    }

    /// Opens a directory beneath this root as a new confined root.
    ///
    /// This is intentionally the only directory operation: it composes the
    /// same kernel-enforced opening boundary for callers that need a narrower
    /// anchor.
    pub fn open_dir(&self, path: impl AsRef<Path>) -> io::Result<Self> {
        let path = relative_path_to_cstring(path.as_ref())?;
        platform::open_dir(self.fd.as_fd(), &path).map(|fd| Self { fd })
    }
}

impl AsFd for Root {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

/// Limited options for [`Root::open_with`].
///
/// The safe surface deliberately exposes ordinary opening intent rather than
/// platform flags. Every option is still opened through the root's confined
/// pathname resolver.
#[derive(Clone, Debug, Default)]
pub struct OpenOptions {
    read: bool,
    write: bool,
    append: bool,
    truncate: bool,
    create: bool,
    create_new: bool,
    mode: u32,
}

impl OpenOptions {
    #[must_use]
    pub fn new() -> Self {
        Self {
            mode: 0o666,
            ..Self::default()
        }
    }

    pub fn read(&mut self, value: bool) -> &mut Self {
        self.read = value;
        self
    }

    pub fn write(&mut self, value: bool) -> &mut Self {
        self.write = value;
        self
    }

    pub fn append(&mut self, value: bool) -> &mut Self {
        self.append = value;
        self
    }

    pub fn truncate(&mut self, value: bool) -> &mut Self {
        self.truncate = value;
        self
    }

    pub fn create(&mut self, value: bool) -> &mut Self {
        self.create = value;
        self
    }

    pub fn create_new(&mut self, value: bool) -> &mut Self {
        self.create_new = value;
        self
    }

    pub fn mode(&mut self, mode: u32) -> &mut Self {
        self.mode = mode;
        self
    }

    fn flags(&self) -> io::Result<(libc::c_int, libc::mode_t)> {
        let writes = self.write || self.append;
        if !self.read && !writes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "rooted open requires read, write, or append access",
            ));
        }
        if (self.create || self.create_new || self.truncate) && !writes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "create and truncate require write or append access",
            ));
        }
        if self.truncate && self.append {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "truncate and append cannot be combined",
            ));
        }

        let mut flags = match (self.read, writes) {
            (true, true) => libc::O_RDWR,
            (true, false) => libc::O_RDONLY,
            (false, true) => libc::O_WRONLY,
            (false, false) => unreachable!("access intent was validated"),
        };
        if self.append {
            flags |= libc::O_APPEND;
        }
        if self.truncate {
            flags |= libc::O_TRUNC;
        }
        if self.create_new {
            flags |= libc::O_CREAT | libc::O_EXCL;
        } else if self.create {
            flags |= libc::O_CREAT;
        }
        Ok((flags, self.mode as libc::mode_t))
    }
}

fn path_to_cstring(path: &Path) -> io::Result<CString> {
    if path.as_os_str().is_empty() {
        return Err(invalid_path("path cannot be empty"));
    }
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| invalid_path("path cannot contain an embedded NUL byte"))
}

fn relative_path_to_cstring(path: &Path) -> io::Result<CString> {
    if path.is_absolute() {
        return Err(invalid_path("rooted path must be relative"));
    }
    path_to_cstring(path)
}

fn invalid_path(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

#[cfg(target_os = "linux")]
mod platform {
    pub(super) use crate::linux::{open_dir, open_file, open_root};
}

#[cfg(target_os = "macos")]
mod platform {
    pub(super) use crate::macos::{open_dir, open_file, open_root};
}
