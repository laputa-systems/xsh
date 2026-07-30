#![allow(clippy::single_call_fn)]

use super::{
    BLKGETSIZE64, BLKRRPART, BLKSSZGET, BTRFS_FSID_OFFSET, BTRFS_MAGIC_OFFSET, BTRFS_SUPER_OFFSET,
    BTRFS_SUPER_SIZE, BlkidInfo, Device, EXT_FEATURE_COMPAT_OFFSET, EXT_FEATURE_INCOMPAT_OFFSET,
    EXT_FEATURE_RO_COMPAT_OFFSET, EXT_LABEL_OFFSET, EXT_MAGIC_OFFSET, EXT_SUPER_OFFSET,
    EXT_SUPER_SIZE, EXT_UUID_OFFSET, EXT3_FEATURE_HAS_JOURNAL, EXT4_FEATURE_INCOMPAT_64BIT,
    EXT4_FEATURE_INCOMPAT_EXTENTS, EXT4_FEATURE_RO_COMPAT_DIR_NLINK,
    EXT4_FEATURE_RO_COMPAT_HUGE_FILE, GPT_ENTRY_SIZE, GPT_HEADER_SIZE, ISO9660_LABEL_OFFSET,
    ISO9660_LABEL_SIZE, ISO9660_PVD_OFFSET, ISO9660_PVD_SIZE, MBR_PARTITION_OFFSET,
    MBR_SIGNATURE_OFFSET, PartitionRecord, VFAT_BOOT_SIZE, VFAT_LABEL16_OFFSET,
    VFAT_LABEL32_OFFSET, VFAT_SERIAL16_OFFSET, VFAT_SERIAL32_OFFSET, XFS_LABEL_OFFSET,
    XFS_SUPER_SIZE, XFS_UUID_OFFSET, str_value,
};
use crate::runtime::value::{LiveStream, PathValue, RecordMap, RuntimeError, StreamValue, Value};
use crate::source::Span;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

pub(super) fn blkid_info(device: &Path) -> io::Result<BlkidInfo> {
    let mut file = File::open(device)?;
    let mut info = read_swap_tags(&mut file)?
        .or_else(|| read_ext_tags(&mut file).ok().flatten())
        .or_else(|| read_xfs_tags(&mut file).ok().flatten())
        .or_else(|| read_vfat_tags(&mut file).ok().flatten())
        .or_else(|| read_btrfs_tags(&mut file).ok().flatten())
        .or_else(|| read_iso9660_tags(&mut file).ok().flatten())
        .or_else(|| read_squashfs_tags(&mut file).ok().flatten())
        .unwrap_or_default();
    if let Ok(table) = partition_table_info(device) {
        info.part_table_type = table.0;
        info.part_entry_uuid = table.1;
    }
    Ok(info)
}

pub(super) fn block_devices_impl(span: Span) -> Result<StreamValue, RuntimeError> {
    let mut entries = Vec::new();
    let directory = fs::read_dir("/sys/block").map_err(|error| {
        RuntimeError::new("linux-block-devices", error.to_string()).with_span(span)
    })?;
    for entry in directory {
        let entry = entry.map_err(|error| {
            RuntimeError::new("linux-block-devices", error.to_string()).with_span(span)
        })?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let sys_path = entry.path();
        let dev_path = PathBuf::from("/dev").join(&name);
        entries.push((name, sys_path, dev_path));
    }
    entries.sort_unstable_by(|left, right| left.2.cmp(&right.2));
    Ok(StreamValue::from_live(
        "linux.block_devices",
        BlockDeviceStream {
            entries: entries.into_iter(),
        },
    ))
}

struct BlockDeviceStream {
    entries: std::vec::IntoIter<(String, PathBuf, PathBuf)>,
}

impl LiveStream for BlockDeviceStream {
    fn next(&mut self, span: Span) -> Result<Option<Value>, RuntimeError> {
        let Some((name, sys_path, dev_path)) = self.entries.next() else {
            return Ok(None);
        };
        block_device_record(&name, &sys_path, &dev_path, span).map(Some)
    }
}

