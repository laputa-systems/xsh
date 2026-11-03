use std::io::{Read, Write};
use std::time::Duration;

use oxish_proto::{Encode, MessageType};
use rustix::event::PollFlags;

use crate::channel::Channel;
use crate::client::Connection;
use crate::error::{Certainty, Error};
use crate::exec::{ExecResult, ExecStatus};
use crate::wire::{Interrupt, Packet, SshReader, put_string};

const OUTPUT_HIGH_WATER: usize = 256 * 1024;
const STDERR_EXTENDED_DATA: u32 = 1;

/// Options for one streamed exec.
#[derive(Clone, Default)]
pub struct StreamOptions {
    /// The whole-command deadline; without one only the interrupt ends a silent command.
    pub timeout: Option<Duration>,
    /// Reports that the caller wants the command abandoned.
    pub interrupt: Option<Interrupt>,
}

impl std::fmt::Debug for StreamOptions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StreamOptions")
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

struct Input<'a> {
    reader: Option<&'a mut dyn Read>,
    buffer: Vec<u8>,
    position: usize,
    length: usize,
    eof: bool,
}

impl<'a> Input<'a> {
    fn new(reader: Option<&'a mut dyn Read>) -> Self {
        let eof = reader.is_none();
        Self {
            reader,
            buffer: vec![0; 64 * 1024],
            position: 0,
            length: 0,
            eof,
        }
    }

    fn drained(&self) -> bool {
        self.position == self.length
    }

    fn refill(&mut self) -> Result<(), Error> {
        let Some(reader) = self.reader.as_mut() else {
            self.eof = true;
            return Ok(());
        };
        loop {
            match reader.read(&mut self.buffer) {
                Ok(0) => {
                    self.eof = true;
                    return Ok(());
                }
                Ok(read) => {
                    self.length = read;
                    self.position = 0;
                    return Ok(());
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    return Err(Error::Submitted {
                        detail: format!("stdin read failed: {error}"),
                    });
                }
            }
        }
    }
}

#[derive(Default)]
struct Completion {
    exit_status: Option<u32>,
    exit_signal: Option<String>,
}

impl Completion {
    fn status(&self) -> ExecStatus {
        match (self.exit_status, &self.exit_signal) {
            (Some(status), _) => ExecStatus::Exited(status),
            (None, Some(signal)) => ExecStatus::Signaled(signal.clone()),
            (None, None) => ExecStatus::Missing,
        }
    }
}

