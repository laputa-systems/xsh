use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;

/// Lists the namespaces held by running processes, one record per namespace,
/// ordered by inode number. With a pid, only the namespaces of that process
/// are listed (its process counts still cover every process).
pub(crate) fn namespaces(pid: Option<i64>, span: Span) -> Result<Value, RuntimeError> {
    match imp::namespaces(pid, span) {
        Ok(value) => Ok(Value::ok(value)),
        Err(error) => Ok(Value::err(Value::Error(Box::new(error)))),
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use crate::runtime::namespace::NamespaceKind;
    use crate::runtime::value::{PathValue, RecordMap, RuntimeError, Value};
    use crate::source::Span;
    use std::collections::BTreeMap;
    use std::fs;
    use std::io;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::MetadataExt;
    use std::path::PathBuf;
    use std::sync::Arc;

    const NS_GET_USERNS: libc::c_ulong = 0xb701;
    const NS_GET_PARENT: libc::c_ulong = 0xb702;
    const RTM_NEWNSID: u16 = 88;
    const RTM_GETNSID: u16 = 90;
    const NLMSG_ERROR: u16 = 2;
    const NETNSA_NSID: u16 = 1;
    const NETNSA_FD: u16 = 3;
    const NETNSA_NSID_NOT_ASSIGNED: i32 = -1;

    struct Namespace {
        kind: &'static str,
        path: PathBuf,
        nprocs: i64,
        pid: i64,
        ppid: i64,
        uid: i64,
        command: String,
    }

    struct Process {
        ppid: i64,
        uid: i64,
        command: String,
    }

    pub(super) fn namespaces(pid_filter: Option<i64>, span: Span) -> Result<Value, RuntimeError> {
        let kind = "linux-namespaces";
        let host = |error: &io::Error| RuntimeError::host(kind, error).with_span(span);
        let mut pids = Vec::new();
        for entry in fs::read_dir("/proc").map_err(|error| host(&error))? {
            let entry = entry.map_err(|error| host(&error))?;
            if let Some(pid) = entry.file_name().to_str().and_then(|name| name.parse::<i64>().ok()) {
                pids.push(pid);
            }
        }
        pids.sort_unstable();

        let wanted: Option<Vec<(&'static str, u64)>> = pid_filter.map(|pid| {
            NamespaceKind::NAMES
                .iter()
                .filter_map(|name| ns_link(pid, name))
                .collect()
        });

        // Processes that vanish or cannot be inspected hold no readable
        // namespace link and are left out, as the kernel's permission checks
        // intend; the lowest readable pid represents each namespace.
        let mut found: BTreeMap<(u64, &'static str), Namespace> = BTreeMap::new();
        for pid in pids {
            let links: Vec<(&'static str, u64)> = NamespaceKind::NAMES
                .iter()
                .filter_map(|name| ns_link(pid, name))
                .collect();
            if links.is_empty() {
                continue;
            }
            let mut process: Option<Process> = None;
            for (name, inode) in links {
                if let Some(entry) = found.get_mut(&(inode, name)) {
                    entry.nprocs += 1;
                    continue;
                }
                if process.is_none() {
                    process = read_process(pid);
                }
                let Some(process) = &process else { break };
                found.insert(
                    (inode, name),
                    Namespace {
                        kind: name,
                        path: PathBuf::from(format!("/proc/{pid}/ns/{name}")),
                        nprocs: 1,
                        pid,
                        ppid: process.ppid,
                        uid: process.uid,
                        command: process.command.clone(),
                    },
                );
            }
        }

        let mounts = nsfs_mounts();
        let mut records = Vec::new();
        for ((inode, name), entry) in &found {
            if let Some(wanted) = &wanted
                && !wanted.contains(&(*name, *inode))
            {
                continue;
            }
            let fd = open_namespace(&entry.path);
            let (parent, owner) = match &fd {
                Some(fd) => (
                    if matches!(entry.kind, "pid" | "user") {
                        related_inode(fd, NS_GET_PARENT)
                    } else {
                        0
                    },
                    related_inode(fd, NS_GET_USERNS),
                ),
                None => (0, 0),
            };
            let netnsid = match (&fd, entry.kind) {
                (Some(fd), "net") => network_namespace_id(fd),
                _ => None,
            };
            let path = PathValue::new(entry.path.as_os_str().as_bytes().to_vec())
                .map_err(|error| error.with_span(span))?;
            let nsfs = mounts
                .iter()
                .filter(|(mount_kind, mount_inode, _)| mount_kind == entry.kind && mount_inode == inode)
                .map(|(_, _, target)| {
                    PathValue::new(target.as_bytes().to_vec())
                        .map(Value::Path)
                        .map_err(|error| error.with_span(span))
                })
                .collect::<Result<Vec<_>, _>>()?;
            records.push(Value::Record(RecordMap::from([
                (Arc::from("ns"), Value::Int(*inode as i64)),
                (Arc::from("type"), Value::Str(entry.kind.into())),
                (Arc::from("path"), Value::Path(path)),
                (Arc::from("nprocs"), Value::Int(entry.nprocs)),
                (Arc::from("pid"), Value::Int(entry.pid)),
                (Arc::from("ppid"), Value::Int(entry.ppid)),
                (Arc::from("uid"), Value::Int(entry.uid)),
                (Arc::from("command"), Value::Str(entry.command.as_str().into())),
                (Arc::from("pns"), Value::Int(parent as i64)),
                (Arc::from("ons"), Value::Int(owner as i64)),
                (
                    Arc::from("netnsid"),
                    netnsid.map(Value::Int).unwrap_or(Value::Null),
                ),
                (Arc::from("nsfs"), Value::List(nsfs)),
            ])));
        }
        Ok(Value::List(records))
    }

    /// The inode a `/proc/PID/ns/NAME` link names (`net:[4026531840]`).
    fn ns_link(pid: i64, name: &'static str) -> Option<(&'static str, u64)> {
        let target = fs::read_link(format!("/proc/{pid}/ns/{name}")).ok()?;
        let text = target.to_str()?;
        let inode = text.split_once(":[")?.1.strip_suffix(']')?.parse().ok()?;
        Some((name, inode))
    }

    fn read_process(pid: i64) -> Option<Process> {
        let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        // The command name may contain spaces and parentheses, so the fields
        // after it are found from its closing parenthesis.
        let after = &stat[stat.rfind(')')? + 1..];
        let ppid = after.split_whitespace().nth(1)?.parse().ok()?;
        let uid = i64::from(fs::metadata(format!("/proc/{pid}")).ok()?.uid());
        let cmdline = fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
        let mut words: Vec<u8> = cmdline
            .iter()
            .map(|byte| if *byte == 0 { b' ' } else { *byte })
            .collect();
        while words.last() == Some(&b' ') {
            words.pop();
        }
        let command = if words.is_empty() {
            let name = fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
            format!("[{}]", name.trim_end())
        } else {
            String::from_utf8_lossy(&words).into_owned()
        };
        Some(Process { ppid, uid, command })
    }

    fn open_namespace(path: &std::path::Path) -> Option<OwnedFd> {
        rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
        .ok()
    }

    /// The inode of the namespace `request` relates `fd` to, or 0 when there
    /// is none or the caller may not see it (the initial user namespace has
    /// no owner, and only pid and user namespaces have a parent).
    fn related_inode(fd: &OwnedFd, request: libc::c_ulong) -> u64 {
        // SAFETY: these requests take no argument and return a new descriptor.
        let related = unsafe { libc::ioctl(fd.as_raw_fd(), request as _) };
        if related < 0 {
            return 0;
        }
        // SAFETY: the kernel returned a descriptor this call now owns.
        let related = unsafe { OwnedFd::from_raw_fd(related) };
        rustix::fs::fstat(&related).map_or(0, |status| status.st_ino)
    }

    /// The identifier the network subsystem assigned to a network namespace
    /// as seen from this one, or `None` when none is assigned or the kernel
    /// cannot report one.
    fn network_namespace_id(fd: &OwnedFd) -> Option<i64> {
        let socket = rustix::net::socket_with(
            rustix::net::AddressFamily::NETLINK,
            rustix::net::SocketType::RAW,
            rustix::net::SocketFlags::CLOEXEC,
            // Protocol 0 is NETLINK_ROUTE.
            None,
        )
        .ok()?;

        let mut request = Vec::with_capacity(28);
        request.extend_from_slice(&28u32.to_ne_bytes());
        request.extend_from_slice(&RTM_GETNSID.to_ne_bytes());
        request.extend_from_slice(&(libc::NLM_F_REQUEST as u16).to_ne_bytes());
        request.extend_from_slice(&1u32.to_ne_bytes());
        request.extend_from_slice(&0u32.to_ne_bytes());
        request.extend_from_slice(&[0u8; 4]);
        request.extend_from_slice(&8u16.to_ne_bytes());
        request.extend_from_slice(&NETNSA_FD.to_ne_bytes());
        request.extend_from_slice(&(fd.as_raw_fd() as u32).to_ne_bytes());
        rustix::net::send(&socket, &request, rustix::net::SendFlags::empty()).ok()?;
        let mut reply = [0u8; 256];
        let (received, _) =
            rustix::net::recv(&socket, &mut reply[..], rustix::net::RecvFlags::empty()).ok()?;
        if received < 16 {
            return None;
        }
        let reply = &reply[..received];
        let message_type = u16::from_ne_bytes([reply[4], reply[5]]);
        if message_type == NLMSG_ERROR || message_type != RTM_NEWNSID {
            return None;
        }
        // Attributes follow the 16-byte header and the 4-byte rtgenmsg.
        let mut at = 20;
        while at + 4 <= reply.len() {
            let length = usize::from(u16::from_ne_bytes([reply[at], reply[at + 1]]));
            let attribute = u16::from_ne_bytes([reply[at + 2], reply[at + 3]]);
            if length < 4 {
                return None;
            }
            if attribute == NETNSA_NSID && at + 8 <= reply.len() {
                let id = i32::from_ne_bytes([reply[at + 4], reply[at + 5], reply[at + 6], reply[at + 7]]);
                return (id != NETNSA_NSID_NOT_ASSIGNED).then_some(i64::from(id));
            }
            at += (length + 3) & !3;
        }
        None
    }

    /// Mount points of namespace files bound into the filesystem, as
    /// (type, inode, mount point), in mount table order.
    fn nsfs_mounts() -> Vec<(String, u64, String)> {
        let Ok(table) = fs::read_to_string("/proc/self/mountinfo") else {
            return Vec::new();
        };
        let mut mounts = Vec::new();
        for line in table.lines() {
            let Some((before, after)) = line.split_once(" - ") else {
                continue;
            };
            if after.split(' ').next() != Some("nsfs") {
                continue;
            }
            let mut fields = before.split(' ');
            let (Some(root), Some(target)) = (fields.nth(3), fields.next()) else {
                continue;
            };
            let Some((kind, rest)) = root.split_once(":[") else {
                continue;
            };
            let Some(inode) = rest.strip_suffix(']').and_then(|text| text.parse().ok()) else {
                continue;
            };
            mounts.push((kind.to_string(), inode, unescape_mount_field(target)));
        }
        mounts
    }

    /// Reverses the octal escapes mountinfo uses for spaces and newlines.
    fn unescape_mount_field(field: &str) -> String {
        let bytes = field.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut at = 0;
        while at < bytes.len() {
            if bytes[at] == b'\\'
                && at + 4 <= bytes.len()
                && bytes[at + 1..at + 4].iter().all(|digit| (b'0'..=b'7').contains(digit))
            {
                let value = u32::from(bytes[at + 1] - b'0') * 64
                    + u32::from(bytes[at + 2] - b'0') * 8
                    + u32::from(bytes[at + 3] - b'0');
                out.push(value as u8);
                at += 4;
            } else {
                out.push(bytes[at]);
                at += 1;
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use crate::runtime::value::{RuntimeError, Value};
    use crate::source::Span;

    pub(super) fn namespaces(_pid: Option<i64>, span: Span) -> Result<Value, RuntimeError> {
        Err(RuntimeError::new(
            "linux-unsupported",
            "real linux.* primitives are only available on Linux",
        )
        .with_span(span))
    }
}
