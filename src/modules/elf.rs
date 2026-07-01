use crate::runtime::value::RuntimeError;
use crate::source::Span;
use std::path::Path;

const EI_CLASS: usize = 4;
const EI_DATA: usize = 5;
const EI_OSABI: usize = 7;
const ELF_MAGIC: &[u8; 4] = b"\x7fELF";
const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;
const PT_INTERP: u32 = 3;
const SHT_DYNAMIC: u32 = 6;
const DT_NULL: i64 = 0;
const DT_NEEDED: i64 = 1;
const DT_STRTAB: i64 = 5;
const DT_STRSZ: i64 = 10;
const DT_SONAME: i64 = 14;
const DT_RPATH: i64 = 15;
const DT_RUNPATH: i64 = 29;
const DT_FLAGS: i64 = 30;
const DT_FLAGS_1: i64 = 0x6fff_fffb;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ElfInfo {
    pub(crate) class: String,
    pub(crate) endian: String,
    pub(crate) machine: String,
    pub(crate) os_abi: String,
    pub(crate) elf_type: String,
    pub(crate) interpreter: String,
    pub(crate) soname: String,
    pub(crate) needed: Vec<String>,
    pub(crate) rpath: String,
    pub(crate) runpath: String,
    pub(crate) flags: Vec<String>,
    pub(crate) dynamic_tags: Vec<ElfDynamicTag>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ElfDynamicTag {
    pub(crate) tag: String,
    pub(crate) value: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ElfClass {
    Elf32,
    Elf64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Endian {
    Little,
    Big,
}

#[derive(Clone, Copy, Debug)]
struct ElfHeader {
    class: ElfClass,
    endian: Endian,
    os_abi: u8,
    elf_type: u16,
    machine: u16,
    flags: u32,
    phoff: u64,
    shoff: u64,
    phentsize: u16,
    phnum: u16,
    shentsize: u16,
    shnum: u16,
}

#[derive(Clone, Copy, Debug)]
struct ProgramHeader {
    p_type: u32,
    offset: u64,
    vaddr: u64,
    filesz: u64,
}

#[derive(Clone, Copy, Debug)]
struct SectionHeader {
    sh_type: u32,
    offset: u64,
    size: u64,
    link: u32,
    entsize: u64,
}

#[derive(Clone, Copy, Debug)]
struct DynamicEntry {
    tag: i64,
    value: u64,
}

pub(crate) fn inspect_path(path: &Path, span: Span) -> Result<ElfInfo, RuntimeError> {
    let data = std::fs::read(path)
        .map_err(|error| RuntimeError::new("elf-read", error.to_string()).with_span(span))?;
    inspect_bytes(&data, span)
}

pub(crate) fn inspect_bytes(data: &[u8], span: Span) -> Result<ElfInfo, RuntimeError> {
    if data.len() < ELF_MAGIC.len() || &data[..4] != ELF_MAGIC {
        return Ok(not_elf());
    }

    let header = parse_header(data, span)?;
    let programs = parse_program_headers(data, header, span)?;
    let sections = parse_section_headers(data, header, span)?;
    let dynamic = dynamic_entries(data, header, &programs, &sections, span)?;
    let strtab = dynamic_strtab(data, &dynamic, &programs, &sections, span)?;

    let mut info = ElfInfo {
        class: match header.class {
            ElfClass::Elf32 => "ELF32".to_string(),
            ElfClass::Elf64 => "ELF64".to_string(),
        },
        endian: match header.endian {
            Endian::Little => "little".to_string(),
            Endian::Big => "big".to_string(),
        },
        machine: machine_name(header.machine).to_string(),
        os_abi: os_abi_name(header.os_abi).to_string(),
        elf_type: elf_type_name(header.elf_type).to_string(),
        interpreter: interpreter(data, &programs, span)?,
        soname: String::new(),
        needed: Vec::new(),
        rpath: String::new(),
        runpath: String::new(),
        flags: Vec::new(),
        dynamic_tags: Vec::new(),
    };

    if header.flags != 0 {
        info.flags.push(format!("e_flags=0x{:x}", header.flags));
    }

    for entry in dynamic {
        info.dynamic_tags.push(ElfDynamicTag {
            tag: dynamic_tag_name(entry.tag).to_string(),
            value: i64::try_from(entry.value).unwrap_or(i64::MAX),
        });

        match entry.tag {
            DT_NEEDED => info.needed.push(read_dynstr(strtab, entry.value, span)?),
            DT_SONAME => info.soname = read_dynstr(strtab, entry.value, span)?,
            DT_RPATH => info.rpath = read_dynstr(strtab, entry.value, span)?,
            DT_RUNPATH => info.runpath = read_dynstr(strtab, entry.value, span)?,
            DT_FLAGS => decode_dynamic_flags(entry.value, false, &mut info.flags),
            DT_FLAGS_1 => decode_dynamic_flags(entry.value, true, &mut info.flags),
            _ => {}
        }
    }

    Ok(info)
}

fn not_elf() -> ElfInfo {
    ElfInfo {
        class: String::new(),
        endian: String::new(),
        machine: String::new(),
        os_abi: String::new(),
        elf_type: "not-elf".to_string(),
        interpreter: String::new(),
        soname: String::new(),
        needed: Vec::new(),
        rpath: String::new(),
        runpath: String::new(),
        flags: Vec::new(),
        dynamic_tags: Vec::new(),
    }
}

fn parse_header(data: &[u8], span: Span) -> Result<ElfHeader, RuntimeError> {
    if data.len() < 16 {
        return Err(malformed("truncated ELF ident", span));
    }
    let class = match data[EI_CLASS] {
        1 => ElfClass::Elf32,
        2 => ElfClass::Elf64,
        _ => return Err(malformed("unsupported ELF class", span)),
    };
    let endian = match data[EI_DATA] {
        1 => Endian::Little,
        2 => Endian::Big,
        _ => return Err(malformed("unsupported ELF endian", span)),
    };

    let min = match class {
        ElfClass::Elf32 => 52,
        ElfClass::Elf64 => 64,
    };
    require_range(data, 0, min, span)?;

    Ok(match class {
        ElfClass::Elf32 => ElfHeader {
            class,
            endian,
            os_abi: data[EI_OSABI],
            elf_type: u16_at(data, 16, endian, span)?,
            machine: u16_at(data, 18, endian, span)?,
            flags: u32_at(data, 36, endian, span)?,
            phoff: u32_at(data, 28, endian, span)?.into(),
            shoff: u32_at(data, 32, endian, span)?.into(),
            phentsize: u16_at(data, 42, endian, span)?,
            phnum: u16_at(data, 44, endian, span)?,
            shentsize: u16_at(data, 46, endian, span)?,
            shnum: u16_at(data, 48, endian, span)?,
        },
        ElfClass::Elf64 => ElfHeader {
            class,
            endian,
            os_abi: data[EI_OSABI],
            elf_type: u16_at(data, 16, endian, span)?,
            machine: u16_at(data, 18, endian, span)?,
            flags: u32_at(data, 48, endian, span)?,
            phoff: u64_at(data, 32, endian, span)?,
            shoff: u64_at(data, 40, endian, span)?,
            phentsize: u16_at(data, 54, endian, span)?,
            phnum: u16_at(data, 56, endian, span)?,
            shentsize: u16_at(data, 58, endian, span)?,
            shnum: u16_at(data, 60, endian, span)?,
        },
    })
}

fn parse_program_headers(
    data: &[u8],
    header: ElfHeader,
    span: Span,
) -> Result<Vec<ProgramHeader>, RuntimeError> {
    if header.phoff == 0 || header.phnum == 0 {
        return Ok(Vec::new());
    }
    let min_size = match header.class {
        ElfClass::Elf32 => 32,
        ElfClass::Elf64 => 56,
    };
    if usize::from(header.phentsize) < min_size {
        return Err(malformed("program header entry is too small", span));
    }

    let mut headers = Vec::with_capacity(usize::from(header.phnum));
    for index in 0..header.phnum {
        let base = table_offset(header.phoff, header.phentsize, index, span)?;
        require_range(data, base, min_size, span)?;
        headers.push(match header.class {
            ElfClass::Elf32 => ProgramHeader {
                p_type: u32_at(data, base, header.endian, span)?,
                offset: u32_at(data, base + 4, header.endian, span)?.into(),
                vaddr: u32_at(data, base + 8, header.endian, span)?.into(),
                filesz: u32_at(data, base + 16, header.endian, span)?.into(),
            },
            ElfClass::Elf64 => ProgramHeader {
                p_type: u32_at(data, base, header.endian, span)?,
                offset: u64_at(data, base + 8, header.endian, span)?,
                vaddr: u64_at(data, base + 16, header.endian, span)?,
                filesz: u64_at(data, base + 32, header.endian, span)?,
            },
        });
    }
    Ok(headers)
}

fn parse_section_headers(
    data: &[u8],
    header: ElfHeader,
    span: Span,
) -> Result<Vec<SectionHeader>, RuntimeError> {
    if header.shoff == 0 || header.shnum == 0 {
        return Ok(Vec::new());
    }
    let min_size = match header.class {
        ElfClass::Elf32 => 40,
        ElfClass::Elf64 => 64,
    };
    if usize::from(header.shentsize) < min_size {
        return Err(malformed("section header entry is too small", span));
    }

    let mut headers = Vec::with_capacity(usize::from(header.shnum));
    for index in 0..header.shnum {
        let base = table_offset(header.shoff, header.shentsize, index, span)?;
        require_range(data, base, min_size, span)?;
        headers.push(match header.class {
            ElfClass::Elf32 => SectionHeader {
                sh_type: u32_at(data, base + 4, header.endian, span)?,
                offset: u32_at(data, base + 16, header.endian, span)?.into(),
                size: u32_at(data, base + 20, header.endian, span)?.into(),
                link: u32_at(data, base + 24, header.endian, span)?,
                entsize: u32_at(data, base + 36, header.endian, span)?.into(),
            },
            ElfClass::Elf64 => SectionHeader {
                sh_type: u32_at(data, base + 4, header.endian, span)?,
                offset: u64_at(data, base + 24, header.endian, span)?,
                size: u64_at(data, base + 32, header.endian, span)?,
                link: u32_at(data, base + 40, header.endian, span)?,
                entsize: u64_at(data, base + 56, header.endian, span)?,
            },
        });
    }
    Ok(headers)
}

fn dynamic_entries(
    data: &[u8],
    header: ElfHeader,
    programs: &[ProgramHeader],
    sections: &[SectionHeader],
    span: Span,
) -> Result<Vec<DynamicEntry>, RuntimeError> {
    if let Some(ph) = programs.iter().find(|ph| ph.p_type == PT_DYNAMIC) {
        return read_dynamic_entries(data, header, ph.offset, ph.filesz, 0, span);
    }

    if let Some(section) = sections
        .iter()
        .find(|section| section.sh_type == SHT_DYNAMIC)
    {
        return read_dynamic_entries(
            data,
            header,
            section.offset,
            section.size,
            section.entsize,
            span,
        );
    }

    Ok(Vec::new())
}

fn read_dynamic_entries(
    data: &[u8],
    header: ElfHeader,
    offset: u64,
    size: u64,
    explicit_entsize: u64,
    span: Span,
) -> Result<Vec<DynamicEntry>, RuntimeError> {
    let entsize = if explicit_entsize != 0 {
        explicit_entsize
    } else {
        match header.class {
            ElfClass::Elf32 => 8,
            ElfClass::Elf64 => 16,
        }
    };
    let min_size = match header.class {
        ElfClass::Elf32 => 8,
        ElfClass::Elf64 => 16,
    };
    if entsize < min_size {
        return Err(malformed("dynamic entry is too small", span));
    }

    let mut entries = Vec::new();
    let count = size / entsize;
    for index in 0..count {
        let base = usize::try_from(
            offset
                .checked_add(
                    index
                        .checked_mul(entsize)
                        .ok_or_else(|| malformed("ELF table offset overflow", span))?,
                )
                .ok_or_else(|| malformed("ELF table offset overflow", span))?,
        )
        .map_err(|_| malformed("ELF table offset overflow", span))?;
        require_range(data, base, usize::try_from(min_size).unwrap(), span)?;
        let entry = match header.class {
            ElfClass::Elf32 => DynamicEntry {
                tag: i64::from(i32_at(data, base, header.endian, span)?),
                value: u32_at(data, base + 4, header.endian, span)?.into(),
            },
            ElfClass::Elf64 => DynamicEntry {
                tag: i64_at(data, base, header.endian, span)?,
                value: u64_at(data, base + 8, header.endian, span)?,
            },
        };
        if entry.tag == DT_NULL {
            break;
        }
        entries.push(entry);
    }
    Ok(entries)
}

fn dynamic_strtab<'a>(
    data: &'a [u8],
    dynamic: &[DynamicEntry],
    programs: &[ProgramHeader],
    sections: &[SectionHeader],
    span: Span,
) -> Result<&'a [u8], RuntimeError> {
    if let Some(strtab_vaddr) = dynamic.iter().find(|entry| entry.tag == DT_STRTAB) {
        let size = dynamic
            .iter()
            .find(|entry| entry.tag == DT_STRSZ)
            .map(|entry| entry.value)
            .unwrap_or(0);
        if let Some(offset) = vaddr_to_offset(strtab_vaddr.value, programs) {
            return slice_u64(data, offset, size, span);
        }
    }

    if let Some(section) = sections
        .iter()
        .find(|section| section.sh_type == SHT_DYNAMIC)
        && let Some(strtab) = sections.get(section.link as usize)
    {
        return slice_u64(data, strtab.offset, strtab.size, span);
    }

    if dynamic
        .iter()
        .any(|entry| matches!(entry.tag, DT_NEEDED | DT_SONAME | DT_RPATH | DT_RUNPATH))
    {
        return Err(malformed("dynamic string table not found", span));
    }

    Ok(&[])
}

fn vaddr_to_offset(vaddr: u64, programs: &[ProgramHeader]) -> Option<u64> {
    programs
        .iter()
        .filter(|ph| ph.p_type == PT_LOAD)
        .find(|ph| vaddr >= ph.vaddr && vaddr < ph.vaddr.saturating_add(ph.filesz))
        .map(|ph| ph.offset + (vaddr - ph.vaddr))
}

fn interpreter(
    data: &[u8],
    programs: &[ProgramHeader],
    span: Span,
) -> Result<String, RuntimeError> {
    let Some(ph) = programs.iter().find(|ph| ph.p_type == PT_INTERP) else {
        return Ok(String::new());
    };
    let bytes = slice_u64(data, ph.offset, ph.filesz, span)?;
    read_cstr(bytes, 0, span)
}

fn read_dynstr(strtab: &[u8], offset: u64, span: Span) -> Result<String, RuntimeError> {
    let offset =
        usize::try_from(offset).map_err(|_| malformed("string table offset overflow", span))?;
    read_cstr(strtab, offset, span)
}

fn read_cstr(data: &[u8], offset: usize, span: Span) -> Result<String, RuntimeError> {
    if offset >= data.len() {
        return Err(malformed("string table offset is out of range", span));
    }
    let end = data[offset..]
        .iter()
        .position(|byte| *byte == 0)
        .map(|pos| offset + pos)
        .unwrap_or(data.len());
    String::from_utf8(data[offset..end].to_vec())
        .map_err(|_| malformed("ELF string is not valid UTF-8", span))
}

fn decode_dynamic_flags(value: u64, flags_1: bool, flags: &mut Vec<String>) {
    let known = if flags_1 {
        &[
            (0x1, "now"),
            (0x2, "global"),
            (0x4, "group"),
            (0x8, "nodelete"),
            (0x10, "loadfltr"),
            (0x20, "initfirst"),
            (0x40, "noopen"),
            (0x80, "origin"),
            (0x100, "direct"),
            (0x400, "interpose"),
            (0x800, "nodeflib"),
            (0x1000, "nodump"),
            (0x2000, "confalt"),
            (0x4000, "endfiltee"),
            (0x8000, "dispreldne"),
            (0x10000, "disprelpnd"),
            (0x20000, "nodirect"),
            (0x40000, "ignmuldef"),
            (0x80000, "noksyms"),
            (0x100000, "nohdr"),
            (0x200000, "edited"),
            (0x400000, "noreloc"),
            (0x800000, "symintpose"),
            (0x1000000, "globalaudit"),
            (0x2000000, "singleton"),
        ][..]
    } else {
        &[
            (0x1, "origin"),
            (0x2, "symbolic"),
            (0x4, "textrel"),
            (0x8, "bind-now"),
            (0x10, "static-tls"),
        ][..]
    };

    for (bit, name) in known {
        if value & bit != 0 {
            flags.push(format!(
                "{}:{name}",
                if flags_1 { "flags_1" } else { "flags" }
            ));
        }
    }
}

fn table_offset(base: u64, entsize: u16, index: u16, span: Span) -> Result<usize, RuntimeError> {
    usize::try_from(
        base.checked_add(u64::from(entsize) * u64::from(index))
            .ok_or_else(|| malformed("ELF table offset overflow", span))?,
    )
    .map_err(|_| malformed("ELF table offset overflow", span))
}

fn slice_u64(data: &[u8], offset: u64, size: u64, span: Span) -> Result<&[u8], RuntimeError> {
    let offset = usize::try_from(offset).map_err(|_| malformed("ELF offset overflow", span))?;
    let size = usize::try_from(size).map_err(|_| malformed("ELF size overflow", span))?;
    require_range(data, offset, size, span)?;
    Ok(&data[offset..offset + size])
}

fn require_range(data: &[u8], offset: usize, size: usize, span: Span) -> Result<(), RuntimeError> {
    if offset
        .checked_add(size)
        .is_some_and(|end| end <= data.len())
    {
        Ok(())
    } else {
        Err(malformed("truncated ELF data", span))
    }
}

fn malformed(message: &str, span: Span) -> RuntimeError {
    RuntimeError::new("elf-malformed", message).with_span(span)
}

fn u16_at(data: &[u8], offset: usize, endian: Endian, span: Span) -> Result<u16, RuntimeError> {
    require_range(data, offset, 2, span)?;
    let bytes = [data[offset], data[offset + 1]];
    Ok(match endian {
        Endian::Little => u16::from_le_bytes(bytes),
        Endian::Big => u16::from_be_bytes(bytes),
    })
}

fn u32_at(data: &[u8], offset: usize, endian: Endian, span: Span) -> Result<u32, RuntimeError> {
    require_range(data, offset, 4, span)?;
    let bytes = [
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ];
    Ok(match endian {
        Endian::Little => u32::from_le_bytes(bytes),
        Endian::Big => u32::from_be_bytes(bytes),
    })
}

fn i32_at(data: &[u8], offset: usize, endian: Endian, span: Span) -> Result<i32, RuntimeError> {
    Ok(u32_at(data, offset, endian, span)? as i32)
}

fn u64_at(data: &[u8], offset: usize, endian: Endian, span: Span) -> Result<u64, RuntimeError> {
    require_range(data, offset, 8, span)?;
    let bytes = [
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
        data[offset + 4],
        data[offset + 5],
        data[offset + 6],
        data[offset + 7],
    ];
    Ok(match endian {
        Endian::Little => u64::from_le_bytes(bytes),
        Endian::Big => u64::from_be_bytes(bytes),
    })
}

fn i64_at(data: &[u8], offset: usize, endian: Endian, span: Span) -> Result<i64, RuntimeError> {
    Ok(u64_at(data, offset, endian, span)? as i64)
}

fn elf_type_name(value: u16) -> &'static str {
    match value {
        0 => "none",
        1 => "relocatable",
        2 => "executable",
        3 => "shared",
        4 => "core",
        _ => "unknown",
    }
}

