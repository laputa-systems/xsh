use super::SYSLOG_ACTION_SIZE_BUFFER;
use super::common::io_error;
use super::{DEV_KMSG, SYSLOG_ACTION_READ_ALL};
use crate::modules::linux::str_value;
use crate::runtime::value::{LiveStream, RuntimeError, StreamValue, Value};
use crate::source::Span;
use std::fs::OpenOptions;
use std::io::{self, Read};
use std::os::unix::fs::OpenOptionsExt;

// Superseded Linux bodies, deliberately not kept.
//
// On Linux the registry binds `linux.meminfo` and `linux.modules` to the
// embedded `linux_text` module, which owns both policies: the `/proc/meminfo`
// and `/proc/modules` paths, the parsing, the dry-run gate, and the
// `linux-meminfo` / `linux-modules` error kinds. The native bodies that used to
// do that work were a second implementation of the same policy, so they were
// removed once the embedded entries were verified on Linux.
//
// Nothing on Linux reaches these arms — the embedded implementation is
// selected at lowering time — so they report the defect of being reached at
// all rather than answering from a retired implementation.
fn retired_text_entry(name: &str, span: Span) -> RuntimeError {
    RuntimeError::new(
        "linux-retired-native-body",
        format!("the embedded standard-library implementation owns {name} on Linux"),
    )
    .with_span(span)
}

pub(crate) fn meminfo(span: Span) -> Result<Value, RuntimeError> {
    Err(retired_text_entry("linux.meminfo", span))
}

pub(crate) fn modules(span: Span) -> Result<Value, RuntimeError> {
    Err(retired_text_entry("linux.modules", span))
}

pub(crate) fn dmesg(span: Span) -> Result<Value, RuntimeError> {
    match read_kmsg() {
        Ok(messages) => {
            return Ok(Value::ok(Value::stream(StreamValue::from_live(
                "linux.dmesg",
                KernelMessageStream {
                    messages: messages.into_iter(),
                },
            ))));
        }
        Err(error) if !matches!(error.kind(), io::ErrorKind::NotFound) => {
            if !matches!(
                error.raw_os_error(),
                Some(libc::EACCES | libc::EPERM | libc::ENODEV)
            ) {
                return Ok(io_error("linux-dmesg", error, span));
            }
        }
        Err(_) => {}
    }

    match read_kernel_log() {
        Ok(messages) => Ok(Value::ok(Value::stream(StreamValue::from_live(
            "linux.dmesg",
            KernelMessageStream {
                messages: messages.into_iter(),
            },
        )))),
        Err(error) => Ok(io_error("linux-dmesg", error, span)),
    }
}

struct KernelMessageStream {
    messages: std::vec::IntoIter<String>,
}

impl LiveStream for KernelMessageStream {
    fn next(&mut self, _span: Span) -> Result<Option<Value>, RuntimeError> {
        Ok(self.messages.next().map(str_value))
    }
}

fn read_kmsg() -> io::Result<Vec<String>> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(DEV_KMSG)?;
    let mut text = String::new();
    match file.read_to_string(&mut text) {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::WouldBlock && !text.is_empty() => {}
        Err(error) => return Err(error),
    }
    Ok(text
        .lines()
        .filter_map(parse_kmsg_line)
        .map(str::to_owned)
        .collect())
}

fn parse_kmsg_line(line: &str) -> Option<&str> {
    line.split_once(';').map(|(_, message)| message)
}

fn read_kernel_log() -> io::Result<Vec<String>> {
    let size = klogctl(SYSLOG_ACTION_SIZE_BUFFER, std::ptr::null_mut(), 0)?;
    if size <= 0 {
        return Ok(Vec::new());
    }
    let mut buffer = vec![0_u8; size as usize];
    let read = klogctl(
        SYSLOG_ACTION_READ_ALL,
        buffer.as_mut_ptr().cast(),
        buffer.len() as libc::c_int,
    )?;
    let text = String::from_utf8_lossy(&buffer[..read as usize]);
    Ok(text
        .lines()
        .map(strip_syslog_prefix)
        .map(str::to_owned)
        .collect())
}

fn klogctl(
    action: libc::c_int,
    buffer: *mut libc::c_char,
    length: libc::c_int,
) -> io::Result<libc::c_int> {
    // SAFETY: the action code is selected by this module; when a buffer is
    // provided it points to writable memory for at least `length` bytes.
    let rc = unsafe { libc::klogctl(action, buffer, length) };
    if rc >= 0 {
        Ok(rc)
    } else {
        Err(io::Error::last_os_error())
    }
}

fn strip_syslog_prefix(line: &str) -> &str {
    let Some(rest) = line.strip_prefix('<') else {
        return line;
    };
    let digits = rest.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if digits == 0 || rest.as_bytes().get(digits) != Some(&b'>') {
        return line;
    }
    &rest[digits + 1..]
}
