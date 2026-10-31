use std::fmt;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use rustix::event::{PollFd, PollFlags, poll};
use rustix::time::Timespec;

use oxish_proto::crypto::{CryptoProvider, HandshakeBuffer, KeySourceSide};
use oxish_proto::key_exchange::NewKeys;
use oxish_proto::{
    Completion, Encode, IncomingPacket, MessageType, ProtoError, ReadState, WriteState,
};

use crate::error::{Certainty, Error};

/// Append an SSH `string`.
pub(crate) fn put_string(buffer: &mut Vec<u8>, value: &[u8]) {
    buffer.extend_from_slice(&(value.len() as u32).to_be_bytes());
    buffer.extend_from_slice(value);
}

/// One decrypted SSH packet.
pub struct Packet {
    /// The SSH message type.
    pub message_type: MessageType,
    /// The message body after the type byte.
    pub payload: Vec<u8>,
}

impl fmt::Debug for Packet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Packet")
            .field("message_type", &self.message_type)
            .field("payload_len", &self.payload.len())
            .finish()
    }
}

impl Packet {
    /// The full payload, including the message-type byte.
    pub fn encoded_payload(&self) -> Vec<u8> {
        let mut encoded = Vec::with_capacity(self.payload.len() + 1);
        encoded.push(u8::from(self.message_type));
        encoded.extend_from_slice(&self.payload);
        encoded
    }
}

/// A bounded reader for SSH wire-format values.
#[derive(Debug, Clone, Copy)]
pub struct SshReader<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> SshReader<'a> {
    /// Wrap a packet body.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    /// Read one byte.
    pub fn u8(&mut self) -> Result<u8, Error> {
        let Some(&value) = self.data.get(self.position) else {
            return Err(self.short());
        };
        self.position += 1;
        Ok(value)
    }

    /// Read a big-endian 32-bit integer.
    pub fn u32(&mut self) -> Result<u32, Error> {
        let Some(bytes) = self.data.get(self.position..self.position + 4) else {
            return Err(self.short());
        };
        let mut value = [0u8; 4];
        value.copy_from_slice(bytes);
        self.position += 4;
        Ok(u32::from_be_bytes(value))
    }

    /// Read an SSH `string`.
    pub fn string(&mut self) -> Result<&'a [u8], Error> {
        let length = self.u32()? as usize;
        let Some(bytes) = self.data.get(self.position..self.position + length) else {
            return Err(self.short());
        };
        self.position += length;
        Ok(bytes)
    }

    /// Read an SSH `string` as UTF-8.
    pub fn text(&mut self) -> Result<&'a str, Error> {
        std::str::from_utf8(self.string()?).map_err(|_| Error::Protocol {
            detail: "invalid UTF-8 in string".to_owned(),
        })
    }

    /// Read a comma-separated SSH name-list.
    pub fn name_list(&mut self) -> Result<Vec<String>, Error> {
        let value = self.text()?;
        if value.is_empty() {
            return Ok(Vec::new());
        }
        Ok(value.split(',').map(str::to_owned).collect())
    }

    /// Require that the body has been consumed.
    pub fn finish(&self) -> Result<(), Error> {
        if self.position == self.data.len() {
            Ok(())
        } else {
            Err(Error::Protocol {
                detail: "trailing bytes in SSH message".to_owned(),
            })
        }
    }

    /// The number of bytes not yet consumed.
    pub fn remaining(&self) -> usize {
        self.data.len() - self.position
    }

    fn short(&self) -> Error {
        Error::Protocol {
            detail: "truncated SSH message".to_owned(),
        }
    }
}

struct RawPayload<'a>(&'a [u8]);

impl Encode for RawPayload<'_> {
    fn encode(&self, buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(self.0);
    }
}

impl fmt::Debug for RawPayload<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RawPayload")
            .field("len", &self.0.len())
            .finish()
    }
}

/// A blocking SSH transport before or after authentication.
pub struct Transport {
    stream: TcpStream,
    read: ReadState,
    write: WriteState,
    client_identification: Vec<u8>,
    server_identification: Vec<u8>,
    certainty: Certainty,
    timeout: Duration,
}

impl Transport {
    /// Connect a TCP stream and prepare SSH packet state.
    pub fn connect(
        host: &str,
        port: u16,
        timeout: Duration,
        provider: &dyn CryptoProvider,
    ) -> Result<Self, Error> {
        let addresses = (host, port).to_socket_addrs()?;
        let mut last_error = None;
        let mut stream = None;
        for address in addresses {
            match TcpStream::connect_timeout(&address, timeout) {
                Ok(connected) => {
                    stream = Some(connected);
                    break;
                }
                Err(error) => last_error = Some(error),
            }
        }

        let stream = stream.ok_or_else(|| match last_error {
            Some(error) if error.kind() == std::io::ErrorKind::TimedOut => Error::Timeout {
                certainty: Certainty::NotSubmitted,
            },
            Some(error) => Error::NotSubmitted {
                detail: error.to_string(),
            },
            None => Error::NotSubmitted {
                detail: format!("no address resolved for {host}:{port}"),
            },
        })?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        stream.set_nodelay(true)?;
        Ok(Self {
            stream,
            read: ReadState::default(),
            write: WriteState::new(provider.secure_random()),
            client_identification: Vec::new(),
            server_identification: Vec::new(),
            certainty: Certainty::NotSubmitted,
            timeout,
        })
    }