fn block_device_record(
    name: &str,
    sys_path: &Path,
    dev_path: &Path,
    _span: Span,
) -> Result<Value, RuntimeError> {
    let sectors = read_sysfs_i64(&sys_path.join("size")).unwrap_or(0);
    let sector_size = read_sysfs_i64(&sys_path.join("queue/logical_block_size")).unwrap_or(512);
    let removable = read_sysfs_i64(&sys_path.join("removable")).unwrap_or(0) != 0;
    let rotational = read_sysfs_i64(&sys_path.join("queue/rotational")).unwrap_or(0) != 0;
    let mut partitions = Vec::new();
    if let Ok(entries) = fs::read_dir(sys_path) {
        for entry in entries.flatten() {
            let partition_name = entry.file_name().to_string_lossy().into_owned();
            if entry.path().join("partition").exists() {
                let path = PathBuf::from("/dev").join(partition_name);
                partitions.push(Value::Path(PathValue::new(
                    path.as_os_str().as_bytes().to_vec(),
                )?));
            }
        }
    }
    partitions.sort_unstable_by(|left, right| {
        path_sort_key(left)
            .unwrap_or_default()
            .cmp(&path_sort_key(right).unwrap_or_default())
    });
    Ok(Value::Record(crate::runtime::value::RecordMap::from([
        (Arc::from("name"), str_value(name.to_string())),
        (
            Arc::from("path"),
            Value::Path(PathValue::new(dev_path.as_os_str().as_bytes().to_vec())?),
        ),
        (
            Arc::from("size"),
            Value::Int(sectors.saturating_mul(sector_size)),
        ),
        (Arc::from("sectors"), Value::Int(sectors)),
        (Arc::from("sector_size"), Value::Int(sector_size)),
        (Arc::from("removable"), Value::Bool(removable)),
        (Arc::from("rotational"), Value::Bool(rotational)),
        (
            Arc::from("partitioned"),
            Value::Bool(!partitions.is_empty()),
        ),
        (Arc::from("partitions"), Value::List(partitions)),
    ])))
}

fn read_sysfs_i64(path: &Path) -> io::Result<i64> {
    Ok(fs::read_to_string(path)?.trim().parse().unwrap_or(0))
}

fn path_sort_key(value: &Value) -> Option<String> {
    match value {
        Value::Path(path) => Some(String::from_utf8_lossy(&path.bytes).into_owned()),
        _ => None,
    }
}

fn read_swap_tags(file: &mut File) -> io::Result<Option<BlkidInfo>> {
    let page = page_size()?;
    if file.metadata()?.len() < page as u64 {
        return Ok(None);
    }
    let mut magic = [0_u8; 10];
    read_at(file, page as u64 - 10, &mut magic)?;
    if &magic != b"SWAPSPACE2" {
        return Ok(None);
    }
    let mut uuid = [0_u8; 16];
    let mut label = [0_u8; 16];
    read_at(file, 1024 + 12, &mut uuid)?;
    read_at(file, 1024 + 28, &mut label)?;
    Ok(Some(BlkidInfo {
        fstype: "swap".to_string(),
        uuid: format_uuid(&uuid).unwrap_or_default(),
        label: read_c_string(&label).unwrap_or_default(),
        ..BlkidInfo::default()
    }))
}

fn read_ext_tags(file: &mut File) -> io::Result<Option<BlkidInfo>> {
    let mut superblock = [0_u8; EXT_SUPER_SIZE];
    read_at(file, EXT_SUPER_OFFSET, &mut superblock)?;
    if u16::from_le_bytes([
        superblock[EXT_MAGIC_OFFSET],
        superblock[EXT_MAGIC_OFFSET + 1],
    ]) != 0xef53
    {
        return Ok(None);
    }
    let compat = read_u32_le(&superblock, EXT_FEATURE_COMPAT_OFFSET);
    let incompat = read_u32_le(&superblock, EXT_FEATURE_INCOMPAT_OFFSET);
    let ro_compat = read_u32_le(&superblock, EXT_FEATURE_RO_COMPAT_OFFSET);
    let fstype =
        if ro_compat & (EXT4_FEATURE_RO_COMPAT_HUGE_FILE | EXT4_FEATURE_RO_COMPAT_DIR_NLINK) != 0
            || incompat & (EXT4_FEATURE_INCOMPAT_EXTENTS | EXT4_FEATURE_INCOMPAT_64BIT) != 0
        {
            "ext4"
        } else if compat & EXT3_FEATURE_HAS_JOURNAL != 0 {
            "ext3"
        } else {
            "ext2"
        };
    Ok(Some(BlkidInfo {
        fstype: fstype.to_string(),
        uuid: format_uuid(&superblock[EXT_UUID_OFFSET..EXT_UUID_OFFSET + 16]).unwrap_or_default(),
        label: read_c_string(&superblock[EXT_LABEL_OFFSET..EXT_LABEL_OFFSET + 16])
            .unwrap_or_default(),
        ..BlkidInfo::default()
    }))
}

fn read_xfs_tags(file: &mut File) -> io::Result<Option<BlkidInfo>> {
    let mut superblock = [0_u8; XFS_SUPER_SIZE];
    read_at(file, 0, &mut superblock)?;
    if &superblock[..4] != b"XFSB" {
        return Ok(None);
    }
    Ok(Some(BlkidInfo {
        fstype: "xfs".to_string(),
        uuid: format_uuid(&superblock[XFS_UUID_OFFSET..XFS_UUID_OFFSET + 16]).unwrap_or_default(),
        label: read_c_string(&superblock[XFS_LABEL_OFFSET..XFS_LABEL_OFFSET + 12])
            .unwrap_or_default(),
        ..BlkidInfo::default()
    }))
}

