//! Payload-free enums that standard modules declare for their closed option sets.
//!
//! A variant is selected only by its target-typed spelling (`.Tar`): the
//! expected type is the parameter, record field or comparison operand that
//! names the enum. Variants are never bare names or module members, so two
//! enums may share a variant name without a collision.

pub struct BuiltinEnum {
    pub name: &'static str,
    pub summary: &'static str,
    pub variants: &'static [BuiltinVariant],
}

pub struct BuiltinVariant {
    pub name: &'static str,
    pub summary: &'static str,
}

const fn variant(name: &'static str, summary: &'static str) -> BuiltinVariant {
    BuiltinVariant { name, summary }
}

pub const ARCHIVE_FORMAT: &str = "archive.Format";
pub const ARCHIVE_COMPRESSION: &str = "archive.Compression";
pub const ARCHIVE_COLLISION: &str = "archive.Collision";
pub const ARCHIVE_LINKS: &str = "archive.Links";
pub const ARCHIVE_SPECIAL: &str = "archive.Special";
pub const ARCHIVE_MODES: &str = "archive.Modes";
pub const ARCHIVE_OWNERSHIP: &str = "archive.Ownership";
pub const ARCHIVE_ENTRY_KIND: &str = "archive.EntryKind";

static BUILTIN_ENUMS: &[BuiltinEnum] = &[
    BuiltinEnum {
        name: ARCHIVE_FORMAT,
        summary: "The container format of an archive.",
        variants: &[
            variant("Tar", "A tar archive: ustar, GNU or pax headers."),
            variant("Zip", "A ZIP archive. It needs a seekable input and compresses its own entries."),
            variant("Cpio", "A cpio archive in the newc (070701) or crc (070702) format."),
        ],
    },
    BuiltinEnum {
        name: ARCHIVE_COMPRESSION,
        summary: "The compression wrapped around an archive or a single file.",
        variants: &[
            variant("Auto", "Detect from the first bytes when reading, from the destination suffix when writing."),
            variant("Plain", "No compression."),
            variant("Gzip", "gzip framing."),
            variant("Bzip2", "bzip2."),
            variant("Xz", "xz."),
            variant("Lzma", "The legacy lzma-alone format."),
            variant("Zstd", "Zstandard."),
        ],
    },
    BuiltinEnum {
        name: ARCHIVE_COLLISION,
        summary: "What to do when a name is already taken.",
        variants: &[
            variant("Refuse", "Fail with Collision and publish nothing."),
            variant("Replace", "Replace an existing file, link or special file; never a directory tree."),
        ],
    },
    BuiltinEnum {
        name: ARCHIVE_LINKS,
        summary: "Which links extraction may create.",
        variants: &[
            variant("Refuse", "Fail on any symbolic or hard link."),
            variant("Contained", "Allow relative symbolic links that stay inside the extraction root and hard links to earlier files of the same extraction."),
            variant("Verbatim", "Create links exactly as stored, including absolute targets. Extraction never follows a link."),
        ],
    },
    BuiltinEnum {
        name: ARCHIVE_SPECIAL,
        summary: "Whether extraction may create FIFOs and device nodes.",
        variants: &[
            variant("Refuse", "Fail on a FIFO, character device or block device."),
            variant("Create", "Create them; a refusal by the kernel is an error."),
        ],
    },
    BuiltinEnum {
        name: ARCHIVE_MODES,
        summary: "Which mode bits extraction applies.",
        variants: &[
            variant("Plain", "Permission bits only (0o777); setuid, setgid and sticky are dropped."),
            variant("Exact", "All twelve mode bits as stored."),
        ],
    },
    BuiltinEnum {
        name: ARCHIVE_OWNERSHIP,
        summary: "Whether extraction applies stored owners.",
        variants: &[
            variant("Ignore", "Files belong to the extracting user."),
            variant("Apply", "Apply the stored uid and gid; a refusal by the kernel is an error."),
        ],
    },
    BuiltinEnum {
        name: ARCHIVE_ENTRY_KIND,
        summary: "The kind of an archive member.",
        variants: &[
            variant("File", "A regular file."),
            variant("Directory", "A directory."),
            variant("Symlink", "A symbolic link."),
            variant("Hardlink", "A hard link to another member."),
            variant("Fifo", "A named pipe."),
            variant("CharDevice", "A character device node."),
            variant("BlockDevice", "A block device node."),
            variant("Other", "An entry the format could not classify or this build does not support."),
        ],
    },
];

pub fn builtin_enums() -> &'static [BuiltinEnum] {
    BUILTIN_ENUMS
}

pub fn builtin_enum(name: &str) -> Option<&'static BuiltinEnum> {
    BUILTIN_ENUMS.iter().find(|entry| entry.name == name)
}
