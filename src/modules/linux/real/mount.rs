use super::common::error_value;
use super::common::{cstring_text, io_error, ok_unit};
use super::{FstabEntry, MountEntry};
use crate::modules::linux::str_value;
use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;
use rustix::io::Errno;
use rustix::mount::{
    MountFlags, MountPropagationFlags, UnmountFlags, mount_bind, mount_bind_recursive,
    mount_change, mount_move, mount_remount, unmount,
};
use std::ffi::CString;
use std::fs;
use std::io;
use std::path::Path;

pub(crate) fn mount(
    source: &str,
    target: &Path,
    fstype: &str,
    options: &[String],
    span: Span,
) -> Result<Value, RuntimeError> {
    mount_one(source, target, fstype, options, span).map(|()| ok_unit())
}

pub(crate) fn mount_all(span: Span) -> Result<Value, RuntimeError> {
    let entries = match read_fstab("/etc/fstab") {
        Ok(entries) => entries,
        Err(error) => return Ok(io_error("linux-mount-all", error, span)),
    };
    for entry in entries {
        if entry.vfstype == "swap" || option_present(&entry.mntops, "noauto") {
            continue;
        }
        mount_one(
            &entry.spec,
            Path::new(&entry.file),
            &entry.vfstype,
            &entry.mntops,
            span,
        )?;
    }
    Ok(ok_unit())
}

pub(crate) fn umount_all(types: &[String], span: Span) -> Result<Value, RuntimeError> {
    let mounts = match read_mounts("/proc/mounts") {
        Ok(mounts) => mounts,
        Err(error) => return Ok(io_error("linux-umount-all", error, span)),
    };
    for mount in mounts.into_iter().rev() {
        if mount.target == "/" || !type_filter_matches(types, &mount.fstype) {
            continue;
        }
        if let Err(error) = unmount(mount.target.as_str(), UnmountFlags::empty())
            && error != Errno::INVAL
            && error != Errno::NOENT
        {
            return Ok(io_error("linux-umount-all", io::Error::from(error), span));
        }
    }
    Ok(ok_unit())
}

pub(crate) fn swapon_all(span: Span) -> Result<Value, RuntimeError> {
    let entries = match read_fstab("/etc/fstab") {
        Ok(entries) => entries,
        Err(error) => return Ok(io_error("linux-swapon-all", error, span)),
    };
    for entry in entries {
        if entry.vfstype != "swap" || option_present(&entry.mntops, "noauto") {
            continue;
        }
        let device = cstring_text(&entry.spec, "linux-swapon-all", span)?;
        let rc = unsafe { libc::swapon(device.as_ptr(), 0) };
        if rc != 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::EBUSY) {
                return Ok(io_error("linux-swapon-all", error, span));
            }
        }
    }
    Ok(ok_unit())
}

pub(crate) fn swapoff_all(span: Span) -> Result<Value, RuntimeError> {
    let text = match fs::read_to_string("/proc/swaps") {
        Ok(text) => text,
        Err(error) => return Ok(io_error("linux-swapoff-all", error, span)),
    };
    for line in text.lines().skip(1) {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.is_empty() {
            continue;
        }
        let device = cstring_text(fields[0], "linux-swapoff-all", span)?;
        let rc = unsafe { libc::swapoff(device.as_ptr()) };
        if rc != 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::EINVAL) {
                return Ok(io_error("linux-swapoff-all", error, span));
            }
        }
    }
    Ok(ok_unit())
}

pub(crate) fn root_device(span: Span) -> Result<Value, RuntimeError> {
    let mounts = match read_mounts("/proc/mounts") {
        Ok(mounts) => mounts,
        Err(error) => return Ok(io_error("linux-root-device", error, span)),
    };
    for mount in mounts {
        if mount.target == "/" {
            return Ok(Value::ok(str_value(mount.source)));
        }
    }
    Ok(error_value(
        "linux-root-device",
        "root mount not found",
        span,
    ))
}

fn mount_one(
    source: &str,
    target: &Path,
    fstype: &str,
    options: &[String],
    span: Span,
) -> Result<(), RuntimeError> {
    let spec = mount_options(options);
    let joined = spec.data.join(",");
    let data = (!joined.is_empty()).then_some(joined.as_str());

    // mount(2) dispatches on the flags with a fixed priority: a remount, bind,
    // or move request is handled before a propagation-type change, which in
    // turn precedes an ordinary new mount. rustix exposes each operation as its
    // own call, so we mirror that priority here.
    let result = if spec.remount {
        mount_remount(target, spec.flags, data.unwrap_or(""))
    } else if spec.bind {
        if spec.recursive {
            mount_bind_recursive(source, target)
        } else {
            mount_bind(source, target)
        }
    } else if spec.relocate {
        mount_move(source, target)
    } else if !spec.propagation.is_empty() {
        let mut propagation = spec.propagation;
        if spec.recursive {
            propagation |= MountPropagationFlags::REC;
        }
        mount_change(target, propagation)
    } else {
        let data_c = match data.map(CString::new).transpose() {
            Ok(data_c) => data_c,
            Err(_) => {
                return Err(
                    RuntimeError::new("linux-mount", "mount data contains NUL").with_span(span)
                );
            }
        };
        rustix::mount::mount(source, target, fstype, spec.flags, data_c.as_deref())
    };

    match result {
        Ok(()) => Ok(()),
        Err(error) if error == Errno::BUSY && target_is_mounted(target) => Ok(()),
        Err(error) => Err(
            RuntimeError::new("linux-mount", io::Error::from(error).to_string()).with_span(span),
        ),
    }
}