fn read_vfat_tags(file: &mut File) -> io::Result<Option<BlkidInfo>> {
    let mut boot = [0_u8; VFAT_BOOT_SIZE];
    read_at(file, 0, &mut boot)?;
    if &boot[3..7] == b"NTFS" || boot[510] != 0x55 || boot[511] != 0xaa {
        return Ok(None);
    }
    let (serial_offset, label_offset) = if &boot[82..90] == b"FAT32   " || &boot[54..59] == b"MSDOS"
    {
        (VFAT_SERIAL32_OFFSET, VFAT_LABEL32_OFFSET)
    } else if &boot[54..62] == b"FAT16   " || &boot[54..62] == b"FAT12   " {
        (VFAT_SERIAL16_OFFSET, VFAT_LABEL16_OFFSET)
    } else {
        return Ok(None);
    };
    let serial = read_u32_le(&boot, serial_offset);
    Ok(Some(BlkidInfo {
        fstype: "vfat".to_string(),
        uuid: format!("{:04X}-{:04X}", serial >> 16, serial & 0xffff),
        label: read_label(&boot[label_offset..label_offset + 11]).unwrap_or_default(),
        ..BlkidInfo::default()
    }))
}

fn read_btrfs_tags(file: &mut File) -> io::Result<Option<BlkidInfo>> {
    let mut superblock = [0_u8; BTRFS_SUPER_SIZE];
    read_at(file, BTRFS_SUPER_OFFSET, &mut superblock)?;
    if &superblock[BTRFS_MAGIC_OFFSET..BTRFS_MAGIC_OFFSET + 8] != b"_BHRfS_M" {
        return Ok(None);
    }
    Ok(Some(BlkidInfo {
        fstype: "btrfs".to_string(),
        uuid: format_uuid(&superblock[BTRFS_FSID_OFFSET..BTRFS_FSID_OFFSET + 16])
            .unwrap_or_default(),
        ..BlkidInfo::default()
    }))
}

fn read_iso9660_tags(file: &mut File) -> io::Result<Option<BlkidInfo>> {
    let mut descriptor = [0_u8; ISO9660_PVD_SIZE];
    read_at(file, ISO9660_PVD_OFFSET, &mut descriptor)?;
    if descriptor[0] != 1 || &descriptor[1..6] != b"CD001" || descriptor[6] != 1 {
        return Ok(None);
    }
    Ok(Some(BlkidInfo {
        fstype: "iso9660".to_string(),
        label: read_label(
            &descriptor[ISO9660_LABEL_OFFSET..ISO9660_LABEL_OFFSET + ISO9660_LABEL_SIZE],
        )
        .unwrap_or_default(),
        ..BlkidInfo::default()
    }))
}

fn read_squashfs_tags(file: &mut File) -> io::Result<Option<BlkidInfo>> {
    let mut magic = [0_u8; 4];
    read_at(file, 0, &mut magic)?;
    Ok((&magic == b"hsqs").then(|| BlkidInfo {
        fstype: "squashfs".to_string(),
        ..BlkidInfo::default()
    }))
}

pub(super) fn partition_table_impl(device: &Path, span: Span) -> Result<Value, RuntimeError> {
    let mut device = open_device(device, false, span)?;
    if device.total_bytes < device.sector_size {
        return Err(
            RuntimeError::new("linux-partition-table", "device is too small").with_span(span),
        );
    }
    let sector0 = read_lba(&mut device, 0, 1, span)?;
    if device.total_bytes / device.sector_size > 1 {
        let sector1 = read_lba(&mut device, 1, 1, span)?;
        if sector1.starts_with(b"EFI PART") {
            return read_gpt_table(&mut device, &sector1, span);
        }
    }
    if sector0.len() >= MBR_SIGNATURE_OFFSET + 2
        && sector0[MBR_SIGNATURE_OFFSET] == 0x55
        && sector0[MBR_SIGNATURE_OFFSET + 1] == 0xaa
    {
        return Ok(partition_table_record(
            "dos",
            "",
            device.sector_size as i64,
            parse_mbr_partitions(&sector0),
        ));
    }
    Ok(partition_table_record(
        "none",
        "",
        device.sector_size as i64,
        Vec::new(),
    ))
}