fn machine_name(value: u16) -> &'static str {
    match value {
        3 => "i386",
        8 => "mips",
        20 => "powerpc",
        21 => "powerpc64",
        40 => "arm",
        50 => "ia64",
        62 => "x86_64",
        183 => "aarch64",
        243 => "riscv",
        _ => "unknown",
    }
}

fn os_abi_name(value: u8) -> &'static str {
    match value {
        0 => "sysv",
        1 => "hpux",
        2 => "netbsd",
        3 => "linux",
        6 => "solaris",
        7 => "aix",
        8 => "irix",
        9 => "freebsd",
        12 => "openbsd",
        64 => "arm-eabi",
        97 => "arm",
        _ => "unknown",
    }
}

pub(crate) fn dynamic_tag_name(tag: i64) -> &'static str {
    match tag {
        1 => "DT_NEEDED",
        2 => "DT_PLTRELSZ",
        3 => "DT_PLTGOT",
        4 => "DT_HASH",
        5 => "DT_STRTAB",
        6 => "DT_SYMTAB",
        7 => "DT_RELA",
        8 => "DT_RELASZ",
        9 => "DT_RELAENT",
        10 => "DT_STRSZ",
        11 => "DT_SYMENT",
        12 => "DT_INIT",
        13 => "DT_FINI",
        14 => "DT_SONAME",
        15 => "DT_RPATH",
        16 => "DT_SYMBOLIC",
        17 => "DT_REL",
        18 => "DT_RELSZ",
        19 => "DT_RELENT",
        20 => "DT_PLTREL",
        21 => "DT_DEBUG",
        22 => "DT_TEXTREL",
        23 => "DT_JMPREL",
        24 => "DT_BIND_NOW",
        25 => "DT_INIT_ARRAY",
        26 => "DT_FINI_ARRAY",
        27 => "DT_INIT_ARRAYSZ",
        28 => "DT_FINI_ARRAYSZ",
        29 => "DT_RUNPATH",
        30 => "DT_FLAGS",
        32 => "DT_PREINIT_ARRAY",
        33 => "DT_PREINIT_ARRAYSZ",
        0x6000_000f => "DT_ANDROID_REL",
        0x6000_0010 => "DT_ANDROID_RELSZ",
        0x6000_0011 => "DT_ANDROID_RELA",
        0x6000_0012 => "DT_ANDROID_RELASZ",
        0x6fff_fef5 => "DT_GNU_HASH",
        0x6fff_ffef => "DT_GNU_PRELINKED",
        0x6fff_fff0 => "DT_GNU_CONFLICTSZ",
        0x6fff_fff9 => "DT_RELACOUNT",
        0x6fff_fffa => "DT_RELCOUNT",
        0x6fff_fffb => "DT_FLAGS_1",
        0x6fff_fffc => "DT_VERDEF",
        0x6fff_fffd => "DT_VERDEFNUM",
        0x6fff_fffe => "DT_VERNEED",
        0x6fff_ffff => "DT_VERNEEDNUM",
        value if (0x6fff_fff0..=0x6fff_ffff).contains(&value) => "DT_GNU",
        _ => "DT_UNKNOWN",
    }
}

