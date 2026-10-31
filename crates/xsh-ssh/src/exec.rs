use std::time::Duration;

use oxish_proto::{MessageType, Encode};

use crate::channel::{Channel, handle_global_request};
use crate::error::{Certainty, Error};
use crate::wire::{SshReader, Transport};

/// Bounded output capture limit for a single exec.
pub const DEFAULT_MAX_OUTPUT: usize = 16 * 1024 * 1024;

/// Options for one exec request.
#[derive(Debug, Clone)]
pub struct ExecOptions {
    /// Bytes to send on stdin before EOF.
    pub stdin: Option<Vec<u8>>,
    /// The whole-command deadline.
    pub timeout: Option<Duration>,
    /// Maximum bytes captured from each output stream.
    pub max_output: usize,
}

impl Default for ExecOptions {
    fn default() -> Self {
        Self {
            stdin: None,
            timeout: None,
            max_output: DEFAULT_MAX_OUTPUT,
        }
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecResult {
    /// The remote process outcome.
    pub status: ExecStatus,
    /// Captured standard output.
    pub stdout: Vec<u8>,
    /// Captured standard error.
    pub stderr: Vec<u8>,
    /// The result is always submitted once a channel exists.
    pub certainty: Certainty,
}

/// Run one remote command with bounded captured output.
pub fn exec(
    transport: &mut Transport,
    command: &str,
    options: &ExecOptions,
) -> Result<ExecResult, Error> {
    let previous = transport.timeout();
    if let Some(timeout) = options.timeout {
        transport.set_timeout(timeout)?;
    }
    let result = run(transport, command, options);
    if options.timeout.is_some() {
        transport.set_timeout(previous)?;
    }
    result
}

fn run(
    transport: &mut Transport,
    command: &str,
    options: &ExecOptions,
) -> Result<ExecResult, Error> {
    let mut channel = Channel::open(transport)?;
    channel.exec(transport, command.as_bytes())?;
    if let Some(stdin) = &options.stdin {
        channel.send_data(transport, stdin)?;
    }
    channel.send_eof(transport)?;

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut exit_status = None;
    let mut exit_signal = None;
    let mut closed = false;

    while !closed {
        let packet = transport.recv()?;
        match packet.message_type {
            MessageType::ChannelData => {
                let mut reader = SshReader::new(&packet.payload);
                let recipient = reader.u32()?;
                let data = reader.string()?;
                reader.finish()?;
                if recipient != channel.local_id {
                    return Err(Error::Channel {
                        detail: format!("data for channel {recipient}"),
                    });
                }
                if stdout.len() + data.len() > options.max_output {
                    return Err(Error::Channel {
                        detail: "stdout exceeded the capture limit".to_owned(),
                    });
                }
                stdout.extend_from_slice(data);
                channel.acknowledge(transport, data.len())?;
            }
            MessageType::ChannelExtendedData => {
                let mut reader = SshReader::new(&packet.payload);
                let recipient = reader.u32()?;
                let data_type = reader.u32()?;
                let data = reader.string()?;
                reader.finish()?;
                if recipient != channel.local_id {
                    return Err(Error::Channel {
                        detail: format!("extended data for channel {recipient}"),
                    });
                }
                if data_type == 1 {
                    if stderr.len() + data.len() > options.max_output {
                        return Err(Error::Channel {
                            detail: "stderr exceeded the capture limit".to_owned(),
                        });
                    }
                    stderr.extend_from_slice(data);
                }
                channel.acknowledge(transport, data.len())?;
            }
            MessageType::ChannelWindowAdjust => {
                let mut reader = SshReader::new(&packet.payload);
                let _recipient = reader.u32()?;
                let added = reader.u32()?;
                reader.finish()?;
                channel.remote_window = channel.remote_window.saturating_add(added);
            }
            MessageType::ChannelRequest => {
                let mut reader = SshReader::new(&packet.payload);
                let _recipient = reader.u32()?;
                let request = reader.text()?.to_owned();
                let want_reply = reader.u8()? != 0;
                match request.as_str() {
                    "exit-status" => exit_status = Some(reader.u32()?),
                    "exit-signal" => {
                        exit_signal = Some(reader.text()?.to_owned());
                    }
                    _ => {}
                }
                if want_reply {
                    let mut reply = Vec::new();
                    MessageType::ChannelFailure.encode(&mut reply);
                    reply.extend_from_slice(&channel.remote_id.to_be_bytes());
                    transport.send_raw(&reply)?;
                }
            }
            MessageType::ChannelEof => {}
            MessageType::ChannelClose => {
                channel.send_close(transport)?;
                closed = true;
            }
            MessageType::ChannelSuccess | MessageType::ChannelFailure => {}
            MessageType::GlobalRequest => {
                handle_global_request(transport, &packet.payload)?;
            }
            MessageType::Ignore | MessageType::Debug | MessageType::Unimplemented => {}
            MessageType::Disconnect => {
                return Err(Error::Submitted {
                    detail: "server disconnected while the command was running".to_owned(),
                });
            }
            other => {
                return Err(Error::Protocol {
                    detail: format!("unexpected channel message {other:?}"),
                });
            }
        }
    }

    Ok(ExecResult {
        status: status_from(exit_status, exit_signal),
        stdout,
        stderr,
        certainty: Certainty::Submitted,
    })
}

fn status_from(exit_status: Option<u32>, exit_signal: Option<String>) -> ExecStatus {
    match (exit_status, exit_signal) {
        (Some(status), _) => ExecStatus::Exited(status),
        (None, Some(signal)) => ExecStatus::Signaled(signal),
        (None, None) => ExecStatus::Missing,
    }
}

#[cfg(test)]
mod tests {
    use super::{ExecStatus, status_from};

    #[test]
    fn missing_completion_is_not_status_zero() {
        assert_eq!(status_from(None, None), ExecStatus::Missing);
        assert_eq!(status_from(Some(0), None), ExecStatus::Exited(0));
        assert_eq!(
            status_from(None, Some("TERM".to_owned())),
            ExecStatus::Signaled("TERM".to_owned())
        );
    }
}