pub(super) fn write_partition_table_impl(
    device: &Path,
    table: &RecordMap,
    span: Span,
) -> Result<(), RuntimeError> {
    let mut device = open_device(device, true, span)?;
    let label = record_str_field(table, "label")?;
    let partitions = record_list_field(table, "partitions")?;
    match label.as_str() {
        "dos" => write_mbr_table(&mut device, partitions, span)?,
        "gpt" => write_gpt_table(&mut device, table, partitions, span)?,
        _ => {
            return Err(RuntimeError::new(
                "linux-write-partition-table",
                "partition-table label must be `dos` or `gpt`",
            )
            .with_span(span));
        }
    }
    device.file.sync_all().map_err(|error| {
        RuntimeError::new("linux-write-partition-table", error.to_string()).with_span(span)
    })?;
    if device.is_block {
        unsafe {
            libc::ioctl(device.file.as_raw_fd(), BLKRRPART as _, 0);
        }
    }
    Ok(())
}

pub(super) fn fsck_impl(
    device: &Path,
    fstype: &str,
    repair: bool,
    span: Span,
) -> Result<Value, RuntimeError> {
    fsck_impl_with_path(device, fstype, repair, None, span)
}

pub(super) fn fsck_impl_with_path(
    device: &Path,
    fstype: &str,
    repair: bool,
    path: Option<&OsStr>,
    span: Span,
) -> Result<Value, RuntimeError> {
    let fstype = if fstype.is_empty() {
        blkid_info(device)
            .map(|info| info.fstype)
            .unwrap_or_else(|_| "auto".to_string())
    } else {
        fstype.to_string()
    };
    let program = if fstype == "auto" {
        "fsck".to_string()
    } else {
        format!("fsck.{fstype}")
    };
    let mut command = Command::new(&program);
    if let Some(path) = path {
        command.env("PATH", path);
    }
    if !repair {
        command.arg("-n");
    }
    command.arg(device);
    let output = command.output().map_err(|error| {
        RuntimeError::new("linux-fsck", format!("executing {program}: {error}")).with_span(span)
    })?;
    let mut errors = Vec::new();
    errors.extend(
        String::from_utf8_lossy(&output.stderr)
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| str_value(line.to_string())),
    );
    Ok(Value::Record(crate::runtime::value::RecordMap::from([
        (
            Arc::from("status"),
            Value::Int(output.status.code().unwrap_or(1) as i64),
        ),
        (Arc::from("errors"), Value::List(errors)),
    ])))
}

fn open_device(path: &Path, write: bool, span: Span) -> Result<Device, RuntimeError> {
    let file = OpenOptions::new()
        .read(true)
        .write(write)
        .open(path)
        .map_err(|error| {
            RuntimeError::new("linux-partition-table", error.to_string()).with_span(span)
        })?;
    let metadata = file.metadata().map_err(|error| {
        RuntimeError::new("linux-partition-table", error.to_string()).with_span(span)
    })?;
    let is_block = metadata.file_type().is_block_device();
    let sector_size = if is_block {
        let mut size = 0_i32;
        let rc = unsafe { libc::ioctl(file.as_raw_fd(), BLKSSZGET as _, &mut size) };
        if rc == 0 && size > 0 {
            size as u64
        } else {
            512
        }
    } else {
        512
    };
    let total_bytes = if is_block {
        let mut size = 0_u64;
        let rc = unsafe { libc::ioctl(file.as_raw_fd(), BLKGETSIZE64 as _, &mut size) };
        if rc == 0 { size } else { metadata.len() }
    } else {
        metadata.len()
    };
    Ok(Device {
        file,
        sector_size,
        total_bytes,
        is_block,
    })
}

fn read_lba(
    device: &mut Device,
    lba: u64,
    sectors: u64,
    span: Span,
) -> Result<Vec<u8>, RuntimeError> {
    let offset = lba.checked_mul(device.sector_size).ok_or_else(|| {
        RuntimeError::new("linux-partition-table", "disk offset overflow").with_span(span)
    })?;
    let byte_count = sectors.checked_mul(device.sector_size).ok_or_else(|| {
        RuntimeError::new("linux-partition-table", "disk size overflow").with_span(span)
    })?;
    let mut buffer = vec![0_u8; byte_count as usize];
    device
        .file
        .seek(SeekFrom::Start(offset))
        .and_then(|_| device.file.read_exact(&mut buffer))
        .map_err(|error| {
            RuntimeError::new("linux-partition-table", error.to_string()).with_span(span)
        })?;
    Ok(buffer)
}

fn write_lba(device: &mut Device, lba: u64, data: &[u8], span: Span) -> Result<(), RuntimeError> {
    let offset = lba.checked_mul(device.sector_size).ok_or_else(|| {
        RuntimeError::new("linux-write-partition-table", "disk offset overflow").with_span(span)
    })?;
    device
        .file
        .seek(SeekFrom::Start(offset))
        .and_then(|_| device.file.write_all(data))
        .map_err(|error| {
            RuntimeError::new("linux-write-partition-table", error.to_string()).with_span(span)
        })
}

