use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsFd, BorrowedFd};
use std::time::{Duration, Instant};

use oxish_proto::{Encode, MessageType};
use rustix::event::{PollFd, PollFlags, poll};
use rustix::time::Timespec;
use zeroize::{Zeroize, Zeroizing};

use crate::channel::{Channel, validate_environment};
use crate::client::Connection;
use crate::error::{Certainty, Error};
use crate::exec::{ExecResult, ExecStatus};
use crate::wire::{Interrupt, Packet, SshReader, plain_text};

const OUTPUT_HIGH_WATER: usize = 256 * 1024;
const STDERR_EXTENDED_DATA: u32 = 1;

/// Options for one streamed exec.
#[derive(Clone, Default)]
pub struct StreamOptions {
    /// The whole-command deadline; without one only the interrupt ends a silent command.
    pub timeout: Option<Duration>,
    /// Reports that the caller wants the command abandoned.
    pub interrupt: Option<Interrupt>,
    /// Variables to ask the server to set, by SSH `env` request.
    pub env: Vec<(String, String)>,
}

impl std::fmt::Debug for StreamOptions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StreamOptions")
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

/// A source of command input. The client waits on its descriptor, so a source that can block
/// without one would stall the connection, its deadline and its cancellation.
pub trait InputSource: Read {
    /// A descriptor that polls readable when the next read will not block, or `None` for a
    /// source whose reads never block.
    fn descriptor(&self) -> Option<BorrowedFd<'_>>;
}

impl InputSource for File {
    fn descriptor(&self) -> Option<BorrowedFd<'_>> {
        Some(self.as_fd())
    }
}

/// A destination for command output. While a sink with a descriptor is not writable the client
/// holds its bytes and keeps serving the connection; a sink without one is written at once.
pub(crate) trait OutputSink: Write {
    /// A descriptor that polls writable when a write of up to `PIPE_BUF` bytes will not block, or
    /// `None` for a sink whose writes never block.
    fn descriptor(&self) -> Option<BorrowedFd<'_>>;
}

/// A caller's writer, which the client cannot wait on.
pub(crate) struct Unpolled<'a>(pub(crate) &'a mut dyn Write);

impl Write for Unpolled<'_> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.0.write(data)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

impl OutputSink for Unpolled<'_> {
    fn descriptor(&self) -> Option<BorrowedFd<'_>> {
        None
    }
}

const PIPE_ATOMIC_WRITE: usize = 4096;
const COMPACT_AFTER: usize = 64 * 1024;
const DRAIN_POLL: Duration = Duration::from_millis(50);

fn ready(descriptor: BorrowedFd<'_>, events: PollFlags, wait: Duration) -> Result<bool, Error> {
    let mut polled = [PollFd::from_borrowed_fd(descriptor, events)];
    let timespec = Timespec {
        tv_sec: wait.as_secs() as _,
        tv_nsec: wait.subsec_nanos() as _,
    };
    match poll(&mut polled, Some(&timespec)) {
        Ok(0) => Ok(false),
        Ok(_) => Ok(true),
        Err(error) if error == rustix::io::Errno::INTR => Ok(false),
        Err(error) => Err(Error::Submitted {
            detail: format!("polling an output file failed: {error}"),
        }),
    }
}

/// One output stream: bytes the sink has not taken yet stay here, and their window stays with
/// the client, so at most the receive window is ever held.
struct Drain<'a> {
    sink: &'a mut dyn OutputSink,
    stream: &'static str,
    held: Zeroizing<Vec<u8>>,
    taken: usize,
}

impl<'a> Drain<'a> {
    fn new(sink: &'a mut dyn OutputSink, stream: &'static str) -> Self {
        Self {
            sink,
            stream,
            held: Zeroizing::new(Vec::new()),
            taken: 0,
        }
    }

    fn waiting(&self) -> bool {
        self.taken < self.held.len()
    }