#[cfg(test)]
mod tests {
    use super::{ElfClass, Endian, inspect_bytes};
    use crate::source::{SourceId, Span};

    #[test]
    fn parses_dynamic_dependencies_from_elf64_little_endian() {
        let bytes = fixture_elf64(Endian::Little, false, false);
        let info = inspect_bytes(&bytes, span()).expect("inspect");

        assert_eq!(info.class, "ELF64");
        assert_eq!(info.endian, "little");
        assert_eq!(info.machine, "x86_64");
        assert_eq!(info.elf_type, "shared");
        assert_eq!(info.interpreter, "/lib/ld-musl-x86_64.so.1");
        assert_eq!(info.soname, "libdemo.so");
        assert_eq!(info.needed, ["libc.musl-x86_64.so.1", "libxsh-private.so"]);
        assert_eq!(info.rpath, "$ORIGIN/lib");
        assert_eq!(info.runpath, "$ORIGIN");
    }

    #[test]
    fn parses_big_endian_elf32_and_android_dynamic_tags() {
        let bytes = fixture_elf32_big_endian();
        let info = inspect_bytes(&bytes, span()).expect("inspect");

        assert_eq!(info.class, "ELF32");
        assert_eq!(info.endian, "big");
        assert_eq!(info.machine, "arm");
        assert!(
            info.dynamic_tags
                .iter()
                .any(|entry| entry.tag == "DT_ANDROID_RELA")
        );
    }