fn parse_mbr_partitions(sector0: &[u8]) -> Vec<PartitionRecord> {
    (0..4)
        .filter_map(|slot| {
            let offset = MBR_PARTITION_OFFSET + slot * 16;
            if offset + 16 > sector0.len() {
                return None;
            }
            let type_code = sector0[offset + 4];
            let start = read_u32_le(sector0, offset + 8);
            let sectors = read_u32_le(sector0, offset + 12);
            if type_code == 0 && sectors == 0 {
                return None;
            }
            Some(PartitionRecord {
                index: slot as i64 + 1,
                start: start as i64,
                end: start.saturating_add(sectors).saturating_sub(1) as i64,
                size: sectors as i64,
                kind: format!("{type_code:02x}"),
                uuid: String::new(),
                name: String::new(),
            })
        })
        .collect()
}

fn read_gpt_table(device: &mut Device, header: &[u8], span: Span) -> Result<Value, RuntimeError> {
    if header.len() < GPT_HEADER_SIZE {
        return Err(
            RuntimeError::new("linux-partition-table", "truncated GPT header").with_span(span),
        );
    }
    let disk_guid = format_guid(&guid_from_gpt(&header[56..72]));
    let entries_lba = read_u64_le(header, 72);
    let entry_count = read_u32_le(header, 80).max(1);
    let entry_size = read_u32_le(header, 84).max(GPT_ENTRY_SIZE as u32);
    let entries_bytes = entry_count as usize * entry_size as usize;
    let sectors = div_ceil(entries_bytes as u64, device.sector_size);
    let mut entries = read_lba(device, entries_lba, sectors, span)?;
    entries.truncate(entries_bytes);
    let mut partitions = Vec::new();
    for index in 0..entry_count {
        let offset = index as usize * entry_size as usize;
        if offset + GPT_ENTRY_SIZE > entries.len() {
            break;
        }
        let entry = &entries[offset..offset + entry_size as usize];
        if entry[0..16].iter().all(|byte| *byte == 0) {
            continue;
        }
        let first = read_u64_le(entry, 32);
        let last = read_u64_le(entry, 40);
        partitions.push(PartitionRecord {
            index: index as i64 + 1,
            start: first as i64,
            end: last as i64,
            size: last.saturating_sub(first).saturating_add(1) as i64,
            kind: format_guid(&guid_from_gpt(&entry[0..16])),
            uuid: format_guid(&guid_from_gpt(&entry[16..32])),
            name: decode_gpt_name(&entry[56..128]),
        });
    }
    Ok(partition_table_record(
        "gpt",
        &disk_guid,
        device.sector_size as i64,
        partitions,
    ))
}

fn partition_table_info(device: &Path) -> io::Result<(String, String)> {
    let mut file = File::open(device)?;
    let mut sector0 = [0_u8; 512];
    read_at(&mut file, 0, &mut sector0)?;
    let mut sector1 = [0_u8; 512];
    if read_at(&mut file, 512, &mut sector1).is_ok() && sector1.starts_with(b"EFI PART") {
        let guid = format_guid(&guid_from_gpt(&sector1[56..72]));
        return Ok(("gpt".to_string(), guid));
    }
    if sector0[MBR_SIGNATURE_OFFSET] == 0x55 && sector0[MBR_SIGNATURE_OFFSET + 1] == 0xaa {
        Ok(("dos".to_string(), String::new()))
    } else {
        Ok((String::new(), String::new()))
    }
}

fn write_mbr_table(
    device: &mut Device,
    partitions: &[Value],
    span: Span,
) -> Result<(), RuntimeError> {
    if device.sector_size < 512 {
        return Err(
            RuntimeError::new("linux-write-partition-table", "sector size is too small")
                .with_span(span),
        );
    }
    let mut sector0 = vec![0_u8; device.sector_size as usize];
    for value in partitions.iter().take(4) {
        let Value::Record(record) = value else {
            continue;
        };
        let index = record_int_field(record, "index")?;
        if !(1..=4).contains(&index) {
            continue;
        }
        let offset = MBR_PARTITION_OFFSET + (index as usize - 1) * 16;
        let start = record_int_field(record, "start")?;
        let size = record_int_field(record, "size")?;
        if !(0..=u32::MAX as i64).contains(&start) || !(0..=u32::MAX as i64).contains(&size) {
            return Err(RuntimeError::new(
                "linux-write-partition-table",
                "DOS partition values are out of range",
            )
            .with_span(span));
        }
        let kind = record_str_field(record, "type")
            .or_else(|_| record_str_field(record, "kind"))
            .unwrap_or_else(|_| "83".to_string());
        let type_code = u8::from_str_radix(kind.trim_start_matches("0x"), 16).unwrap_or(0x83);
        sector0[offset] = 0;
        sector0[offset + 1..offset + 4].copy_from_slice(&[0xff, 0xff, 0xff]);
        sector0[offset + 4] = type_code;
        sector0[offset + 5..offset + 8].copy_from_slice(&[0xff, 0xff, 0xff]);
        sector0[offset + 8..offset + 12].copy_from_slice(&(start as u32).to_le_bytes());
        sector0[offset + 12..offset + 16].copy_from_slice(&(size as u32).to_le_bytes());
    }
    sector0[MBR_SIGNATURE_OFFSET] = 0x55;
    sector0[MBR_SIGNATURE_OFFSET + 1] = 0xaa;
    write_lba(device, 0, &sector0, span)
}