    fn descriptor(&self) -> Option<BorrowedFd<'_>> {
        if self.waiting() {
            self.sink.descriptor()
        } else {
            None
        }
    }

    /// Hand `data` to the sink and return how many held bytes left the client.
    fn accept(&mut self, data: &[u8]) -> Result<usize, Error> {
        if self.sink.descriptor().is_none() {
            sink(self.sink, data, self.stream)?;
            return Ok(data.len());
        }
        self.held.extend_from_slice(data);
        self.pump()
    }

    /// Write held bytes while the sink is writable and return how many left the client.
    fn pump(&mut self) -> Result<usize, Error> {
        let mut written = 0;
        while self.waiting() {
            let Some(descriptor) = self.sink.descriptor() else {
                break;
            };
            if !ready(descriptor, PollFlags::OUT, Duration::ZERO)? {
                break;
            }
            let end = self.held.len().min(self.taken + PIPE_ATOMIC_WRITE);
            match self.sink.write(&self.held[self.taken..end]) {
                Ok(0) => {
                    return Err(Error::Submitted {
                        detail: format!("{} write accepted no bytes", self.stream),
                    });
                }
                Ok(count) => {
                    self.taken += count;
                    written += count;
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(sink_error(error, self.stream)),
            }
        }
        if !self.waiting() || self.taken >= COMPACT_AFTER {
            let kept = self.held.len() - self.taken;
            self.held.copy_within(self.taken.., 0);
            self.held[kept..].zeroize();
            self.held.truncate(kept);
            self.taken = 0;
        }
        Ok(written)
    }
}

/// Write what the outputs still hold after the connection ended, bounded by the operation's
/// deadline and interrupt.
fn drain_after_disconnect(connection: &mut Connection, outputs: &mut [Drain<'_>; 2]) -> Result<(), Error> {
    let deadline = connection.transport().deadline();
    let interrupt = connection.transport().interrupt();
    loop {
        for output in outputs.iter_mut() {
            output.pump()?;
        }
        let Some(descriptor) = outputs.iter().find_map(Drain::descriptor) else {
            return Ok(());
        };
        if interrupt.as_ref().is_some_and(|interrupt| interrupt()) {
            return Err(Error::Cancelled {
                certainty: Certainty::Submitted,
            });
        }
        let mut slice = DRAIN_POLL;
        if let Some(deadline) = deadline {
            let now = Instant::now();
            if now >= deadline {
                return Err(Error::Timeout {
                    certainty: Certainty::Submitted,
                });
            }
            slice = slice.min(deadline - now);
        }
        ready(descriptor, PollFlags::OUT, slice)?;
    }
}

struct Input<'a> {
    reader: Option<&'a mut dyn InputSource>,
    buffer: Zeroizing<Vec<u8>>,
    position: usize,
    length: usize,
    eof: bool,
}

impl<'a> Input<'a> {
    fn new(reader: Option<&'a mut dyn InputSource>) -> Self {
        let eof = reader.is_none();
        Self {
            reader,
            buffer: Zeroizing::new(vec![0; 64 * 1024]),
            position: 0,
            length: 0,
            eof,
        }
    }

    fn drained(&self) -> bool {
        self.position == self.length
    }