    /// Record that a mutation request has been submitted.
    pub fn mark_submitted(&mut self) {
        self.certainty = Certainty::Submitted;
    }

    /// Change the read and write deadline.
    pub fn set_timeout(&mut self, timeout: Duration) -> Result<(), Error> {
        self.stream.set_read_timeout(Some(timeout))?;
        self.stream.set_write_timeout(Some(timeout))?;
        self.timeout = timeout;
        Ok(())
    }

    /// The current read and write deadline.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Switch between blocking and nonblocking transport I/O.
    pub fn set_nonblocking(&self, enabled: bool) -> Result<(), Error> {
        self.stream
            .set_nonblocking(enabled)
            .map_err(|error| self.transport_error(error))
    }

    /// Wait for read or write readiness.
    pub fn poll_ready(
        &self,
        write: bool,
        timeout: Option<Duration>,
    ) -> Result<PollFlags, Error> {
        let mut events = PollFlags::IN;
        if write {
            events |= PollFlags::OUT;
        }
        let mut descriptors = [PollFd::new(&self.stream, events)];
        let timespec = timeout.map(|duration| Timespec {
            tv_sec: duration.as_secs() as _,
            tv_nsec: duration.subsec_nanos() as _,
        });
        match poll(&mut descriptors, timespec.as_ref()) {
            Ok(0) => Ok(PollFlags::empty()),
            Ok(_) => Ok(descriptors[0].revents()),
            Err(error) if error == rustix::io::Errno::INTR => Ok(PollFlags::empty()),
            Err(error) => Err(Error::NotSubmitted {
                detail: error.to_string(),
            }),
        }
    }