/// Parsed mount options, split the way rustix's typed mount API expects: plain
/// filesystem `flags`, the mount operation (`remount`/`bind`/`relocate`),
/// propagation-type changes, and any leftover free-form `data`.
struct MountSpec {
    flags: MountFlags,
    propagation: MountPropagationFlags,
    remount: bool,
    bind: bool,
    relocate: bool,
    recursive: bool,
    data: Vec<String>,
}

fn mount_options(options: &[String]) -> MountSpec {
    let mut spec = MountSpec {
        flags: MountFlags::empty(),
        propagation: MountPropagationFlags::empty(),
        remount: false,
        bind: false,
        relocate: false,
        recursive: false,
        data: Vec::new(),
    };
    for option in options {
        match option.as_str() {
            "" | "defaults" | "rw" => {}
            "ro" => spec.flags |= MountFlags::RDONLY,
            "nosuid" => spec.flags |= MountFlags::NOSUID,
            "nodev" => spec.flags |= MountFlags::NODEV,
            "noexec" => spec.flags |= MountFlags::NOEXEC,
            "sync" => spec.flags |= MountFlags::SYNCHRONOUS,
            "dirsync" => spec.flags |= MountFlags::DIRSYNC,
            "remount" => spec.remount = true,
            "mand" => spec.flags |= MountFlags::PERMIT_MANDATORY_FILE_LOCKING,
            "noatime" => spec.flags |= MountFlags::NOATIME,
            "nodiratime" => spec.flags |= MountFlags::NODIRATIME,
            "bind" => spec.bind = true,
            "rbind" => {
                spec.bind = true;
                spec.recursive = true;
            }
            "move" => spec.relocate = true,
            "rec" => spec.recursive = true,
            "silent" => spec.flags |= MountFlags::SILENT,
            // rustix has no `MS_POSIXACL` constant; it is bit 1 << 16.
            "posixacl" => spec.flags |= MountFlags::from_bits_retain(1 << 16),
            "unbindable" => spec.propagation |= MountPropagationFlags::UNBINDABLE,
            "private" => spec.propagation |= MountPropagationFlags::PRIVATE,
            "slave" => spec.propagation |= MountPropagationFlags::DOWNSTREAM,
            "shared" => spec.propagation |= MountPropagationFlags::SHARED,
            "relatime" => spec.flags |= MountFlags::RELATIME,
            "strictatime" => spec.flags |= MountFlags::STRICTATIME,
            value => spec.data.push(value.to_string()),
        }
    }
    spec
}

fn read_fstab(path: &str) -> io::Result<Vec<FstabEntry>> {
    let text = fs::read_to_string(path)?;
    Ok(text.lines().filter_map(parse_fstab_line).collect())
}

fn parse_fstab_line(line: &str) -> Option<FstabEntry> {
    let line = line.split('#').next()?.trim();
    if line.is_empty() {
        return None;
    }
    let fields = line.split_whitespace().collect::<Vec<_>>();
    if fields.len() < 4 {
        return None;
    }
    Some(FstabEntry {
        spec: unescape_field(fields[0]),
        file: unescape_field(fields[1]),
        vfstype: fields[2].to_string(),
        mntops: fields[3]
            .split(',')
            .filter(|option| !option.is_empty())
            .map(str::to_string)
            .collect(),
    })
}

pub(super) fn read_mounts(path: &str) -> io::Result<Vec<MountEntry>> {
    let text = fs::read_to_string(path)?;
    Ok(text.lines().filter_map(parse_mount_line).collect())
}

fn parse_mount_line(line: &str) -> Option<MountEntry> {
    let fields = line.split_whitespace().collect::<Vec<_>>();
    if fields.len() < 3 {
        return None;
    }
    Some(MountEntry {
        source: unescape_field(fields[0]),
        target: unescape_field(fields[1]),
        fstype: fields[2].to_string(),
    })
}

fn target_is_mounted(target: &Path) -> bool {
    let target = target.to_string_lossy();
    read_mounts("/proc/mounts")
        .ok()
        .is_some_and(|mounts| mounts.iter().any(|mount| mount.target == target))
}

fn option_present(options: &[String], target: &str) -> bool {
    options.iter().any(|option| {
        option == target
            || option
                .split_once('=')
                .is_some_and(|(name, _)| name == target)
    })
}

fn type_filter_matches(filters: &[String], fstype: &str) -> bool {
    let excluded = filters
        .iter()
        .filter_map(|item| item.strip_prefix("no"))
        .collect::<Vec<_>>();
    if excluded.contains(&fstype) {
        return false;
    }
    let included = filters
        .iter()
        .filter(|item| !item.starts_with("no"))
        .map(String::as_str)
        .collect::<Vec<_>>();
    included.is_empty() || included.contains(&fstype)
}

fn unescape_field(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' && index + 3 < bytes.len() {
            let octal = &value[index + 1..index + 4];
            if octal
                .as_bytes()
                .iter()
                .all(|byte| (b'0'..=b'7').contains(byte))
                && let Ok(decoded) = u8::from_str_radix(octal, 8)
            {
                result.push(decoded as char);
                index += 4;
                continue;
            }
        }
        result.push(bytes[index] as char);
        index += 1;
    }
    result
}