fn write_gpt_table(
    device: &mut Device,
    table: &RecordMap,
    partitions: &[Value],
    span: Span,
) -> Result<(), RuntimeError> {
    if device.sector_size < 512 {
        return Err(
            RuntimeError::new("linux-write-partition-table", "sector size is too small")
                .with_span(span),
        );
    }
    let total_sectors = device.total_bytes / device.sector_size;
    if total_sectors < 34 {
        return Err(RuntimeError::new(
            "linux-write-partition-table",
            "device is too small for GPT",
        )
        .with_span(span));
    }
    let entry_count = 128_u32;
    let entry_size = GPT_ENTRY_SIZE as u32;
    let entry_sectors = div_ceil(entry_count as u64 * entry_size as u64, device.sector_size);
    let first_usable = 2 + entry_sectors;
    let last_usable = total_sectors.saturating_sub(entry_sectors + 2);
    if last_usable <= first_usable {
        return Err(RuntimeError::new(
            "linux-write-partition-table",
            "device is too small for GPT",
        )
        .with_span(span));
    }
    let disk_guid = record_str_field(table, "id")
        .ok()
        .and_then(|value| parse_guid(&value))
        .unwrap_or([0; 16]);
    let mut entries = vec![0_u8; entry_count as usize * entry_size as usize];
    for value in partitions.iter().take(entry_count as usize) {
        let Value::Record(record) = value else {
            continue;
        };
        let index = record_int_field(record, "index")?;
        if !(1..=entry_count as i64).contains(&index) {
            continue;
        }
        let start = record_int_field(record, "start")? as u64;
        let end = record_int_field(record, "end")? as u64;
        if start < first_usable || end > last_usable || end < start {
            return Err(RuntimeError::new(
                "linux-write-partition-table",
                "GPT partition is outside the usable LBA range",
            )
            .with_span(span));
        }
        let offset = (index as usize - 1) * entry_size as usize;
        let type_guid = record_str_field(record, "type")
            .ok()
            .and_then(|value| parse_guid(&value))
            .unwrap_or(GUID_LINUX_FILESYSTEM);
        let part_guid = record_str_field(record, "uuid")
            .ok()
            .and_then(|value| parse_guid(&value))
            .unwrap_or([0; 16]);
        entries[offset..offset + 16].copy_from_slice(&guid_to_gpt(&type_guid));
        entries[offset + 16..offset + 32].copy_from_slice(&guid_to_gpt(&part_guid));
        entries[offset + 32..offset + 40].copy_from_slice(&start.to_le_bytes());
        entries[offset + 40..offset + 48].copy_from_slice(&end.to_le_bytes());
        if let Ok(name) = record_str_field(record, "name") {
            entries[offset + 56..offset + 128].copy_from_slice(&encode_gpt_name(&name));
        }
    }
    let entries_crc = crc32(&entries);
    let primary_entries_lba = 2;
    let backup_entries_lba = total_sectors - entry_sectors - 1;
    let primary_header = gpt_header(
        GptHeaderSpec {
            current_lba: 1,
            backup_lba: total_sectors - 1,
            first_usable,
            last_usable,
            disk_guid,
            entries_lba: primary_entries_lba,
            entry_count,
            entry_size,
            entries_crc,
        },
        device.sector_size,
    );
    let backup_header = gpt_header(
        GptHeaderSpec {
            current_lba: total_sectors - 1,
            backup_lba: 1,
            first_usable,
            last_usable,
            disk_guid,
            entries_lba: backup_entries_lba,
            entry_count,
            entry_size,
            entries_crc,
        },
        device.sector_size,
    );
    write_lba(
        device,
        0,
        &protective_mbr(total_sectors, device.sector_size),
        span,
    )?;
    write_padded_lba(device, primary_entries_lba, entry_sectors, &entries, span)?;
    write_lba(device, 1, &primary_header, span)?;
    write_padded_lba(device, backup_entries_lba, entry_sectors, &entries, span)?;
    write_lba(device, total_sectors - 1, &backup_header, span)
}