    fn descriptor(&self) -> Option<BorrowedFd<'_>> {
        self.reader.as_ref().and_then(|reader| reader.descriptor())
    }

    fn readable(&self) -> bool {
        let Some(descriptor) = self.descriptor() else {
            return true;
        };
        let mut polled = [PollFd::from_borrowed_fd(descriptor, PollFlags::IN)];
        let immediately = Timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        !matches!(poll(&mut polled, Some(&immediately)), Ok(0) | Err(_))
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
    fn is_reported(&self) -> bool {
        self.exit_status.is_some() || self.exit_signal.is_some()
    }

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
    stdin: Option<&mut dyn InputSource>,
    stdout: &mut dyn OutputSink,
    stderr: &mut dyn OutputSink,
) -> Result<ExecResult, Error> {
    connection.require_usable()?;
    validate_environment(&options.env)?;
    connection
        .transport()
        .begin_operation(options.timeout, options.interrupt.clone());
    let result = run(connection, command, &options.env, stdin, stdout, stderr);
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
    env: &[(String, String)],
    stdin: Option<&mut dyn InputSource>,
    stdout: &mut dyn OutputSink,
    stderr: &mut dyn OutputSink,
) -> Result<ExecResult, Error> {
    let mut channel = Channel::open(connection)?;
    for (name, value) in env {
        channel.set_environment(connection, name, value)?;
    }
    channel.exec(connection, command.as_bytes())?;
    connection.transport().await_command();

    let mut input = Input::new(stdin);
    let mut completion = Completion::default();
    let mut outputs = [Drain::new(stdout, "stdout"), Drain::new(stderr, "stderr")];
    let driven = drive(connection, &mut channel, &mut completion, &mut input, &mut outputs);
    match driven {
        Ok(()) => {}
        Err(_) if connection.transport().peer_closed() && completion.is_reported() => {
            connection.mark_unusable();
            drain_after_disconnect(connection, &mut outputs)?;
        }
        Err(error) => return Err(error),
    }
    Ok(ExecResult {
        status: completion.status(),
        stdout: Vec::new(),
        stderr: Vec::new(),
    })
}

fn drive(
    connection: &mut Connection,
    channel: &mut Channel,
    completion: &mut Completion,
    input: &mut Input<'_>,
    outputs: &mut [Drain<'_>; 2],
) -> Result<(), Error> {
    loop {
        connection.transport().check()?;
        while let Some(packet) = connection.try_next_packet()? {
            on_packet(connection, channel, completion, packet, outputs)?;
        }
        for output in outputs.iter_mut() {
            let written = output.pump()?;
            channel.release(written);
        }
        channel.replenish(connection.transport())?;
        let feeding = feed(connection, channel, input)?;
        if channel.close_received {
            channel.send_close(connection.transport())?;
        }
        let flushed = connection.transport().flush_nonblocking()?;
        let holding = outputs.iter().any(Drain::waiting);
        if channel.is_closed() && flushed && !holding {
            break;
        }
        if connection.rekey_due() {
            connection.rekey(None)?;
            continue;
        }
        if flushed && feeding == Feed::OutputFull {
            continue;
        }
        let waiting_for = if feeding == Feed::WaitingForInput {
            input.descriptor()
        } else {
            None
        };
        let [stdout, stderr] = &*outputs;
        let events = connection.transport().wait_with_files(
            !flushed,
            waiting_for,
            [stdout.descriptor(), stderr.descriptor()],
        )?;
        if events.contains(PollFlags::IN) || events.contains(PollFlags::HUP) {
            connection.transport().fill_nonblocking()?;
        }
    }

    Ok(())
}

#[derive(PartialEq, Eq)]
enum Feed {
    Done,
    WaitingForWindow,
    WaitingForInput,
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
            if !input.readable() {
                return Ok(Feed::WaitingForInput);
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
    outputs: &mut [Drain<'_>; 2],
) -> Result<(), Error> {
    let [stdout, stderr] = outputs;
    match packet.message_type {
        MessageType::ChannelData => {
            let mut reader = SshReader::new(&packet.payload);
            let recipient = reader.u32()?;
            let data = reader.string()?;
            reader.finish()?;
            channel.expect_recipient(recipient)?;
            check_open_for_data(channel)?;
            channel.receive(data.len())?;
            let delivered = stdout.accept(data)?;
            channel.release(delivered);
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
            let delivered = if data_type == STDERR_EXTENDED_DATA {
                stderr.accept(data)?
            } else {
                data.len()
            };
            channel.release(delivered);
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
                    let signal = plain_text(reader.text()?);
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
                connection.transport().reply(&reply)?;
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

fn sink_error(error: std::io::Error, stream: &'static str) -> Error {
    if error.kind() == std::io::ErrorKind::OutOfMemory {
        Error::OutputLimit { stream }
    } else {
        Error::Submitted {
            detail: format!("{stream} write failed: {error}"),
        }
    }
}

fn sink(output: &mut dyn Write, data: &[u8], stream: &'static str) -> Result<(), Error> {
    output.write_all(data).map_err(|error| sink_error(error, stream))
}
