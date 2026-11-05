use std::io::{ErrorKind, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

use rustix::event::{PollFd, PollFlags, poll};
use rustix::time::Timespec;

use crate::error::{Certainty, Error};
use crate::wire::{Interrupt, SshReader, put_string};

const REQUEST_IDENTITIES: u8 = 11;
const IDENTITIES_ANSWER: u8 = 12;
const SIGN_REQUEST: u8 = 13;
const SIGN_RESPONSE: u8 = 14;
const MAX_MESSAGE: usize = 256 * 1024;
const MAX_IDENTITIES: u32 = 1024;
const ED25519_SIGNATURE: usize = 64;
const INTERRUPT_POLL: Duration = Duration::from_millis(50);

fn agent_error(detail: impl Into<String>) -> Error {
    Error::Agent {
        detail: detail.into(),
    }
}

/// A connection to a local `ssh-agent` that signs on the client's behalf.
///
/// The agent keeps private keys, including encrypted ones, out of this process.
/// The client only asks it to sign the authentication request for one identity
/// chosen by fingerprint, and never forwards the agent to the server.
pub struct Agent {
    stream: UnixStream,
    idle: Duration,
    deadline: Option<Instant>,
    interrupt: Option<Interrupt>,
}

impl Agent {
    /// Connect to the agent socket; the agent may stay silent for at most `timeout` at a time.
    pub fn connect(socket: &Path, timeout: Duration) -> Result<Self, Error> {
        let stream = UnixStream::connect(socket)
            .map_err(|error| agent_error(format!("cannot connect to {}: {error}", socket.display())))?;
        stream
            .set_nonblocking(true)
            .map_err(|error| agent_error(format!("cannot bound agent I/O: {error}")))?;
        Ok(Self {
            stream,
            idle: timeout,
            deadline: None,
            interrupt: None,
        })
    }

    /// End every exchange at `deadline` however steadily the agent answers, and as soon as
    /// `interrupt` reports that the caller wants the operation abandoned.
    pub fn bounded(mut self, deadline: Option<Instant>, interrupt: Option<Interrupt>) -> Self {
        self.deadline = deadline;
        self.interrupt = interrupt;
        self
    }

    fn wait(&self, events: PollFlags, last_progress: Instant) -> Result<(), Error> {
        loop {
            if self.interrupt.as_ref().is_some_and(|interrupt| interrupt()) {
                return Err(Error::Cancelled {
                    certainty: Certainty::NotSubmitted,
                });
            }
            let now = Instant::now();
            if self.deadline.is_some_and(|deadline| now >= deadline) {
                return Err(Error::Timeout {
                    certainty: Certainty::NotSubmitted,
                });
            }
            let silent = now.duration_since(last_progress);
            if silent >= self.idle {
                return Err(agent_error(format!("agent did not answer within {:?}", self.idle)));
            }
            let mut slice = INTERRUPT_POLL.min(self.idle - silent);
            if let Some(deadline) = self.deadline {
                slice = slice.min(deadline.saturating_duration_since(now));
            }
            let mut descriptors = [PollFd::new(&self.stream, events)];
            let timespec = Timespec {
                tv_sec: slice.as_secs() as _,
                tv_nsec: slice.subsec_nanos() as _,
            };
            match poll(&mut descriptors, Some(&timespec)) {
                Ok(0) => {}
                Ok(_) => return Ok(()),
                Err(error) if error == rustix::io::Errno::INTR => {}
                Err(error) => return Err(agent_error(format!("agent wait failed: {error}"))),
            }
        }
    }

    fn write_all(&mut self, mut bytes: &[u8]) -> Result<(), Error> {
        let mut progress = Instant::now();
        while !bytes.is_empty() {
            match self.stream.write(bytes) {
                Ok(0) => return Err(agent_error("agent write failed: connection closed")),
                Ok(written) => {
                    bytes = &bytes[written..];
                    progress = Instant::now();
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    self.wait(PollFlags::OUT, progress)?;
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) => return Err(agent_error(format!("agent write failed: {error}"))),
            }
        }
        Ok(())
    }

    fn read_exact(&mut self, mut buffer: &mut [u8]) -> Result<(), Error> {
        let mut progress = Instant::now();
        while !buffer.is_empty() {
            match self.stream.read(buffer) {
                Ok(0) => return Err(agent_error("agent read failed: connection closed")),
                Ok(read) => {
                    buffer = &mut buffer[read..];
                    progress = Instant::now();
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    self.wait(PollFlags::IN, progress)?;
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) => return Err(agent_error(format!("agent read failed: {error}"))),
            }
        }
        Ok(())
    }

    fn exchange(&mut self, request: &[u8]) -> Result<Vec<u8>, Error> {
        let mut framed = (request.len() as u32).to_be_bytes().to_vec();
        framed.extend_from_slice(request);
        self.write_all(&framed)?;
        let mut length = [0u8; 4];
        self.read_exact(&mut length)?;
        let length = u32::from_be_bytes(length) as usize;
        if length == 0 || length > MAX_MESSAGE {
            return Err(agent_error(format!("agent reply of {length} bytes is out of bounds")));
        }
        let mut reply = vec![0u8; length];
        self.read_exact(&mut reply)?;
        Ok(reply)
    }

    /// The public key blobs of every identity the agent holds.
    pub fn identities(&mut self) -> Result<Vec<Vec<u8>>, Error> {
        let reply = self.exchange(&[REQUEST_IDENTITIES])?;
        if reply[0] != IDENTITIES_ANSWER {
            return Err(agent_error("agent refused to list identities"));
        }
        let mut reader = SshReader::new(&reply[1..]);
        let count = reader.u32()?;
        if count > MAX_IDENTITIES {
            return Err(agent_error(format!("agent lists {count} identities")));
        }
        let mut blobs = Vec::new();
        for _ in 0..count {
            blobs.push(reader.string()?.to_vec());
            let _comment = reader.string()?;
        }
        reader.finish()?;
        Ok(blobs)
    }

    /// Sign `data` with the identity whose public key blob is `blob`, returning the SSH signature blob.
    pub fn sign(&mut self, blob: &[u8], data: &[u8]) -> Result<Vec<u8>, Error> {
        let mut request = vec![SIGN_REQUEST];
        put_string(&mut request, blob);
        put_string(&mut request, data);
        request.extend_from_slice(&0u32.to_be_bytes());
        let reply = self.exchange(&request)?;
        if reply[0] != SIGN_RESPONSE {
            return Err(agent_error("agent refused to sign"));
        }
        let mut reader = SshReader::new(&reply[1..]);
        let signature = reader.string()?.to_vec();
        reader.finish()?;
        let mut inner = SshReader::new(&signature);
        let algorithm = inner.string()?;
        let raw = inner.string()?;
        inner.finish()?;
        if algorithm != b"ssh-ed25519" || raw.len() != ED25519_SIGNATURE {
            return Err(agent_error("agent returned a malformed Ed25519 signature"));
        }
        Ok(signature)
    }
}