    #[test]
    fn uses_section_headers_when_program_dynamic_is_absent() {
        let bytes = fixture_elf64(Endian::Little, true, false);
        let info = inspect_bytes(&bytes, span()).expect("inspect");

        assert_eq!(info.soname, "libdemo.so");
        assert_eq!(info.needed.len(), 2);
    }

    #[test]
    fn reports_non_elf_without_error() {
        let info = inspect_bytes(b"not an elf", span()).expect("inspect");

        assert_eq!(info.elf_type, "not-elf");
        assert!(info.needed.is_empty());
    }

    #[test]
    fn rejects_truncated_elf() {
        let error = inspect_bytes(b"\x7fELF\x02\x01", span()).expect_err("malformed");

        assert_eq!(error.kind, "elf-malformed");
    }

    fn fixture_elf64(endian: Endian, section_only: bool, android_tag: bool) -> Vec<u8> {
        let mut data = vec![0; 0x600];
        ident(&mut data, ElfClass::Elf64, endian);
        write16(&mut data, 16, endian, 3);
        write16(&mut data, 18, endian, 62);
        write32(&mut data, 20, endian, 1);
        write64(&mut data, 32, endian, if section_only { 0 } else { 0x40 });
        write64(&mut data, 40, endian, 0x300);
        write16(&mut data, 52, endian, 64);
        write16(&mut data, 54, endian, 56);
        write16(&mut data, 56, endian, if section_only { 0 } else { 3 });
        write16(&mut data, 58, endian, 64);
        write16(&mut data, 60, endian, 3);

        if !section_only {
            ph64(&mut data, endian, 0x40, 1, 0x100, 0x400000, 0x200);
            ph64(&mut data, endian, 0x78, 2, 0x200, 0x400100, 0x80);
            ph64(&mut data, endian, 0xb0, 3, 0x280, 0x400180, 0x18);
        }
        sh64(
            &mut data,
            endian,
            0x340,
            SectionHeader64 {
                sh_type: 6,
                offset: 0x200,
                size: 0x80,
                link: 2,
                entsize: 16,
            },
        );
        sh64(
            &mut data,
            endian,
            0x380,
            SectionHeader64 {
                sh_type: 3,
                offset: 0x500,
                size: 0x80,
                link: 0,
                entsize: 0,
            },
        );

        let strings =
            b"\0libc.musl-x86_64.so.1\0libxsh-private.so\0libdemo.so\0$ORIGIN/lib\0$ORIGIN\0";
        data[0x500..0x500 + strings.len()].copy_from_slice(strings);
        dyn64(&mut data, endian, 0x200, 0, 1, 1);
        dyn64(&mut data, endian, 0x200, 1, 1, 23);
        dyn64(&mut data, endian, 0x200, 2, 14, 41);
        dyn64(&mut data, endian, 0x200, 3, 15, 52);
        dyn64(&mut data, endian, 0x200, 4, 29, 64);
        dyn64(&mut data, endian, 0x200, 5, 5, 0x400400);
        dyn64(&mut data, endian, 0x200, 6, 10, strings.len() as u64);
        dyn64(&mut data, endian, 0x200, 7, 30, 0x8);
        if android_tag {
            dyn64(&mut data, endian, 0x200, 8, 0x60000011, 0x1234);
            dyn64(&mut data, endian, 0x200, 9, 0, 0);
        } else {
            dyn64(&mut data, endian, 0x200, 8, 0, 0);
        }
        data[0x280..0x299].copy_from_slice(b"/lib/ld-musl-x86_64.so.1\0");
        data
    }