const GUID_LINUX_FILESYSTEM: [u8; 16] = [
    0x0f, 0xc6, 0x3d, 0xaf, 0x84, 0x83, 0x47, 0x72, 0x8e, 0x79, 0x3d, 0x69, 0xd8, 0x47, 0x7d, 0xe4,
];

struct GptHeaderSpec {
    current_lba: u64,
    backup_lba: u64,
    first_usable: u64,
    last_usable: u64,
    disk_guid: [u8; 16],
    entries_lba: u64,
    entry_count: u32,
    entry_size: u32,
    entries_crc: u32,
}

fn gpt_header(spec: GptHeaderSpec, sector_size: u64) -> Vec<u8> {
    let mut header = vec![0_u8; sector_size as usize];
    header[0..8].copy_from_slice(b"EFI PART");
    header[8..12].copy_from_slice(&0x0001_0000_u32.to_le_bytes());
    header[12..16].copy_from_slice(&(GPT_HEADER_SIZE as u32).to_le_bytes());
    header[24..32].copy_from_slice(&spec.current_lba.to_le_bytes());
    header[32..40].copy_from_slice(&spec.backup_lba.to_le_bytes());
    header[40..48].copy_from_slice(&spec.first_usable.to_le_bytes());
    header[48..56].copy_from_slice(&spec.last_usable.to_le_bytes());
    header[56..72].copy_from_slice(&guid_to_gpt(&spec.disk_guid));
    header[72..80].copy_from_slice(&spec.entries_lba.to_le_bytes());
    header[80..84].copy_from_slice(&spec.entry_count.to_le_bytes());
    header[84..88].copy_from_slice(&spec.entry_size.to_le_bytes());
    header[88..92].copy_from_slice(&spec.entries_crc.to_le_bytes());
    let crc = crc32(&header[..GPT_HEADER_SIZE]);
    header[16..20].copy_from_slice(&crc.to_le_bytes());
    header
}

fn protective_mbr(total_sectors: u64, sector_size: u64) -> Vec<u8> {
    let mut sector0 = vec![0_u8; sector_size as usize];
    let size = total_sectors.saturating_sub(1).min(u32::MAX as u64) as u32;
    sector0[MBR_PARTITION_OFFSET + 1..MBR_PARTITION_OFFSET + 4]
        .copy_from_slice(&[0x00, 0x02, 0x00]);
    sector0[MBR_PARTITION_OFFSET + 4] = 0xee;
    sector0[MBR_PARTITION_OFFSET + 5..MBR_PARTITION_OFFSET + 8]
        .copy_from_slice(&[0xff, 0xff, 0xff]);
    sector0[MBR_PARTITION_OFFSET + 8..MBR_PARTITION_OFFSET + 12]
        .copy_from_slice(&1_u32.to_le_bytes());
    sector0[MBR_PARTITION_OFFSET + 12..MBR_PARTITION_OFFSET + 16]
        .copy_from_slice(&size.to_le_bytes());
    sector0[MBR_SIGNATURE_OFFSET] = 0x55;
    sector0[MBR_SIGNATURE_OFFSET + 1] = 0xaa;
    sector0
}

fn write_padded_lba(
    device: &mut Device,
    lba: u64,
    sectors: u64,
    data: &[u8],
    span: Span,
) -> Result<(), RuntimeError> {
    let mut padded = vec![0_u8; (sectors * device.sector_size) as usize];
    padded[..data.len()].copy_from_slice(data);
    write_lba(device, lba, &padded, span)
}

fn partition_table_record(
    label: &str,
    id: &str,
    sector_size: i64,
    partitions: Vec<PartitionRecord>,
) -> Value {
    Value::Record(crate::runtime::value::RecordMap::from([
        (Arc::from("label"), str_value(label.to_string())),
        (Arc::from("id"), str_value(id.to_string())),
        (Arc::from("sector_size"), Value::Int(sector_size)),
        (
            Arc::from("partitions"),
            Value::List(
                partitions
                    .into_iter()
                    .map(|part| {
                        Value::Record(crate::runtime::value::RecordMap::from([
                            (Arc::from("index"), Value::Int(part.index)),
                            (Arc::from("start"), Value::Int(part.start)),
                            (Arc::from("end"), Value::Int(part.end)),
                            (Arc::from("size"), Value::Int(part.size)),
                            (Arc::from("type"), str_value(part.kind)),
                            (Arc::from("uuid"), str_value(part.uuid)),
                            (Arc::from("name"), str_value(part.name)),
                        ]))
                    })
                    .collect(),
            ),
        ),
    ]))
}

pub(super) fn blkid_record(info: BlkidInfo) -> Value {
    Value::Record(crate::runtime::value::RecordMap::from([
        (Arc::from("type"), str_value(info.fstype)),
        (Arc::from("uuid"), str_value(info.uuid)),
        (Arc::from("label"), str_value(info.label)),
        (
            Arc::from("part_table_type"),
            str_value(info.part_table_type),
        ),
        (
            Arc::from("part_entry_uuid"),
            str_value(info.part_entry_uuid),
        ),
    ]))
}

