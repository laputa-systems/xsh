use crate::Root;
use rustix::fd::{AsFd, BorrowedFd, OwnedFd};
use rustix::fs::{
    AtFlags, Dir, FileType, Gid, Mode, OFlags, RenameFlags, Uid, chownat, fchmod, linkat, mkdirat,
    openat, readlinkat, renameat, renameat_with, statat, symlinkat, unlinkat,
};
use std::ffi::{OsStr, OsString};
use std::io;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileKind {
    File,
    Directory,
    Symlink,
    Fifo,
    CharDevice,
    BlockDevice,
    Socket,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeKind {
    Fifo,
    CharDevice,
    BlockDevice,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenameMode {
    NoReplace,
    Replace,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Metadata {
    pub kind: FileKind,
    pub mode: u32,
    pub size: u64,
    pub uid: u32,
    pub gid: u32,
    pub inode: u64,
    pub device: u64,
    pub link_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirEntry {
    pub name: OsString,
    pub kind: FileKind,
}

enum Directory<'a> {
    Root(BorrowedFd<'a>),
    Opened(Root),
}

impl Directory<'_> {
    fn fd(&self) -> BorrowedFd<'_> {
        match self {
            Self::Root(fd) => *fd,
            Self::Opened(root) => root.fd.as_fd(),
        }
    }
}

struct Target<'a> {
    parent: Directory<'a>,
    leaf: OsString,
}

impl Root {
    fn target(&self, path: &Path) -> io::Result<Target<'_>> {
        let mut components = Vec::new();
        for component in path.components() {
            match component {
                Component::Normal(part) => components.push(part),
                _ => {
                    return Err(invalid(
                        "a rooted mutation path must be relative and contain no `.` or `..`",
                    ));
                }
            }
        }
        let Some((leaf, parents)) = components.split_last() else {
            return Err(invalid("a rooted mutation path cannot be empty"));
        };
        let parent = if parents.is_empty() {
            Directory::Root(self.fd.as_fd())
        } else {
            let mut joined = std::path::PathBuf::new();
            joined.extend(parents);
            Directory::Opened(self.open_dir_strict(joined)?)
        };
        Ok(Target {
            parent,
            leaf: (*leaf).to_os_string(),
        })
    }

    fn directory(&self, path: &Path) -> io::Result<Directory<'_>> {
        if path.as_os_str() == "." {
            Ok(Directory::Root(self.fd.as_fd()))
        } else {
            self.open_dir_strict(path).map(Directory::Opened)
        }
    }

    pub fn mkdir(&self, path: impl AsRef<Path>, mode: u32) -> io::Result<()> {
        let target = self.target(path.as_ref())?;
        mkdirat(target.parent.fd(), target.leaf.as_os_str(), mode_bits(mode)).map_err(io::Error::from)
    }

    pub fn symlink(&self, target: impl AsRef<OsStr>, link: impl AsRef<Path>) -> io::Result<()> {
        let at = self.target(link.as_ref())?;
        symlinkat(target.as_ref(), at.parent.fd(), at.leaf.as_os_str()).map_err(io::Error::from)
    }

    pub fn hard_link(
        &self,
        existing: impl AsRef<Path>,
        new: impl AsRef<Path>,
    ) -> io::Result<()> {
        let from = self.target(existing.as_ref())?;
        let to = self.target(new.as_ref())?;
        linkat(
            from.parent.fd(),
            from.leaf.as_os_str(),
            to.parent.fd(),
            to.leaf.as_os_str(),
            AtFlags::empty(),
        )
        .map_err(io::Error::from)
    }

    pub fn rename(
        &self,
        from: impl AsRef<Path>,
        to: impl AsRef<Path>,
        mode: RenameMode,
    ) -> io::Result<()> {
        let source = self.target(from.as_ref())?;
        let destination = self.target(to.as_ref())?;
        match mode {
            RenameMode::NoReplace => renameat_with(
                source.parent.fd(),
                source.leaf.as_os_str(),
                destination.parent.fd(),
                destination.leaf.as_os_str(),
                RenameFlags::NOREPLACE,
            ),
            RenameMode::Replace => renameat(
                source.parent.fd(),
                source.leaf.as_os_str(),
                destination.parent.fd(),
                destination.leaf.as_os_str(),
            ),
        }
        .map_err(io::Error::from)
    }

    pub fn remove_file(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let target = self.target(path.as_ref())?;
        unlinkat(target.parent.fd(), target.leaf.as_os_str(), AtFlags::empty())
            .map_err(io::Error::from)
    }

    pub fn remove_dir(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let target = self.target(path.as_ref())?;
        unlinkat(target.parent.fd(), target.leaf.as_os_str(), AtFlags::REMOVEDIR)
            .map_err(io::Error::from)
    }

    pub fn set_mode(&self, path: impl AsRef<Path>, mode: u32) -> io::Result<()> {
        let target = self.target(path.as_ref())?;
        let opened = openat(
            target.parent.fd(),
            target.leaf.as_os_str(),
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        fchmod(&opened, mode_bits(mode)).map_err(io::Error::from)
    }

    pub fn set_owner(&self, path: impl AsRef<Path>, uid: u32, gid: u32) -> io::Result<()> {
        let target = self.target(path.as_ref())?;
        chownat(
            target.parent.fd(),
            target.leaf.as_os_str(),
            Some(Uid::from_raw(uid)),
            Some(Gid::from_raw(gid)),
            AtFlags::SYMLINK_NOFOLLOW,
        )
        .map_err(io::Error::from)
    }

    #[cfg(target_os = "linux")]
    pub fn make_node(
        &self,
        path: impl AsRef<Path>,
        kind: NodeKind,
        mode: u32,
        major: u32,
        minor: u32,
    ) -> io::Result<()> {
        let target = self.target(path.as_ref())?;
        let file_type = match kind {
            NodeKind::Fifo => FileType::Fifo,
            NodeKind::CharDevice => FileType::CharacterDevice,
            NodeKind::BlockDevice => FileType::BlockDevice,
        };
        rustix::fs::mknodat(
            target.parent.fd(),
            target.leaf.as_os_str(),
            file_type,
            mode_bits(mode),
            rustix::fs::makedev(major, minor),
        )
        .map_err(io::Error::from)
    }

    #[cfg(not(target_os = "linux"))]
    pub fn make_node(
        &self,
        _path: impl AsRef<Path>,
        _kind: NodeKind,
        _mode: u32,
        _major: u32,
        _minor: u32,
    ) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "special files cannot be created on this platform",
        ))
    }

    pub fn metadata(&self, path: impl AsRef<Path>) -> io::Result<Metadata> {
        let path = path.as_ref();
        let stat = if path.as_os_str() == "." {
            rustix::fs::fstat(&self.fd)?
        } else {
            let target = self.target(path)?;
            statat(
                target.parent.fd(),
                target.leaf.as_os_str(),
                AtFlags::SYMLINK_NOFOLLOW,
            )?
        };
        Ok(Metadata {
            kind: file_kind(FileType::from_raw_mode(stat.st_mode)),
            mode: narrow::<_, u32>(stat.st_mode) & 0o7777,
            size: u64::try_from(stat.st_size).unwrap_or(0),
            uid: stat.st_uid,
            gid: stat.st_gid,
            inode: stat.st_ino,
            device: narrow(stat.st_dev),
            link_count: narrow(stat.st_nlink),
        })
    }

    pub fn read_link(&self, path: impl AsRef<Path>) -> io::Result<OsString> {
        let target = self.target(path.as_ref())?;
        let link = readlinkat(target.parent.fd(), target.leaf.as_os_str(), Vec::new())?;
        Ok(OsString::from_vec(link.into_bytes()))
    }

    pub fn read_dir(&self, path: impl AsRef<Path>) -> io::Result<Vec<DirEntry>> {
        let path = path.as_ref();
        let directory = if path.as_os_str() == "." {
            self.open_readable_dir(".")?
        } else {
            self.open_dir_strict(path)?.open_readable_dir(".")?
        };
        let owned: OwnedFd = directory.into();
        let mut entries = Vec::new();
        for entry in Dir::read_from(&owned)? {
            let entry = entry?;
            let name = entry.file_name().to_bytes();
            if name == b"." || name == b".." {
                continue;
            }
            let kind = match entry.file_type() {
                FileType::Unknown => {
                    let stat = statat(&owned, entry.file_name(), AtFlags::SYMLINK_NOFOLLOW)?;
                    file_kind(FileType::from_raw_mode(stat.st_mode))
                }
                known => file_kind(known),
            };
            entries.push(DirEntry {
                name: OsStr::from_bytes(name).to_os_string(),
                kind,
            });
        }
        Ok(entries)
    }

    pub fn open_directory_handle(&self, path: impl AsRef<Path>) -> io::Result<Root> {
        match self.directory(path.as_ref())? {
            Directory::Opened(root) => Ok(root),
            Directory::Root(fd) => rustix::io::fcntl_dupfd_cloexec(fd, 0)
                .map(|fd| Root { fd })
                .map_err(io::Error::from),
        }
    }
}

fn narrow<T: TryInto<U>, U: Default>(value: T) -> U {
    value.try_into().unwrap_or_default()
}

fn mode_bits(mode: u32) -> Mode {
    Mode::from_raw_mode((mode & 0o7777) as rustix::fs::RawMode)
}

fn file_kind(kind: FileType) -> FileKind {
    match kind {
        FileType::Directory => FileKind::Directory,
        FileType::Symlink => FileKind::Symlink,
        FileType::Fifo => FileKind::Fifo,
        FileType::CharacterDevice => FileKind::CharDevice,
        FileType::BlockDevice => FileKind::BlockDevice,
        FileType::Socket => FileKind::Socket,
        FileType::RegularFile | FileType::Unknown => FileKind::File,
    }
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