    fn span() -> Span {
        Span::new(SourceId::new(0), 0, 0)
    }

    fn fixture_elf32_big_endian() -> Vec<u8> {
        let mut data = vec![0; 0x400];
        ident(&mut data, ElfClass::Elf32, Endian::Big);
        write16(&mut data, 16, Endian::Big, 3);
        write16(&mut data, 18, Endian::Big, 40);
        write32(&mut data, 20, Endian::Big, 1);
        write32(&mut data, 28, Endian::Big, 0x40);
        write16(&mut data, 40, Endian::Big, 52);
        write16(&mut data, 42, Endian::Big, 32);
        write16(&mut data, 44, Endian::Big, 2);
        ph32(&mut data, Endian::Big, 0x40, 1, 0x100, 0x8000, 0x300);
        ph32(&mut data, Endian::Big, 0x60, 2, 0x180, 0x8080, 0x40);
        let strings = b"\0libarm.so\0";
        data[0x300..0x300 + strings.len()].copy_from_slice(strings);
        dyn32(&mut data, Endian::Big, 0x180, 0, 1, 1);
        dyn32(&mut data, Endian::Big, 0x180, 1, 5, 0x8200);
        dyn32(&mut data, Endian::Big, 0x180, 2, 10, strings.len() as u32);
        dyn32(&mut data, Endian::Big, 0x180, 3, 0x60000011, 0x55);
        dyn32(&mut data, Endian::Big, 0x180, 4, 0, 0);
        data
    }

