//! Candidate storage devices from sysfs: SCSI disks (`sd*`), NVMe namespaces
//! (`nvme<c>n<n>`), and NVMe controllers (`nvme<c>`). Discovery only reads
//! sysfs attributes; it never opens a device node.

use super::args::StorageArgs;
use crate::runtime::value::{PathValue, RecordMap, RuntimeError, Value};
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const KIND: &str = "linux-storage-candidates";

#[derive(Clone, Copy, Eq, PartialEq)]
enum Kind {
    ScsiDisk,
    NvmeNamespace,
    NvmeController,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::ScsiDisk => "scsi_disk",
            Self::NvmeNamespace => "nvme_namespace",
            Self::NvmeController => "nvme_controller",
        }
    }

    fn protocol(self) -> &'static str {
        match self {
            Self::ScsiDisk => "scsi",
            Self::NvmeNamespace | Self::NvmeController => "nvme",
        }
    }
}

/// `sd` followed by letters only: whole disks, never partitions.
fn is_scsi_disk(name: &str) -> bool {
    name.strip_prefix("sd")
        .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_lowercase()))
}

/// The controller number of `nvme<c>`, and the namespace number too when the
/// name is `nvme<c>n<n>`. Multipath path nodes such as `nvme0c1n1` have no
/// device node and do not match.
fn nvme_numbers(name: &str) -> Option<(u64, Option<u64>)> {
    let rest = name.strip_prefix("nvme")?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let controller = rest[..digits].parse().ok()?;
    match &rest[digits..] {
        "" if digits > 0 => Some((controller, None)),
        tail if digits > 0 => {
            let namespace = tail.strip_prefix('n')?;
            if namespace.is_empty() || !namespace.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            Some((controller, Some(namespace.parse().ok()?)))
        }
        _ => None,
    }
}

fn attribute(directory: &Path, name: &str) -> Value {
    match fs::read_to_string(directory.join(name)) {
        Ok(text) => Value::Str(text.trim().into()),
        Err(_) => Value::Null,
    }
}

fn path_value(path: PathBuf) -> Result<Value, RuntimeError> {
    PathValue::new(path.as_os_str().as_bytes().to_vec()).map(Value::Path)
}

struct Candidate {
    name: String,
    order: (u64, u64),
    record: Value,
}

fn candidate(
    name: &str,
    kind: Kind,
    sys_directory: &Path,
    dev_root: &Path,
    order: (u64, u64),
) -> Result<Candidate, RuntimeError> {
    let attributes = match kind {
        Kind::ScsiDisk | Kind::NvmeNamespace => sys_directory.join("device"),
        Kind::NvmeController => sys_directory.to_path_buf(),
    };
    let (model, serial, firmware) = match kind {
        Kind::ScsiDisk => (
            attribute(&attributes, "model"),
            attribute(&attributes, "serial"),
            attribute(&attributes, "rev"),
        ),
        Kind::NvmeNamespace | Kind::NvmeController => (
            attribute(&attributes, "model"),
            attribute(&attributes, "serial"),
            attribute(&attributes, "firmware_rev"),
        ),
    };
    let number = |file: &str| {
        fs::read_to_string(sys_directory.join(file))
            .ok()
            .and_then(|text| text.trim().parse::<i64>().ok())
    };
    let (size_bytes, rotational, removable) = if kind == Kind::NvmeController {
        (Value::Null, Value::Null, Value::Null)
    } else {
        let sectors = number("size");
        // The kernel counts size in 512-byte units regardless of the logical
        // block size.
        (
            sectors.map_or(Value::Null, |sectors| Value::Int(sectors.saturating_mul(512))),
            number("queue/rotational").map_or(Value::Null, |value| Value::Bool(value != 0)),
            number("removable").map_or(Value::Null, |value| Value::Bool(value != 0)),
        )
    };
    let controller = match (kind, nvme_numbers(name)) {
        (Kind::NvmeNamespace, Some((controller, _))) => {
            path_value(dev_root.join(format!("nvme{controller}")))?
        }
        _ => Value::Null,
    };
    let record = Value::Record(RecordMap::from([
        (Arc::from("name"), Value::Str(name.into())),
        (Arc::from("path"), path_value(dev_root.join(name))?),
        (Arc::from("kind"), Value::Str(kind.name().into())),
        (Arc::from("protocol"), Value::Str(kind.protocol().into())),
        (Arc::from("model"), model),
        (Arc::from("serial"), serial),
        (Arc::from("firmware"), firmware),
        (Arc::from("size_bytes"), size_bytes),
        (Arc::from("rotational"), rotational),
        (Arc::from("removable"), removable),
        (Arc::from("controller"), controller),
    ]));
    Ok(Candidate { name: name.to_string(), order, record })
}

fn entries(directory: &Path) -> Result<Vec<String>, RuntimeError> {
    let read = match fs::read_dir(directory) {
        Ok(read) => read,
        // A system without NVMe has no /sys/class/nvme.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(RuntimeError::host(KIND, &error)),
    };
    let mut names = Vec::new();
    for entry in read {
        let entry = entry.map_err(|error| RuntimeError::host(KIND, &error))?;
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    Ok(names)
}

/// `linux.storage_candidates(sys_root, dev_root)`.
pub(super) fn candidates(args: &StorageArgs<'_>) -> Result<Value, RuntimeError> {
    let span = args.span();
    let sys_root = args.path_opt(0)?.unwrap_or_else(|| PathBuf::from("/sys"));
    let dev_root = args.path_opt(1)?.unwrap_or_else(|| PathBuf::from("/dev"));
    let block = sys_root.join("block");
    let mut found = Vec::new();
    let mut gather = || -> Result<(), RuntimeError> {
        for name in entries(&block)? {
            let kind = if is_scsi_disk(&name) {
                Kind::ScsiDisk
            } else if matches!(nvme_numbers(&name), Some((_, Some(_)))) {
                Kind::NvmeNamespace
            } else {
                continue;
            };
            let (controller, namespace) = nvme_numbers(&name).unwrap_or((0, None));
            found.push(candidate(
                &name,
                kind,
                &block.join(&name),
                &dev_root,
                (controller, namespace.map_or(0, |namespace| namespace + 1)),
            )?);
        }
        let class = sys_root.join("class/nvme");
        for name in entries(&class)? {
            let Some((controller, None)) = nvme_numbers(&name) else {
                continue;
            };
            found.push(candidate(
                &name,
                Kind::NvmeController,
                &class.join(&name),
                &dev_root,
                (controller, 0),
            )?);
        }
        Ok(())
    };
    gather().map_err(|error| error.with_span(span))?;
    // SCSI disks first by name length then name (sda < sdb < sdaa), then
    // NVMe controllers followed by their namespaces in numeric order.
    found.sort_by(|left, right| {
        let family = |candidate: &Candidate| u8::from(candidate.name.starts_with("nvme"));
        family(left)
            .cmp(&family(right))
            .then_with(|| {
                if family(left) == 0 {
                    left.name.len().cmp(&right.name.len()).then_with(|| left.name.cmp(&right.name))
                } else {
                    left.order.cmp(&right.order)
                }
            })
    });
    Ok(Value::List(found.into_iter().map(|candidate| candidate.record).collect()))
}
