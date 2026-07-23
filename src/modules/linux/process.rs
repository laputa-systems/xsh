#![allow(clippy::single_call_fn)]

use super::block::path_value;
use super::str_value;
use crate::runtime::value::{LiveStream, RuntimeError, Value};
use crate::source::Span;
use rustc_hash::FxHashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

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
            .map_err(|error| {
                RuntimeError::new("linux-open-files", error.to_string()).with_span(span)
            })?
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
    entries: std::vec::IntoIter<(i64, PathBuf)>,
}

impl LiveStream for OpenFilesStream {
    fn next(&mut self, span: Span) -> Result<Option<Value>, RuntimeError> {
        loop {
            if let Some(current) = &mut self.current {
                for (fd, path) in current.entries.by_ref() {
                    let Ok(target) = fs::read_link(&path) else {
                        continue;
                    };
                    let target_text = target.to_string_lossy().into_owned();
                    let (kind, inode, protocol, local, remote) = describe_fd_target(
                        &target_text,
                        self.sockets.get(&socket_inode(&target_text)),
                    );
                    return Ok(Some(Value::Record(crate::runtime::value::RecordMap::from(
                        [
                            (Arc::from("pid"), Value::Int(current.pid as i64)),
                            (Arc::from("command"), str_value(current.command.clone())),
                            (Arc::from("fd"), Value::Int(fd)),
                            (Arc::from("type"), str_value(kind)),
                            (Arc::from("path"), Value::Path(path_value(&target, span)?)),
                            (Arc::from("inode"), Value::Int(inode)),
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
                    Some((fd, entry.path()))
                })
                .collect::<Vec<_>>();
            entries.sort_unstable_by_key(|(fd, _)| *fd);
            self.current = Some(OpenFilesPid {
                pid,
                command: process_command(pid),
                entries: entries.into_iter(),
            });
        }
    }
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
    read_unix_sockets(&mut sockets);
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

fn socket_inode(target: &str) -> i64 {
    target
        .strip_prefix("socket:[")
        .and_then(|value| value.strip_suffix(']'))
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

fn describe_fd_target(
    target: &str,
    socket: Option<&SocketInfo>,
) -> (String, i64, String, String, String) {
    if let Some(socket) = socket {
        return (
            "socket".to_string(),
            socket_inode(target),
            socket.protocol.clone(),
            socket.local.clone(),
            socket.remote.clone(),
        );
    }
    if target.starts_with("pipe:[") {
        (
            "pipe".to_string(),
            socket_inode(target),
            String::new(),
            String::new(),
            String::new(),
        )
    } else if target.starts_with("anon_inode:") {
        (
            "anon".to_string(),
            0,
            String::new(),
            String::new(),
            String::new(),
        )
    } else {
        (
            "file".to_string(),
            0,
            String::new(),
            String::new(),
            String::new(),
        )
    }
}
