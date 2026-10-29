use crate::error::{Error, ErrorKind, Result};
use crate::extract::Collision;
use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use xsh_root::{OpenOptions, RenameMode, Root};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) struct PendingOutput {
    parent: Root,
    temp_name: String,
    leaf: OsString,
    identity: (u64, u64),
}

impl PendingOutput {
    pub(crate) fn open(dest: &Path) -> Result<(Self, File)> {
        let leaf = dest
            .file_name()
            .ok_or_else(|| Error::options("destination has no final name"))?
            .to_os_string();
        let parent_path = match dest.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
            _ => PathBuf::from("."),
        };
        let parent = Root::open(&parent_path).map_err(|error| Error::from_open(&error))?;
        for _ in 0..16 {
            let name = format!(
                ".xsh-archive-out-{}-{}",
                std::process::id(),
                TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
            );
            match parent.open_with(
                &name,
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .no_symlinks(true)
                    .mode(0o666),
            ) {
                Ok(file) => {
                    let metadata = file.metadata().map_err(|error| Error::from_host(&error))?;
                    let output = Self {
                        parent,
                        temp_name: name,
                        leaf,
                        identity: (metadata.dev(), metadata.ino()),
                    };
                    return Ok((output, file));
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(Error::from_open(&error)),
            }
        }
        Err(Error::new(
            ErrorKind::Io,
            "could not create a unique temporary file",
        ))
    }

    pub(crate) const fn identity(&self) -> (u64, u64) {
        self.identity
    }

    pub(crate) fn publish(self, collision: Collision) -> Result<()> {
        let mode = match collision {
            Collision::Refuse => RenameMode::NoReplace,
            Collision::Replace => RenameMode::Replace,
        };
        let renamed = self
            .parent
            .rename(Path::new(&self.temp_name), Path::new(&self.leaf), mode);
        match renamed {
            Ok(()) => Ok(()),
            Err(error) => {
                let failure = if error.kind() == io::ErrorKind::AlreadyExists {
                    Error::new(ErrorKind::Collision, "destination exists")
                } else {
                    Error::from_host(&error)
                };
                Err(self.discard(failure))
            }
        }
    }

    pub(crate) fn discard(self, error: Error) -> Error {
        match self.parent.remove_file(Path::new(&self.temp_name)) {
            Ok(()) => error,
            Err(cleanup) if cleanup.kind() == io::ErrorKind::NotFound => error,
            Err(cleanup) => error.with_cleanup(&Error::from_host(&cleanup)),
        }
    }
}
