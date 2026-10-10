#![allow(clippy::single_call_fn)]

use super::block::path_value;
use super::str_value;
use crate::runtime::value::{LiveStream, RuntimeError, Value};
use crate::source::Span;
use rustc_hash::FxHashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::os::unix::fs::{FileTypeExt, MetadataExt};

pub(super) fn open_files_impl(
    pid: Option<i64>,
    span: Span,
) -> Result<OpenFilesStream, RuntimeError> {
    let sockets = socket_index();
    let mut pids = if let Some(pid) = pid {
        if pid < 0 {
            return Err(
                RuntimeError::new("linux-open-files", "pid cannot be negative").with_span(span),
            );
        }
        vec![pid as i32]
    } else {
        fs::read_dir("/proc")
            .map_err(|error| RuntimeError::host("linux-open-files", &error).with_span(span))?
            .flatten()
            .filter_map(|entry| entry.file_name().to_str()?.parse::<i32>().ok())
            .collect()
    };
    pids.sort_unstable();
    Ok(OpenFilesStream {
        pids: pids.into_iter(),
        current: None,
        sockets,
    })
}

pub(super) struct OpenFilesStream {
    pids: std::vec::IntoIter<i32>,
    current: Option<OpenFilesPid>,
    sockets: FxHashMap<i64, SocketInfo>,
}

struct OpenFilesPid {
    pid: i32,
    command: String,
    entries: std::vec::IntoIter<(i64, String, PathBuf)>,
}

impl LiveStream for OpenFilesStream {
    fn next(&mut self, span: Span) -> Result<Option<Value>, RuntimeError> {
        loop {
            if let Some(current) = &mut self.current {
                for (fd, fd_label, path) in current.entries.by_ref() {
                    let Ok(target) = fs::read_link(&path) else {
                        continue;
                    };
                    let target_text = target.to_string_lossy().into_owned();
                    let metadata = fs::metadata(&path).ok();
                    let kind = descriptor_kind(
                        &target_text,
                        metadata.as_ref().map(fs::Metadata::file_type),
                    );
                    let inode = metadata
                        .as_ref()
                        .map_or_else(|| link_inode(&target_text), |metadata| metadata.ino() as i64);
                    let device = metadata.as_ref().map(|metadata| metadata.dev());
                    // Only a descriptor that is itself a socket consults the socket tables:
                    // table rows with inode 0 (for example TCP connections in TIME_WAIT) must
                    // never leak onto descriptors of other kinds.
                    let (protocol, local, remote) = match kind {
                        "socket" => self.sockets.get(&inode).map_or_else(
                            Default::default,
                            |socket| {
                                (socket.protocol.clone(), socket.local.clone(), socket.remote.clone())
                            },
                        ),
                        _ => Default::default(),
                    };
                    return Ok(Some(Value::Record(crate::runtime::value::RecordMap::from(
                        [
                            (Arc::from("pid"), Value::Int(current.pid as i64)),
                            (Arc::from("command"), str_value(current.command.clone())),
                            (Arc::from("fd"), Value::Int(fd)),
                            (Arc::from("fd_label"), str_value(fd_label)),
                            (Arc::from("access"), str_value(descriptor_access(&path))),
                            (Arc::from("type"), str_value(kind.to_string())),
                            (Arc::from("path"), Value::Path(path_value(&target, span)?)),
                            (Arc::from("inode"), Value::Int(inode)),
                            (Arc::from("dev"), device.map(|device| Value::Int(device as i64)).unwrap_or(Value::Null)),
                            (Arc::from("protocol"), str_value(protocol)),
                            (Arc::from("local"), str_value(local)),
                            (Arc::from("remote"), str_value(remote)),
                        ],
                    ))));
                }
                self.current = None;
            }

            let Some(pid) = self.pids.next() else {
                return Ok(None);
            };
            let Ok(entries) = fs::read_dir(format!("/proc/{pid}/fd")) else {
                continue;
            };
            let mut entries = entries
                .flatten()
                .filter_map(|entry| {
                    let fd = entry.file_name().to_str()?.parse::<i64>().ok()?;
                    Some((fd, fd.to_string(), entry.path()))
                })
                .collect::<Vec<_>>();
            entries.sort_unstable_by_key(|(fd, _, _)| *fd);
            for (link, label) in [("exe", "txt"), ("root", "rtd"), ("cwd", "cwd")] {
                entries.insert(0, (-1, label.to_string(), PathBuf::from(format!("/proc/{pid}/{link}"))));
            }
            self.current = Some(OpenFilesPid {
                pid,
                command: process_command(pid),
                entries: entries.into_iter(),
            });
        }
    }
}

// A vanished or inaccessible fdinfo record leaves access explicitly unknown.
// Special cwd/root/exe links are references, not open descriptors.
fn descriptor_access(path: &Path) -> String {
    let Some(number) = path.file_name().and_then(|name| name.to_str()).filter(|name| name.bytes().all(|byte| byte.is_ascii_digit())) else {
        return String::new();
    };
    let Some(process_root) = path.parent().and_then(Path::parent) else { return "?".to_string(); };
    let Ok(text) = fs::read_to_string(process_root.join("fdinfo").join(number)) else { return "?".to_string(); };
    let Some(flags) = text.lines().find_map(|line| line.strip_prefix("flags:\t"))
        .and_then(|flags| u32::from_str_radix(flags.trim(), 8).ok()) else { return "?".to_string(); };
    match flags & libc::O_ACCMODE as u32 {
        0 => "r", 1 => "w", 2 => "u", _ => "?",
    }.to_string()
}

