use std::io::{Read, Write};
use std::time::{Duration, Instant};

use oxish_proto::crypto::CryptoProvider;
use oxish_proto::{Encode, MessageType};
use rustix::event::PollFlags;

use crate::channel::{Channel, handle_global_request};
use crate::error::{Certainty, Error};
use crate::exec::{ExecResult, ExecStatus};
use crate::host_key::HostKeyPin;
use crate::kex;
use crate::wire::{SshReader, Transport, put_string};

/// Options for one streamed exec.
#[derive(Debug, Clone)]
pub struct StreamOptions {
    /// The whole-command deadline.
    pub timeout: Option<Duration>,
}

impl Default for StreamOptions {
    fn default() -> Self {
        Self {
            timeout: Some(Duration::from_secs(30)),
        }
    }
}

/// Run one remote command with streaming stdin and independent output sinks.
pub fn exec_stream(
    transport: &mut Transport,
    command: &str,
    options: &StreamOptions,
    stdin: Option<&mut dyn Read>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    pin: &HostKeyPin,
    provider: &dyn CryptoProvider,
    session_id: &[u8],
    strict_kex: bool,
) -> Result<ExecResult, Error> {
    let mut channel = Channel::open(transport)?;
    channel.exec(transport, command.as_bytes())?;
    transport.set_nonblocking(true)?;

    let deadline = Instant::now() + options.timeout.unwrap_or(Duration::from_secs(30));
    let mut stdin = stdin;
    let mut input = vec![0u8; 64 * 1024];
    let mut input_len = 0;
    let mut input_pos = 0;
    let mut input_eof = stdin.is_none();
    let mut eof_sent = false;
    let mut exit_status = None;
    let mut exit_signal = None;
    let mut closed = false;

    while !closed {
        if Instant::now() >= deadline {
            transport.shutdown();
            return Err(Error::Timeout {
                certainty: Certainty::Submitted,
            });
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        let want_write = transport.has_pending_output()
            || (!eof_sent && input_pos >= input_len)
            || (input_pos < input_len && channel.remote_window > 0);
        let events = transport.poll_ready(want_write, Some(remaining))?;

        if events.contains(PollFlags::IN) {
            if transport.fill_nonblocking()? {
                while let Some(packet) = transport.try_recv()? {
                    if packet.message_type == MessageType::KeyExchangeInit {
                        transport.set_nonblocking(false)?;
                        let queued =
                            kex::rekey(transport, pin, provider, session_id, strict_kex, packet)?;
                        transport.set_nonblocking(true)?;
                        for queued_packet in queued {
                            process_packet(
                                queued_packet,
                                transport,
                                &mut channel,
                                stdout,
                                stderr,
                                &mut exit_status,
                                &mut exit_signal,
                                &mut closed,
                            )?;
                        }
                        continue;
                    }
                    process_packet(
                        packet,
                        transport,
                        &mut channel,
                        stdout,
                        stderr,
                        &mut exit_status,
                        &mut exit_signal,
                        &mut closed,
                    )?;
                }
            }
        }

        if events.contains(PollFlags::OUT) {
            if input_pos >= input_len && !input_eof {
                if let Some(reader) = stdin.as_mut() {
                    input_len = reader.read(&mut input).map_err(|error| Error::Submitted {
                        detail: format!("stdin read failed: {error}"),
                    })?;
                    input_pos = 0;
                    if input_len == 0 {
                        input_eof = true;
                    }
                }
            }
            while input_pos < input_len && channel.remote_window > 0 {
                let limit = usize::try_from(channel.remote_window.min(channel.remote_max_packet))
                    .unwrap_or(usize::MAX)
                    .min(input_len - input_pos);
                let mut packet = Vec::new();
                MessageType::ChannelData.encode(&mut packet);
                packet.extend_from_slice(&channel.remote_id.to_be_bytes());
                put_string(&mut packet, &input[input_pos..input_pos + limit]);
                transport.encode_raw(&packet)?;
                channel.remote_window -= limit as u32;
                input_pos += limit;
            }
            if input_eof && input_pos >= input_len && !eof_sent {
                let mut packet = Vec::new();
                MessageType::ChannelEof.encode(&mut packet);
                packet.extend_from_slice(&channel.remote_id.to_be_bytes());
                transport.encode_raw(&packet)?;
                eof_sent = true;
            }
            transport.flush_nonblocking()?;
        }
    }

    transport.set_nonblocking(false)?;
    Ok(ExecResult {
        status: status_from(exit_status, exit_signal),
        stdout: Vec::new(),
        stderr: Vec::new(),
        certainty: Certainty::Submitted,
    })
}

fn process_packet(
    packet: crate::wire::Packet,
    transport: &mut Transport,
    channel: &mut Channel,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    exit_status: &mut Option<u32>,
    exit_signal: &mut Option<String>,
    closed: &mut bool,
) -> Result<(), Error> {
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
            stdout.write_all(data).map_err(|error| Error::Submitted {
                detail: format!("stdout write failed: {error}"),
            })?;
            queue_window_adjust(transport, channel, data.len())?;
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
                stderr.write_all(data).map_err(|error| Error::Submitted {
                    detail: format!("stderr write failed: {error}"),
                })?;
            }
            queue_window_adjust(transport, channel, data.len())?;
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
                "exit-status" => *exit_status = Some(reader.u32()?),
                "exit-signal" => *exit_signal = Some(reader.text()?.to_owned()),
                _ => {}
            }
            if want_reply {
                let mut reply = Vec::new();
                MessageType::ChannelFailure.encode(&mut reply);
                reply.extend_from_slice(&channel.remote_id.to_be_bytes());
                transport.encode_raw(&reply)?;
            }
        }
        MessageType::ChannelEof => {}
        MessageType::ChannelClose => {
            let mut reply = Vec::new();
            MessageType::ChannelClose.encode(&mut reply);
            reply.extend_from_slice(&channel.remote_id.to_be_bytes());
            transport.encode_raw(&reply)?;
            *closed = true;
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
    Ok(())
}

fn queue_window_adjust(
    transport: &mut Transport,
    channel: &Channel,
    bytes: usize,
) -> Result<(), Error> {
    if bytes == 0 {
        return Ok(());
    }
    let mut packet = Vec::new();
    MessageType::ChannelWindowAdjust.encode(&mut packet);
    packet.extend_from_slice(&channel.remote_id.to_be_bytes());
    packet.extend_from_slice(&(bytes as u32).to_be_bytes());
    transport.encode_raw(&packet)
}

fn status_from(exit_status: Option<u32>, exit_signal: Option<String>) -> ExecStatus {
    match (exit_status, exit_signal) {
        (Some(status), _) => ExecStatus::Exited(status),
        (None, Some(signal)) => ExecStatus::Signaled(signal),
        (None, None) => ExecStatus::Missing,
    }
}
