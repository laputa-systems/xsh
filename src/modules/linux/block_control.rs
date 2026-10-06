use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;
use std::path::Path;

fn result_value(result: Result<Value, RuntimeError>) -> Result<Value, RuntimeError> {
    Ok(match result {
        Ok(value) => Value::ok(value),
        Err(error) => Value::err(Value::Error(Box::new(error))),
    })
}

pub(crate) fn umount(
    target: &Path,
    lazy: bool,
    force: bool,
    span: Span,
) -> Result<Value, RuntimeError> {
    result_value(imp::umount(target, lazy, force, span))
}

pub(crate) fn blockdev_info(path: &Path, span: Span) -> Result<Value, RuntimeError> {
    result_value(imp::blockdev_info(path, span))
}

pub(crate) fn blockdev_set_read_only(
    path: &Path,
    read_only: bool,
    span: Span,
) -> Result<Value, RuntimeError> {
    result_value(imp::blockdev_set_read_only(path, read_only, span))
}

pub(crate) fn blockdev_flush(path: &Path, span: Span) -> Result<Value, RuntimeError> {
    result_value(imp::blockdev_flush(path, span))
}

pub(crate) fn blockdev_reread_partition_table(
    path: &Path,
    span: Span,
) -> Result<Value, RuntimeError> {
    result_value(imp::blockdev_reread_partition_table(path, span))
}

pub(crate) fn fstrim(
    path: &Path,
    offset: i64,
    length: Option<i64>,
    minlen: i64,
    span: Span,
) -> Result<Value, RuntimeError> {
    result_value(imp::fstrim(path, offset, length, minlen, span))
}

pub(crate) fn fsfreeze(
    path: &Path,
    freeze: bool,
    span: Span,
) -> Result<Value, RuntimeError> {
    result_value(imp::fsfreeze(path, freeze, span))
}

pub(crate) fn block_signatures(path: &Path, span: Span) -> Result<Value, RuntimeError> {
    result_value(imp::block_signatures(path, span))
}