    fn ident(data: &mut [u8], class: ElfClass, endian: Endian) {
        data[..4].copy_from_slice(b"\x7fELF");
        data[4] = match class {
            ElfClass::Elf32 => 1,
            ElfClass::Elf64 => 2,
        };
        data[5] = match endian {
            Endian::Little => 1,
            Endian::Big => 2,
        };
        data[6] = 1;
        data[7] = 3;
    }

    fn ph64(
        data: &mut [u8],
        endian: Endian,
        base: usize,
        p_type: u32,
        offset: u64,
        vaddr: u64,
        filesz: u64,
    ) {
        write32(data, base, endian, p_type);
        write64(data, base + 8, endian, offset);
        write64(data, base + 16, endian, vaddr);
        write64(data, base + 32, endian, filesz);
    }

    fn ph32(
        data: &mut [u8],
        endian: Endian,
        base: usize,
        p_type: u32,
        offset: u32,
        vaddr: u32,
        filesz: u32,
    ) {
        write32(data, base, endian, p_type);
        write32(data, base + 4, endian, offset);
        write32(data, base + 8, endian, vaddr);
        write32(data, base + 16, endian, filesz);
    }

    struct SectionHeader64 {
        sh_type: u32,
        offset: u64,
        size: u64,
        link: u32,
        entsize: u64,
    }