fn read_at(file: &mut File, offset: u64, buffer: &mut [u8]) -> io::Result<()> {
    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(buffer)
}

fn read_u32_le(bytes: &[u8], offset: usize) -> u32 {
    bytes
        .get(offset..offset + 4)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u32::from_le_bytes)
        .unwrap_or(0)
}

fn read_u64_le(bytes: &[u8], offset: usize) -> u64 {
    bytes
        .get(offset..offset + 8)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u64::from_le_bytes)
        .unwrap_or(0)
}

fn format_uuid(bytes: &[u8]) -> Option<String> {
    if bytes.len() != 16 || bytes.iter().all(|byte| *byte == 0) {
        return None;
    }
    Some(format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    ))
}

fn guid_from_gpt(bytes: &[u8]) -> [u8; 16] {
    let mut guid = [0_u8; 16];
    if bytes.len() >= 16 {
        guid[0] = bytes[3];
        guid[1] = bytes[2];
        guid[2] = bytes[1];
        guid[3] = bytes[0];
        guid[4] = bytes[5];
        guid[5] = bytes[4];
        guid[6] = bytes[7];
        guid[7] = bytes[6];
        guid[8..16].copy_from_slice(&bytes[8..16]);
    }
    guid
}

fn guid_to_gpt(guid: &[u8; 16]) -> [u8; 16] {
    [
        guid[3], guid[2], guid[1], guid[0], guid[5], guid[4], guid[7], guid[6], guid[8], guid[9],
        guid[10], guid[11], guid[12], guid[13], guid[14], guid[15],
    ]
}

fn parse_guid(value: &str) -> Option<[u8; 16]> {
    let hex = value.replace('-', "");
    if hex.len() != 32 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut bytes = [0_u8; 16];
    for index in 0..16 {
        bytes[index] = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(bytes)
}

fn format_guid(bytes: &[u8; 16]) -> String {
    format_uuid(bytes).unwrap_or_default()
}

fn decode_gpt_name(bytes: &[u8]) -> String {
    let units = bytes
        .as_chunks::<2>().0.iter()
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .take_while(|unit| *unit != 0)
        .collect::<Vec<_>>();
    String::from_utf16_lossy(&units)
}

fn encode_gpt_name(name: &str) -> [u8; 72] {
    let mut bytes = [0_u8; 72];
    for (index, unit) in name.encode_utf16().take(36).enumerate() {
        bytes[index * 2..index * 2 + 2].copy_from_slice(&unit.to_le_bytes());
    }
    bytes
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn read_c_string(bytes: &[u8]) -> Option<String> {
    let len = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    (len > 0).then(|| {
        String::from_utf8_lossy(&bytes[..len])
            .trim_end()
            .to_string()
    })
}

fn read_label(bytes: &[u8]) -> Option<String> {
    let value = String::from_utf8_lossy(bytes)
        .trim_matches(|ch| ch == '\0' || char::is_whitespace(ch))
        .to_string();
    (!value.is_empty() && value != "NO NAME").then_some(value)
}

pub(super) fn page_size() -> io::Result<usize> {
    Ok(rustix::param::page_size())
}

fn div_ceil(left: u64, right: u64) -> u64 {
    left / right + u64::from(!left.is_multiple_of(right))
}

fn record_str_field(fields: &RecordMap, field: &str) -> Result<String, RuntimeError> {
    match fields.get(field) {
        Some(Value::Str(value)) => Ok(value.to_string()),
        _ => Err(RuntimeError::new(
            "record-field",
            format!("missing string field `{field}`"),
        )),
    }
}

fn record_int_field(fields: &RecordMap, field: &str) -> Result<i64, RuntimeError> {
    match fields.get(field) {
        Some(Value::Int(value)) => Ok(*value),
        _ => Err(RuntimeError::new(
            "record-field",
            format!("missing int field `{field}`"),
        )),
    }
}

fn record_list_field<'a>(fields: &'a RecordMap, field: &str) -> Result<&'a [Value], RuntimeError> {
    match fields.get(field) {
        Some(Value::List(values)) => Ok(values),
        _ => Err(RuntimeError::new(
            "record-field",
            format!("missing list field `{field}`"),
        )),
    }
}

pub(super) fn path_value(path: &Path, span: Span) -> Result<PathValue, RuntimeError> {
    PathValue::new(path.as_os_str().as_bytes().to_vec()).map_err(|error| error.with_span(span))
}

pub(super) fn io_error(kind: &str, error: io::Error, span: Span) -> Value {
    Value::err(Value::Error(Box::new(
        RuntimeError::new(kind, error.to_string()).with_span(span),
    )))
}