pub(crate) fn wipe_block_signatures(
    path: &Path,
    offsets: &[i64],
    span: Span,
) -> Result<Value, RuntimeError> {
    result_value(imp::wipe_block_signatures(path, offsets, span))
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{Path, RuntimeError, Span, Value};
    use crate::runtime::value::RecordMap;
    use rustix::fs::{Mode, OFlags, open};
    use rustix::mount::{UnmountFlags, unmount};
    use std::fs::File;
    use std::io;
    use std::os::fd::{AsRawFd, OwnedFd};
    use std::os::unix::fs::{FileExt, FileTypeExt};
    use std::sync::Arc;

    const BLKROSET: libc::c_ulong = 0x125d;
    const BLKROGET: libc::c_ulong = 0x125e;
    const BLKRRPART: libc::c_ulong = 0x125f;
    const BLKFLSBUF: libc::c_ulong = 0x1261;
    const BLKSSZGET: libc::c_ulong = 0x1268;
    const BLKGETSIZE64: libc::c_ulong = 0x8008_1272;
    const BLKPBSZGET: libc::c_ulong = 0x127b;
    const FITRIM: libc::c_ulong = 0xc018_5879;
    const FIFREEZE: libc::c_ulong = 0xc004_5877;
    const FITHAW: libc::c_ulong = 0xc004_5878;

    #[repr(C)]
    struct TrimRange {
        start: u64,
        len: u64,
        minlen: u64,
    }

    struct Signature {
        offset: u64,
        filesystem: &'static str,
        kind: &'static str,
        magic: Vec<u8>,
    }

    pub(super) fn block_signatures(path: &Path, span: Span) -> Result<Value, RuntimeError> {
        let file = signature_file(path, false, span)?;
        let signatures = signatures(&file)
            .map_err(|error| RuntimeError::host("linux-block-signatures", &error).with_span(span))?;
        let records = signatures.into_iter().map(|signature| {
            Ok(Value::Record(RecordMap::from([
                (Arc::from("offset"), unsigned_value(signature.offset, "linux-block-signatures", span)?),
                (Arc::from("type"), Value::Str(Arc::from(signature.filesystem))),
                (Arc::from("kind"), Value::Str(Arc::from(signature.kind))),
                (Arc::from("magic"), Value::Bytes(signature.magic)),
            ])))
        }).collect::<Result<Vec<_>, RuntimeError>>()?;
        Ok(Value::List(records))
    }

    pub(super) fn wipe_block_signatures(
        path: &Path,
        offsets: &[i64],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let kind = "linux-wipe-signatures";
        let offsets = offsets.iter().map(|offset| unsigned_argument(*offset, "offset", kind, span))
            .collect::<Result<Vec<_>, _>>()?;
        let file = signature_file(path, true, span)?;
        let signatures = signatures(&file)
            .map_err(|error| RuntimeError::host(kind, &error).with_span(span))?;
        // Resolve every selection before writing. Unknown offsets cannot turn
        // a partly valid request into a partial wipe of another signature.
        let selected = offsets.iter().map(|offset| {
            signatures.iter().find(|signature| signature.offset == *offset).ok_or_else(|| {
                RuntimeError::new(kind, format!("no recognized signature at offset {offset}"))
                    .with_span(span)
            })
        }).collect::<Result<Vec<_>, _>>()?;
        for signature in &selected {
            let current = read_at(&file, signature.offset, signature.magic.len())
                .map_err(|error| RuntimeError::host(kind, &error).with_span(span))?;
            if current.as_deref() != Some(signature.magic.as_slice()) {
                return Err(RuntimeError::new(kind, "signature changed before removal").with_span(span));
            }
        }
        for signature in selected {
            file.write_all_at(&vec![0; signature.magic.len()], signature.offset)
                .map_err(|error| RuntimeError::host(kind, &error).with_span(span))?;
        }
        file.sync_all().map_err(|error| RuntimeError::host(kind, &error).with_span(span))?;
        Ok(Value::Unit)
    }

    fn signature_file(path: &Path, write: bool, span: Span) -> Result<File, RuntimeError> {
        let kind = if write { "linux-wipe-signatures" } else { "linux-block-signatures" };
        let access = if write { OFlags::RDWR } else { OFlags::RDONLY };
        let fd = open(path, access | OFlags::CLOEXEC | OFlags::NONBLOCK, Mode::empty())
            .map_err(|error| RuntimeError::host(kind, &error).with_span(span))?;
        let file = File::from(fd);
        let metadata = file.metadata().map_err(|error| RuntimeError::host(kind, &error).with_span(span))?;
        if !metadata.is_file() && !metadata.file_type().is_block_device() {
            return Err(RuntimeError::new(kind, "path must be a regular file or block device").with_span(span));
        }
        Ok(file)
    }

    fn read_at(file: &File, offset: u64, length: usize) -> io::Result<Option<Vec<u8>>> {
        let mut bytes = vec![0; length];
        match file.read_exact_at(&mut bytes, offset) {
            Ok(()) => Ok(Some(bytes)),
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn signatures(file: &File) -> io::Result<Vec<Signature>> {
        let mut found = Vec::new();
        let mut add = |offset, filesystem, kind, magic: &[u8]| -> io::Result<()> {
            if read_at(file, offset, magic.len())?.as_deref() == Some(magic) {
                found.push(Signature { offset, filesystem, kind, magic: magic.to_vec() });
            }
            Ok(())
        };
        let ext_type = match read_at(file, 1024 + 0x5c, 12)? {
            Some(features) if le32(&features[4..8]) & 0xc0 != 0 || le32(&features[8..12]) & 0x28 != 0 => "ext4",
            Some(features) if le32(&features[..4]) & 4 != 0 => "ext3",
            _ => "ext2",
        };
        add(1024 + 0x38, ext_type, "filesystem", b"\x53\xef")?;
        add(0, "xfs", "filesystem", b"XFSB")?;
        add(0, "squashfs", "filesystem", b"hsqs")?;
        add(16 * 2048 + 1, "iso9660", "filesystem", b"CD001")?;
        for superblock in [64 * 1024, 64 * 1024 * 1024, 256 * 1024 * 1024 * 1024u64] {
            add(superblock + 0x40, "btrfs", "filesystem", b"_BHRfS_M")?;
        }
        for page in [4096, 8192, 16384, 32768, 65536] {
            add(page - 10, "swap", "filesystem", b"SWAPSPACE2")?;
            add(page - 10, "swap", "filesystem", b"SWAP-SPACE")?;
        }
        if let Some(boot) = read_at(file, 0, 512)? {
            let sector = u16::from_le_bytes([boot[11], boot[12]]);
            let fat = boot[510..512] == [0x55, 0xaa]
                && sector.is_power_of_two() && (512..=4096).contains(&sector)
                && boot[13].is_power_of_two() && boot[14..16] != [0, 0]
                && boot[16] != 0;
            if fat {
                for (offset, magic) in [(54, &b"FAT12   "[..]), (54, &b"FAT16   "[..]), (82, &b"FAT32   "[..])] {
                    add(offset, "vfat", "filesystem", magic)?;
                }
            }
            let partition = boot[446..510].chunks_exact(16).any(|entry| {
                (entry[0] == 0 || entry[0] == 0x80) && entry[4] != 0
                    && le32(&entry[8..12]) != 0 && le32(&entry[12..16]) != 0
            });
            if partition && boot[510..512] == [0x55, 0xaa] {
                add(510, "dos", "partition-table", b"\x55\xaa")?;
            }
        }
        let metadata = file.metadata()?;
        let (size, sector) = if metadata.file_type().is_block_device() {
            let mut size = 0u64;
            let mut sector = 0i32;
            // SAFETY: both pointers have the exact storage required by these
            // read-only block-device requests and the file remains open.
            if unsafe { libc::ioctl(file.as_raw_fd(), BLKGETSIZE64 as _, &mut size) } < 0 {
                return Err(io::Error::last_os_error());
            }
            if unsafe { libc::ioctl(file.as_raw_fd(), BLKSSZGET as _, &mut sector) } < 0 {
                return Err(io::Error::last_os_error());
            }
            if !(512..=4096).contains(&sector) || !(sector as u32).is_power_of_two() {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid block-device sector size"));
            }
            (size, sector as u64)
        } else {
            (metadata.len(), 512)
        };
        if size >= sector * 2 {
            let last = size / sector - 1;
            for lba in [1, last] {
                if let Some(header) = valid_gpt_header(file, lba, sector, size)? {
                    found.push(Signature { offset: lba * sector, filesystem: "gpt", kind: "partition-table", magic: header[..8].to_vec() });
                }
            }
        }
        found.sort_unstable_by_key(|signature| signature.offset);
        found.dedup_by_key(|signature| signature.offset);
        Ok(found)
    }

    // GPT stores independently checksummed primary and backup headers. Each
    // recognized magic must belong to a valid header at its claimed LBA.
    fn valid_gpt_header(file: &File, lba: u64, sector: u64, size: u64) -> io::Result<Option<Vec<u8>>> {
        let Some(mut header) = read_at(file, lba * sector, sector as usize)? else { return Ok(None) };
        if &header[..8] != b"EFI PART" || le32(&header[8..12]) != 0x0001_0000 {
            return Ok(None);
        }
        let header_size = le32(&header[12..16]) as usize;
        let alternate = le64(&header[32..40]);
        let entries_lba = le64(&header[72..80]);
        let entries_count = u64::from(le32(&header[80..84]));
        let entry_size = u64::from(le32(&header[84..88]));
        let entries_bytes = entries_count.checked_mul(entry_size);
        let entries_offset = entries_lba.checked_mul(sector);
        if !(92..=sector as usize).contains(&header_size)
            || le32(&header[20..24]) != 0 || le64(&header[24..32]) != lba
            || alternate == 0 || alternate == lba || alternate >= size / sector
            || le64(&header[40..48]) <= 1
            || le64(&header[40..48]) > le64(&header[48..56])
            || le64(&header[48..56]) >= size / sector
            || entries_count == 0 || entry_size < 128 || entry_size % 128 != 0
            || !(entry_size / 128).is_power_of_two()
            || entries_offset.zip(entries_bytes).and_then(|(offset, len)| offset.checked_add(len)).is_none_or(|end| end > size)
        {
            return Ok(None);
        }
        let checksum = le32(&header[16..20]);
        header[16..20].fill(0);
        if crc32(&header[..header_size]) != checksum {
            return Ok(None);
        }
        let mut remaining = entries_bytes.unwrap();
        let mut offset = entries_offset.unwrap();
        let mut crc = 0xffff_ffffu32;
        while remaining != 0 {
            let length = remaining.min(4096) as usize;
            let Some(bytes) = read_at(file, offset, length)? else { return Ok(None) };
            crc = crc32_update(crc, &bytes);
            remaining -= length as u64;
            offset += length as u64;
        }
        if !crc != le32(&header[88..92]) {
            return Ok(None);
        }
        Ok(Some(header))
    }

    fn le32(bytes: &[u8]) -> u32 {
        u32::from_le_bytes(bytes.try_into().expect("four-byte ABI field"))
    }

    fn le64(bytes: &[u8]) -> u64 {
        u64::from_le_bytes(bytes.try_into().expect("eight-byte ABI field"))
    }

    fn crc32_update(mut crc: u32, bytes: &[u8]) -> u32 {
        for byte in bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
            }
        }
        crc
    }

    fn crc32(bytes: &[u8]) -> u32 {
        !crc32_update(0xffff_ffff, bytes)
    }

    pub(super) fn umount(
        target: &Path,
        lazy: bool,
        force: bool,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let mut flags = UnmountFlags::empty();
        if lazy {
            flags |= UnmountFlags::DETACH;
        }
        if force {
            flags |= UnmountFlags::FORCE;
        }
        unmount(target, flags)
            .map_err(|error| RuntimeError::host("linux-umount", &error).with_span(span))?;
        Ok(Value::Unit)
    }

    pub(super) fn blockdev_info(path: &Path, span: Span) -> Result<Value, RuntimeError> {
        let kind = "linux-blockdev";
        let fd = open_path(path, kind, span)?;
        let mut size = 0u64;
        let mut logical = 0i32;
        let mut physical = 0u32;
        let mut read_only = 0i32;
        // Each request's storage matches its kernel ABI: u64, int, unsigned
        // int, and int respectively. All queries use the same open device.
        ioctl_pointer(&fd, BLKGETSIZE64, &mut size, kind, span)?;
        ioctl_pointer(&fd, BLKSSZGET, &mut logical, kind, span)?;
        ioctl_pointer(&fd, BLKPBSZGET, &mut physical, kind, span)?;
        ioctl_pointer(&fd, BLKROGET, &mut read_only, kind, span)?;
        if logical <= 0 || physical == 0 {
            return Err(RuntimeError::new(kind, "kernel returned an invalid sector size")
                .with_span(span));
        }
        let size = unsigned_value(size, kind, span)?;
        Ok(Value::Record(RecordMap::from([
            (Arc::from("size_bytes"), size),
            (Arc::from("logical_sector_bytes"), Value::Int(i64::from(logical))),
            (Arc::from("physical_sector_bytes"), Value::Int(i64::from(physical))),
            (Arc::from("read_only"), Value::Bool(read_only != 0)),
        ])))
    }

    pub(super) fn blockdev_set_read_only(
        path: &Path,
        read_only: bool,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let kind = "linux-blockdev";
        let fd = open_path(path, kind, span)?;
        let mut value = i32::from(read_only);
        ioctl_pointer(&fd, BLKROSET, &mut value, kind, span)?;
        Ok(Value::Unit)
    }

    pub(super) fn blockdev_flush(path: &Path, span: Span) -> Result<Value, RuntimeError> {
        ioctl_unit(path, BLKFLSBUF, "linux-blockdev", span)
    }

    pub(super) fn blockdev_reread_partition_table(
        path: &Path,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        ioctl_unit(path, BLKRRPART, "linux-blockdev", span)
    }

    pub(super) fn fstrim(
        path: &Path,
        offset: i64,
        length: Option<i64>,
        minlen: i64,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let kind = "linux-fstrim";
        let mut range = TrimRange {
            start: unsigned_argument(offset, "offset", kind, span)?,
            len: match length {
                Some(length) => unsigned_argument(length, "length", kind, span)?,
                None => u64::MAX,
            },
            minlen: unsigned_argument(minlen, "minlen", kind, span)?,
        };
        if range.len == 0 {
            return Err(RuntimeError::new(kind, "length must be positive").with_span(span));
        }
        let fd = open_path(path, kind, span)?;
        ioctl_pointer(&fd, FITRIM, &mut range, kind, span)?;
        unsigned_value(range.len, kind, span)
    }

    pub(super) fn fsfreeze(
        path: &Path,
        freeze: bool,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        ioctl_unit(path, if freeze { FIFREEZE } else { FITHAW }, "linux-fsfreeze", span)
    }

    fn open_path(path: &Path, kind: &str, span: Span) -> Result<OwnedFd, RuntimeError> {
        open(path, OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NONBLOCK, Mode::empty())
            .map_err(|error| RuntimeError::host(kind, &error).with_span(span))
    }

    // Only the ABI-specific callers above choose a request and storage type.
    // This helper does not accept requests supplied by XSH scripts.
    fn ioctl_pointer<T>(
        fd: &OwnedFd,
        request: libc::c_ulong,
        value: &mut T,
        kind: &str,
        span: Span,
    ) -> Result<(), RuntimeError> {
        // SAFETY: the descriptor remains open and each caller pairs its
        // constant request with the exact kernel argument layout.
        let result = unsafe { libc::ioctl(fd.as_raw_fd(), request as _, value as *mut T) };
        ioctl_result(result, kind, span)
    }

    fn ioctl_unit(
        path: &Path,
        request: libc::c_ulong,
        kind: &str,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let fd = open_path(path, kind, span)?;
        // SAFETY: these requests take no pointed-to argument; the descriptor
        // remains open for the entire call.
        let result = unsafe { libc::ioctl(fd.as_raw_fd(), request as _, 0) };
        ioctl_result(result, kind, span)?;
        Ok(Value::Unit)
    }

    fn ioctl_result(result: libc::c_int, kind: &str, span: Span) -> Result<(), RuntimeError> {
        if result < 0 {
            Err(RuntimeError::host(kind, &io::Error::last_os_error()).with_span(span))
        } else {
            Ok(())
        }
    }

    fn unsigned_argument(value: i64, name: &str, kind: &str, span: Span) -> Result<u64, RuntimeError> {
        u64::try_from(value).map_err(|_| {
            RuntimeError::new(kind, format!("{name} must be nonnegative")).with_span(span)
        })
    }

    fn unsigned_value(value: u64, kind: &str, span: Span) -> Result<Value, RuntimeError> {
        i64::try_from(value).map(Value::Int).map_err(|_| {
            RuntimeError::new(kind, "kernel value exceeds the UInt range").with_span(span)
        })
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use super::{Path, RuntimeError, Span, Value};

    fn unsupported(span: Span) -> Result<Value, RuntimeError> {
        Err(RuntimeError::new(
            "linux-unsupported",
            "real linux.* primitives are only available on Linux",
        ).with_span(span))
    }

    pub(super) fn umount(_target: &Path, _lazy: bool, _force: bool, span: Span) -> Result<Value, RuntimeError> {
        unsupported(span)
    }

    pub(super) fn blockdev_info(_path: &Path, span: Span) -> Result<Value, RuntimeError> {
        unsupported(span)
    }

    pub(super) fn blockdev_set_read_only(_path: &Path, _read_only: bool, span: Span) -> Result<Value, RuntimeError> {
        unsupported(span)
    }

    pub(super) fn blockdev_flush(_path: &Path, span: Span) -> Result<Value, RuntimeError> {
        unsupported(span)
    }

    pub(super) fn blockdev_reread_partition_table(_path: &Path, span: Span) -> Result<Value, RuntimeError> {
        unsupported(span)
    }

    pub(super) fn fstrim(_path: &Path, _offset: i64, _length: Option<i64>, _minlen: i64, span: Span) -> Result<Value, RuntimeError> {
        unsupported(span)
    }

    pub(super) fn fsfreeze(_path: &Path, _freeze: bool, span: Span) -> Result<Value, RuntimeError> {
        unsupported(span)
    }

    pub(super) fn block_signatures(_path: &Path, span: Span) -> Result<Value, RuntimeError> {
        unsupported(span)
    }

    pub(super) fn wipe_block_signatures(_path: &Path, _offsets: &[i64], span: Span) -> Result<Value, RuntimeError> {
        unsupported(span)
    }
}