    fn sh64(data: &mut [u8], endian: Endian, base: usize, section: SectionHeader64) {
        write32(data, base + 4, endian, section.sh_type);
        write64(data, base + 24, endian, section.offset);
        write64(data, base + 32, endian, section.size);
        write32(data, base + 40, endian, section.link);
        write64(data, base + 56, endian, section.entsize);
    }

    fn dyn64(data: &mut [u8], endian: Endian, table: usize, index: usize, tag: i64, value: u64) {
        let base = table + index * 16;
        write64(data, base, endian, tag as u64);
        write64(data, base + 8, endian, value);
    }

    fn dyn32(data: &mut [u8], endian: Endian, table: usize, index: usize, tag: i64, value: u32) {
        let base = table + index * 8;
        write32(data, base, endian, tag as u32);
        write32(data, base + 4, endian, value);
    }

    fn write16(data: &mut [u8], offset: usize, endian: Endian, value: u16) {
        let bytes = match endian {
            Endian::Little => value.to_le_bytes(),
            Endian::Big => value.to_be_bytes(),
        };
        data[offset..offset + 2].copy_from_slice(&bytes);
    }

    fn write32(data: &mut [u8], offset: usize, endian: Endian, value: u32) {
        let bytes = match endian {
            Endian::Little => value.to_le_bytes(),
            Endian::Big => value.to_be_bytes(),
        };
        data[offset..offset + 4].copy_from_slice(&bytes);
    }

    fn write64(data: &mut [u8], offset: usize, endian: Endian, value: u64) {
        let bytes = match endian {
            Endian::Little => value.to_le_bytes(),
            Endian::Big => value.to_be_bytes(),
        };
        data[offset..offset + 8].copy_from_slice(&bytes);
    }
}