    /// Read available bytes into the packet buffer.
    pub fn fill_nonblocking(&mut self) -> Result<bool, Error> {
        let mut buffer = [0u8; 16_384];
        match self.stream.read(&mut buffer) {
            Ok(0) => Err(match self.certainty {
                Certainty::NotSubmitted => Error::NotSubmitted {
                    detail: "connection closed".to_owned(),
                },
                Certainty::Submitted => Error::Submitted {
                    detail: "connection closed".to_owned(),
                },
            }),
            Ok(read) => {
                self.read.buf.extend_from_slice(&buffer[..read]);
                Ok(true)
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(false),
            Err(error) => Err(self.transport_error(error)),
        }
    }

    /// Decode one complete buffered packet, if any.
    pub fn try_recv(&mut self) -> Result<Option<Packet>, Error> {
        let polled = match self.read.poll_packet() {
            Ok(completion) => completion,
            Err(ProtoError::Incomplete(_)) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        match polled {
            Completion::Incomplete(_) => Ok(None),
            Completion::Complete((sequence, length)) => {
                let packet: IncomingPacket<'_> = self.read.decode_packet(sequence, length)?;
                let message_type = packet.message_type;
                let payload = packet.payload().to_vec();
                Ok(Some(Packet {
                    message_type,
                    payload,
                }))
            }
        }
    }

    /// Encode a payload without flushing it.
    pub fn encode_raw(&mut self, payload: &[u8]) -> Result<(), Error> {
        self.write.encode(&RawPayload(payload))?;
        Ok(())
    }

    /// Flush as much buffered output as the transport accepts.
    pub fn flush_nonblocking(&mut self) -> Result<bool, Error> {
        loop {
            let buffered = self.write.buffered();
            if buffered.is_empty() {
                return Ok(true);
            }
            match self.stream.write(buffered) {
                Ok(0) => {
                    return Err(Error::Submitted {
                        detail: "write returned zero".to_owned(),
                    });
                }
                Ok(written) => {
                    self.write
                        .written(Ok(written))
                        .map_err(|error| self.transport_error(error))?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    return Ok(false);
                }
                Err(error) => return Err(self.transport_error(error)),
            }
        }
    }

    /// Whether an encoded packet is still waiting to be written.
    pub fn has_pending_output(&self) -> bool {
        !self.write.buffered().is_empty()
    }

    /// Close both directions of the transport.
    pub fn shutdown(&self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }

    /// Exchange and validate the SSH identification lines.
    pub fn exchange_identifications(&mut self, software: &str) -> Result<(), Error> {
        let client = format!("SSH-2.0-{software}");
        self.stream
            .write_all(client.as_bytes())
            .map_err(|error| self.transport_error(error))?;
        self.stream
            .write_all(b"\r\n")
            .map_err(|error| self.transport_error(error))?;
        self.client_identification = client.into_bytes();

        let mut line = Vec::with_capacity(128);
        while !line.ends_with(b"\r\n") {
            let mut byte = [0u8; 1];
            self.stream
                .read_exact(&mut byte)
                .map_err(|error| self.transport_error(error))?;
            line.push(byte[0]);
            if line.len() > 255 + 2 {
                return Err(Error::Protocol {
                    detail: "identification line too long".to_owned(),
                });
            }
        }
        line.truncate(line.len() - 2);
        let text = std::str::from_utf8(&line).map_err(|_| Error::Protocol {
            detail: "identification is not UTF-8".to_owned(),
        })?;
        if !text.starts_with("SSH-2.0-") {
            return Err(Error::Protocol {
                detail: format!("unsupported SSH identification {text:?}"),
            });
        }
        self.server_identification = line;
        Ok(())
    }

    /// The client identification without its CRLF.
    pub fn client_identification(&self) -> &[u8] {
        &self.client_identification
    }

    /// The server identification without its CRLF.
    pub fn server_identification(&self) -> &[u8] {
        &self.server_identification
    }

    /// Start the exchange-hash buffer with both identification strings.
    pub fn handshake_buffer(&self) -> HandshakeBuffer {
        let mut buffer = HandshakeBuffer::default();
        buffer.prefixed(&self.client_identification);
        buffer.prefixed(&self.server_identification);
        buffer
    }

    /// Send one already-framed message payload.
    pub fn send_raw(&mut self, payload: &[u8]) -> Result<(), Error> {
        self.write.encode(&RawPayload(payload))?;
        self.flush()
    }

    /// Send one typed message.
    pub fn send(&mut self, payload: &(impl Encode + fmt::Debug)) -> Result<(), Error> {
        self.write.encode(payload)?;
        self.flush()
    }

    /// Receive one message.
    pub fn recv(&mut self) -> Result<Packet, Error> {
        loop {
            let polled = match self.read.poll_packet() {
                Ok(completion) => completion,
                Err(ProtoError::Incomplete(needed)) => {
                    self.read_more(needed)?;
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            match polled {
                Completion::Complete((sequence, length)) => {
                    let packet: IncomingPacket<'_> = self.read.decode_packet(sequence, length)?;
                    let message_type = packet.message_type;
                    let payload = packet.payload().to_vec();
                    return Ok(Packet {
                        message_type,
                        payload,
                    });
                }
                Completion::Incomplete(needed) => self.read_more(needed)?,
            }
        }
    }

    fn read_more(&mut self, needed: Option<usize>) -> Result<(), Error> {
        let mut buffer = [0u8; 16_384];
        let wanted = needed.unwrap_or(buffer.len()).clamp(1, buffer.len());
        let read = self
            .stream
            .read(&mut buffer[..wanted])
            .map_err(|error| self.transport_error(error))?;
        if read == 0 {
            let detail = "connection closed".to_owned();
            return Err(match self.certainty {
                Certainty::NotSubmitted => Error::NotSubmitted { detail },
                Certainty::Submitted => Error::Submitted { detail },
            });
        }
        self.read.buf.extend_from_slice(&buffer[..read]);
        Ok(())
    }

    /// Send `SSH_MSG_NEWKEYS` and reset the send sequence for strict key exchange.
    pub fn send_newkeys(&mut self, strict: bool) -> Result<(), Error> {
        self.send(&NewKeys)?;
        if strict {
            self.write.sequence_number = 0;
        }
        Ok(())
    }

    /// Receive `SSH_MSG_NEWKEYS` and reset the receive sequence for strict key exchange.
    pub fn recv_newkeys(&mut self, strict: bool) -> Result<(), Error> {
        let packet = self.recv()?;
        if packet.message_type != MessageType::NewKeys {
            return Err(Error::Protocol {
                detail: format!("expected NEWKEYS, got {:?}", packet.message_type),
            });
        }
        if strict {
            self.read.sequence_number = 0;
        }
        Ok(())
    }

    /// Install the client-to-server sealing key.
    pub fn install_sealer(
        &mut self,
        keys: &KeySourceSide,
        provider: &dyn CryptoProvider,
    ) -> Result<(), Error> {
        self.write.sealer = Some(provider.sealing_key(0, keys)?);
        Ok(())
    }

    /// Install the server-to-client opening key.
    pub fn install_opener(
        &mut self,
        keys: &KeySourceSide,
        provider: &dyn CryptoProvider,
    ) -> Result<(), Error> {
        self.read.opener = Some(provider.opening_key(0, keys)?);
        Ok(())
    }

    /// Flush buffered packets to the transport.
    pub fn flush(&mut self) -> Result<(), Error> {
        let buffered = self.write.buffered().to_vec();
        if buffered.is_empty() {
            return Ok(());
        }
        self.stream
            .write_all(&buffered)
            .map_err(|error| self.transport_error(error))?;
        self.write.clear();
        Ok(())
    }

    fn transport_error(&self, error: std::io::Error) -> Error {
        match error.kind() {
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => Error::Timeout {
                certainty: self.certainty,
            },
            _ => match self.certainty {
                Certainty::NotSubmitted => Error::NotSubmitted {
                    detail: error.to_string(),
                },
                Certainty::Submitted => Error::Submitted {
                    detail: error.to_string(),
                },
            },
        }
    }

}
