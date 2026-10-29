#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Format {
    Tar,
    Zip,
    Cpio,
}

impl Format {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Tar => "tar",
            Self::Zip => "zip",
            Self::Cpio => "cpio",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
    Hardlink,
    Fifo,
    CharDevice,
    BlockDevice,
    Other,
}

impl EntryKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::File => "File",
            Self::Directory => "Directory",
            Self::Symlink => "Symlink",
            Self::Hardlink => "Hardlink",
            Self::Fifo => "Fifo",
            Self::CharDevice => "CharDevice",
            Self::BlockDevice => "BlockDevice",
            Self::Other => "Other",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntryMeta {
    pub name: Vec<u8>,
    pub kind: EntryKind,
    pub size: Option<u64>,
    pub mode: Option<u32>,
    pub modified: Option<i64>,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    pub link: Option<Vec<u8>>,
    pub device: Option<(u32, u32)>,
    pub link_count: Option<u32>,
}

impl EntryMeta {
    pub fn new(name: Vec<u8>, kind: EntryKind) -> Self {
        Self {
            name,
            kind,
            size: None,
            mode: None,
            modified: None,
            uid: None,
            gid: None,
            link: None,
            device: None,
            link_count: None,
        }
    }

    pub fn index_bytes(&self) -> u64 {
        const FIXED: u64 = 96;
        FIXED
            + self.name.len() as u64
            + self.link.as_ref().map_or(0, |link| link.len() as u64)
    }
}
