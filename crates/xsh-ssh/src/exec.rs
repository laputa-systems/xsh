use std::fs::File;
use std::io::{Read, Write};
use std::time::Duration;

use crate::client::Connection;
use crate::error::Error;
use crate::stream::{self, InputSource, StreamOptions};
use crate::wire::Interrupt;

/// Bounded output capture limit for a single exec.
pub const DEFAULT_MAX_OUTPUT: usize = 16 * 1024 * 1024;

/// Options for one exec request.
pub struct ExecOptions {
    /// Bytes to send on stdin before EOF.
    pub stdin: Option<Vec<u8>>,
    /// A file whose contents follow `stdin` before EOF.
    pub stdin_file: Option<File>,
    /// A file that receives standard output instead of the bounded capture.
    pub stdout_file: Option<File>,
    /// A file that receives standard error instead of the bounded capture.
    pub stderr_file: Option<File>,
    /// The whole-command deadline.
    pub timeout: Option<Duration>,
    /// Maximum bytes captured from each output stream that does not go to a file.
    pub max_output: usize,
    /// Reports that the caller wants the command abandoned.
    pub interrupt: Option<Interrupt>,
    /// Variables to ask the server to set, by SSH `env` request.
    pub env: Vec<(String, String)>,
}

impl Default for ExecOptions {
    fn default() -> Self {
        Self {
            stdin: None,
            stdin_file: None,
            stdout_file: None,
            stderr_file: None,
            timeout: None,
            max_output: DEFAULT_MAX_OUTPUT,
            interrupt: None,
            env: Vec::new(),
        }
    }
}

impl std::fmt::Debug for ExecOptions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ExecOptions")
            .field("stdin_len", &self.stdin.as_ref().map(Vec::len))
            .field("timeout", &self.timeout)
            .field("max_output", &self.max_output)
            .field("env_names", &self.env.iter().map(|(name, _)| name).collect::<Vec<_>>())
            .finish()
    }
}

/// The remote process outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecStatus {
    /// The process exited with this status.
    Exited(u32),
    /// The process was terminated by this signal name.
    Signaled(String),
    /// The channel closed without an exit status or signal.
    Missing,
}

/// Captured output and the remote outcome.
#[derive(Clone, PartialEq, Eq)]
pub struct ExecResult {
    /// The remote process outcome.
    pub status: ExecStatus,
    /// Captured standard output.
    pub stdout: Vec<u8>,
    /// Captured standard error.
    pub stderr: Vec<u8>,
}

impl std::fmt::Debug for ExecResult {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ExecResult")
            .field("status", &self.status)
            .field("stdout_len", &self.stdout.len())
            .field("stderr_len", &self.stderr.len())
            .finish()
    }
}

struct Capture {
    bytes: Vec<u8>,
    limit: usize,
}

impl Write for Capture {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        if self.bytes.len() + data.len() > self.limit {
            return Err(std::io::Error::new(
                std::io::ErrorKind::OutOfMemory,
                "capture limit exceeded",
            ));
        }
        self.bytes.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct Stdin<'a> {
    head: &'a [u8],
    tail: Option<&'a File>,
}

impl Read for Stdin<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if !self.head.is_empty() {
            return self.head.read(buffer);
        }
        match self.tail.as_mut() {
            Some(tail) => tail.read(buffer),
            None => Ok(0),
        }
    }
}

impl InputSource for Stdin<'_> {
    fn descriptor(&self) -> Option<std::os::fd::BorrowedFd<'_>> {
        if self.head.is_empty() {
            self.tail.and_then(InputSource::descriptor)
        } else {
            None
        }
    }
}

enum Output<'a> {
    Capture(Capture),
    File(&'a File),
}

impl<'a> Output<'a> {
    fn new(file: Option<&'a File>, limit: usize) -> Self {
        match file {
            Some(file) => Self::File(file),
            None => Self::Capture(Capture {
                bytes: Vec::new(),
                limit,
            }),
        }
    }

    fn sink(&mut self) -> &mut dyn Write {
        match self {
            Self::Capture(capture) => capture,
            Self::File(file) => file,
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        match self {
            Self::Capture(capture) => capture.bytes,
            Self::File(_) => Vec::new(),
        }
    }
}

pub(crate) fn exec(
    connection: &mut Connection,
    command: &str,
    options: &ExecOptions,
) -> Result<ExecResult, Error> {
    let mut stdout = Output::new(options.stdout_file.as_ref(), options.max_output);
    let mut stderr = Output::new(options.stderr_file.as_ref(), options.max_output);
    let mut stdin = Stdin {
        head: options.stdin.as_deref().unwrap_or_default(),
        tail: options.stdin_file.as_ref(),
    };
    let mut result = stream::exec_stream(
        connection,
        command,
        &StreamOptions {
            timeout: options.timeout,
            interrupt: options.interrupt.clone(),
            env: options.env.clone(),
        },
        Some(&mut stdin),
        stdout.sink(),
        stderr.sink(),
    )?;
    result.stdout = stdout.into_bytes();
    result.stderr = stderr.into_bytes();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_refuses_bytes_past_its_limit() {
        let mut capture = Capture {
            bytes: Vec::new(),
            limit: 4,
        };
        capture.write_all(b"1234").expect("within limit");
        assert!(capture.write_all(b"5").is_err());
        assert_eq!(capture.bytes, b"1234");
    }
}