pub(crate) fn exec_stream(
    connection: &mut Connection,
    command: &str,
    options: &StreamOptions,
    stdin: Option<&mut dyn Read>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<ExecResult, Error> {
    connection.require_usable()?;
    connection
        .transport()
        .begin_operation(options.timeout, options.interrupt.clone());
    let result = run(connection, command, stdin, stdout, stderr);
    connection.transport().end_operation();
    result.map_err(|error| {
        let error = if connection.transport().certainty() == Certainty::Submitted {
            error.after_submission()
        } else {
            error
        };
        connection.mark_unusable();
        error
    })
}

fn run(
    connection: &mut Connection,
    command: &str,
    stdin: Option<&mut dyn Read>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<ExecResult, Error> {
    let mut channel = Channel::open(connection)?;
    channel.exec(connection, command.as_bytes())?;
    connection.transport().await_command();

    let mut input = Input::new(stdin);
    let mut completion = Completion::default();
    while !channel.is_closed() {
        connection.transport().check()?;
        while let Some(packet) = connection.try_next_packet()? {
            on_packet(connection, &mut channel, &mut completion, packet, stdout, stderr)?;
        }
        let feeding = feed(connection, &mut channel, &mut input)?;
        if channel.close_received {
            channel.send_close(connection.transport())?;
        }
        let flushed = connection.transport().flush_nonblocking()?;
        if channel.is_closed() && flushed {
            break;
        }
        if connection.rekey_due() {
            connection.rekey(None)?;
            continue;
        }
        if flushed && feeding == Feed::OutputFull {
            continue;
        }
        let events = connection.transport().wait(!flushed)?;
        if events.contains(PollFlags::IN) || events.contains(PollFlags::HUP) {
            connection.transport().fill_nonblocking()?;
        }
    }

    Ok(ExecResult {
        status: completion.status(),
        stdout: Vec::new(),
        stderr: Vec::new(),
        certainty: Certainty::Submitted,
    })
}

#[derive(PartialEq, Eq)]
enum Feed {
    Done,
    WaitingForWindow,
    OutputFull,
}

fn feed(
    connection: &mut Connection,
    channel: &mut Channel,
    input: &mut Input<'_>,
) -> Result<Feed, Error> {
    if channel.eof_sent || channel.close_received {
        return Ok(Feed::Done);
    }
    loop {
        if connection.transport().pending_output() >= OUTPUT_HIGH_WATER {
            return Ok(Feed::OutputFull);
        }
        if input.drained() {
            if input.eof {
                break;
            }
            if channel.remote_window == 0 {
                return Ok(Feed::WaitingForWindow);
            }
            input.refill()?;
            continue;
        }
        if channel.remote_window == 0 {
            return Ok(Feed::WaitingForWindow);
        }
        let sent = channel.send_data(
            connection.transport(),
            &input.buffer[input.position..input.length],
        )?;
        input.position += sent;
    }
    channel.send_eof(connection.transport())?;
    Ok(Feed::Done)
}

fn on_packet(
    connection: &mut Connection,
    channel: &mut Channel,
    completion: &mut Completion,
    packet: Packet,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<(), Error> {
    match packet.message_type {
        MessageType::ChannelData => {
            let mut reader = SshReader::new(&packet.payload);
            let recipient = reader.u32()?;
            let data = reader.string()?;
            reader.finish()?;
            channel.expect_recipient(recipient)?;
            check_open_for_data(channel)?;
            channel.receive(data.len())?;
            sink(stdout, data, "stdout")?;
            channel.replenish(connection.transport())?;
        }
        MessageType::ChannelExtendedData => {
            let mut reader = SshReader::new(&packet.payload);
            let recipient = reader.u32()?;
            let data_type = reader.u32()?;
            let data = reader.string()?;
            reader.finish()?;
            channel.expect_recipient(recipient)?;
            check_open_for_data(channel)?;
            channel.receive(data.len())?;
            if data_type == STDERR_EXTENDED_DATA {
                sink(stderr, data, "stderr")?;
            }
            channel.replenish(connection.transport())?;
        }
        MessageType::ChannelWindowAdjust => channel.window_adjust(&packet.payload)?,
        MessageType::ChannelRequest => {
            let mut reader = SshReader::new(&packet.payload);
            let recipient = reader.u32()?;
            channel.expect_recipient(recipient)?;
            let request = reader.text()?.to_owned();
            let want_reply = reader.u8()? != 0;
            match request.as_str() {
                "exit-status" => {
                    let status = reader.u32()?;
                    reader.finish()?;
                    record_completion(completion, Some(status), None)?;
                }
                "exit-signal" => {
                    let signal = reader.text()?.to_owned();
                    let _core_dumped = reader.u8()?;
                    let _message = reader.string()?;
                    let _language = reader.string()?;
                    reader.finish()?;
                    record_completion(completion, None, Some(signal))?;
                }
                _ => {}
            }
            if want_reply {
                let mut reply = Vec::new();
                MessageType::ChannelFailure.encode(&mut reply);
                reply.extend_from_slice(&channel.remote_id.to_be_bytes());
                connection.transport().encode_raw(&reply)?;
            }
        }
        MessageType::ChannelEof => {
            expect_recipient_only(channel, &packet.payload)?;
            channel.eof_received = true;
        }
        MessageType::ChannelClose => {
            expect_recipient_only(channel, &packet.payload)?;
            channel.close_received = true;
        }
        MessageType::ChannelOpen => {
            let mut refusal = Vec::new();
            MessageType::ChannelOpenFailure.encode(&mut refusal);
            let mut reader = SshReader::new(&packet.payload);
            let _kind = reader.text()?;
            let sender = reader.u32()?;
            refusal.extend_from_slice(&sender.to_be_bytes());
            refusal.extend_from_slice(&1u32.to_be_bytes());
            put_string(&mut refusal, b"channels are not accepted");
            put_string(&mut refusal, b"");
            connection.transport().encode_raw(&refusal)?;
        }
        other => {
            return Err(Error::Protocol {
                detail: format!("unexpected channel message {other:?}"),
            });
        }
    }
    Ok(())
}

fn record_completion(
    completion: &mut Completion,
    status: Option<u32>,
    signal: Option<String>,
) -> Result<(), Error> {
    if completion.exit_status.is_some() || completion.exit_signal.is_some() {
        return Err(Error::Protocol {
            detail: "server reported the command's completion twice".to_owned(),
        });
    }
    completion.exit_status = status;
    completion.exit_signal = signal;
    Ok(())
}

fn expect_recipient_only(channel: &Channel, payload: &[u8]) -> Result<(), Error> {
    let mut reader = SshReader::new(payload);
    let recipient = reader.u32()?;
    reader.finish()?;
    channel.expect_recipient(recipient)
}

fn check_open_for_data(channel: &Channel) -> Result<(), Error> {
    if channel.eof_received || channel.close_received {
        return Err(Error::Protocol {
            detail: "data after the peer ended its output".to_owned(),
        });
    }
    Ok(())
}

fn sink(output: &mut dyn Write, data: &[u8], stream: &'static str) -> Result<(), Error> {
    output.write_all(data).map_err(|error| {
        if error.kind() == std::io::ErrorKind::OutOfMemory {
            Error::OutputLimit { stream }
        } else {
            Error::Submitted {
                detail: format!("{stream} write failed: {error}"),
            }
        }
    })
}