struct SocketInfo {
    protocol: String,
    local: String,
    remote: String,
}

fn socket_index() -> FxHashMap<i64, SocketInfo> {
    let mut sockets = FxHashMap::default();
    read_inet_sockets("/proc/net/tcp", "tcp", false, &mut sockets);
    read_inet_sockets("/proc/net/tcp6", "tcp6", true, &mut sockets);
    read_inet_sockets("/proc/net/udp", "udp", false, &mut sockets);
    read_inet_sockets("/proc/net/udp6", "udp6", true, &mut sockets);
    read_inet_sockets("/proc/net/udplite", "udplite", false, &mut sockets);
    read_inet_sockets("/proc/net/udplite6", "udplite6", true, &mut sockets);
    read_inet_sockets("/proc/net/raw", "raw", false, &mut sockets);
    read_inet_sockets("/proc/net/raw6", "raw6", true, &mut sockets);
    read_inet_sockets("/proc/net/icmp", "icmp", false, &mut sockets);
    read_inet_sockets("/proc/net/icmp6", "icmp6", true, &mut sockets);
    read_unix_sockets(&mut sockets);
    read_netlink_sockets(&mut sockets);
    read_packet_sockets(&mut sockets);
    sockets
}

fn read_inet_sockets(
    path: &str,
    protocol: &str,
    ipv6: bool,
    sockets: &mut FxHashMap<i64, SocketInfo>,
) {
    let Ok(text) = fs::read_to_string(path) else {
        return;
    };
    for line in text.lines().skip(1) {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 10 {
            continue;
        }
        if let Ok(inode) = fields[9].parse::<i64>() {
            sockets.insert(
                inode,
                SocketInfo {
                    protocol: protocol.to_string(),
                    local: format_inet_addr(fields[1], ipv6),
                    remote: format_inet_addr(fields[2], ipv6),
                },
            );
        }
    }
}

fn read_unix_sockets(sockets: &mut FxHashMap<i64, SocketInfo>) {
    let Ok(text) = fs::read_to_string("/proc/net/unix") else {
        return;
    };
    for line in text.lines().skip(1) {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() >= 7
            && let Ok(inode) = fields[6].parse::<i64>()
        {
            sockets.insert(
                inode,
                SocketInfo {
                    protocol: "unix".to_string(),
                    local: fields.get(7).copied().unwrap_or("").to_string(),
                    remote: String::new(),
                },
            );
        }
    }
}

// Columns: sk Eth Pid Groups Rmem Wmem Dump Locks Drops Inode. The local address is the
// netlink port id.
fn read_netlink_sockets(sockets: &mut FxHashMap<i64, SocketInfo>) {
    let Ok(text) = fs::read_to_string("/proc/net/netlink") else {
        return;
    };
    for line in text.lines().skip(1) {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() >= 10
            && let Ok(inode) = fields[9].parse::<i64>()
        {
            sockets.insert(
                inode,
                SocketInfo {
                    protocol: "netlink".to_string(),
                    local: fields[2].to_string(),
                    remote: String::new(),
                },
            );
        }
    }
}

// Columns: sk RefCnt Type Proto Iface R Rmem User Inode.
fn read_packet_sockets(sockets: &mut FxHashMap<i64, SocketInfo>) {
    let Ok(text) = fs::read_to_string("/proc/net/packet") else {
        return;
    };
    for line in text.lines().skip(1) {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() >= 9
            && let Ok(inode) = fields[8].parse::<i64>()
        {
            sockets.insert(
                inode,
                SocketInfo {
                    protocol: "packet".to_string(),
                    local: String::new(),
                    remote: String::new(),
                },
            );
        }
    }
}

fn format_inet_addr(value: &str, ipv6: bool) -> String {
    let Some((addr, port)) = value.split_once(':') else {
        return value.to_string();
    };
    let port = u16::from_str_radix(port, 16).unwrap_or(0);
    if ipv6 {
        format!("{addr}:{port}")
    } else if addr.len() == 8 {
        let bytes = (0..4)
            .map(|index| u8::from_str_radix(&addr[index * 2..index * 2 + 2], 16).unwrap_or(0))
            .collect::<Vec<_>>();
        format!("{}.{}.{}.{}:{port}", bytes[3], bytes[2], bytes[1], bytes[0])
    } else {
        format!("{addr}:{port}")
    }
}

fn process_command(pid: i32) -> String {
    fs::read_to_string(format!("/proc/{pid}/comm"))
        .map(|value| value.trim().to_string())
        .unwrap_or_default()
}

// The inode embedded in a `kind:[inode]` link target (sockets, pipes, anon inodes).
fn link_inode(target: &str) -> i64 {
    target
        .split_once(":[")
        .and_then(|(_, rest)| rest.strip_suffix(']'))
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

// The kind comes from the descriptor's own type (file type of the link target), with the
// link text only deciding anonymous inodes and the case where the target cannot be stat'ed.
fn descriptor_kind(target: &str, file_type: Option<fs::FileType>) -> &'static str {
    if target.starts_with("anon_inode:") {
        return "anon";
    }
    if let Some(file_type) = file_type {
        return if file_type.is_socket() {
            "socket"
        } else if file_type.is_char_device() {
            "character"
        } else if file_type.is_block_device() {
            "block"
        } else if file_type.is_dir() {
            "directory"
        } else if file_type.is_fifo() {
            "pipe"
        } else {
            "file"
        };
    }
    if target.starts_with("socket:[") {
        "socket"
    } else if target.starts_with("pipe:[") {
        "pipe"
    } else {
        "file"
    }
}
