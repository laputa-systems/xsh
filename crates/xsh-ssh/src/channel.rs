use oxish_proto::{Encode, MessageType};

use crate::error::Error;
use crate::wire::{SshReader, Transport, put_string};

/// The local receive window advertised to the server.
pub const LOCAL_WINDOW: u32 = 2 * 1024 * 1024;
/// The largest packet the local side accepts.
pub const LOCAL_MAX_PACKET: u32 = 32 * 1024;

/// Decline an optional global request without interpreting its name.
pub(crate) fn handle_global_request(
    transport: &mut Transport,
    payload: &[u8],
) -> Result<(), Error> {
    let mut reader = SshReader::new(payload);
    let _name = reader.text()?;
    let want_reply = reader.u8()? != 0;
    if want_reply {
        let mut reply = Vec::new();
        MessageType::RequestFailure.encode(&mut reply);
        transport.send_raw(&reply)?;
    }
    Ok(())
}

/// An open session channel.
#[derive(Debug)]
pub struct Channel {
    /// The local channel identifier.
    pub local_id: u32,
    /// The peer's channel identifier.
    pub remote_id: u32,
    /// Bytes the peer will still accept.
    pub remote_window: u32,
    /// Largest data packet the peer accepts.
    pub remote_max_packet: u32,
}

impl Channel {
    /// Open a `session` channel and wait for confirmation.
    pub fn open(transport: &mut Transport) -> Result<Self, Error> {
        let local_id: u32 = 0;
        let mut request = Vec::new();
        MessageType::ChannelOpen.encode(&mut request);
        put_string(&mut request, b"session");
        request.extend_from_slice(&local_id.to_be_bytes());
        request.extend_from_slice(&LOCAL_WINDOW.to_be_bytes());
        request.extend_from_slice(&LOCAL_MAX_PACKET.to_be_bytes());
        transport.send_raw(&request)?;

        loop {
            let packet = transport.recv()?;
            match packet.message_type {
                MessageType::ChannelOpenConfirmation => {
                    let mut reader = SshReader::new(&packet.payload);
                    let recipient = reader.u32()?;
                    let sender = reader.u32()?;
                    let window = reader.u32()?;
                    let maximum = reader.u32()?;
                    reader.finish()?;
                    if recipient != local_id {
                        return Err(Error::Channel {
                            detail: format!("confirmation used channel {recipient}"),
                        });
                    }
                    return Ok(Self {
                        local_id,
                        remote_id: sender,
                        remote_window: window,
                        remote_max_packet: maximum,
                    });
                }
                MessageType::ChannelOpenFailure => {
                    let mut reader = SshReader::new(&packet.payload);
                    let _recipient = reader.u32()?;
                    let reason = reader.u32()?;
                    let description = reader.text()?.to_owned();
                    return Err(Error::Channel {
                        detail: format!("channel open failed ({reason}): {description}"),
                    });
                }
                MessageType::GlobalRequest => {
                    handle_global_request(transport, &packet.payload)?;
                    continue;
                }
                MessageType::Ignore | MessageType::Debug | MessageType::Unimplemented => continue,
                MessageType::Disconnect => {
                    return Err(Error::Channel {
                        detail: "server disconnected while opening a channel".to_owned(),
                    });
                }
                other => {
                    return Err(Error::Protocol {
                        detail: format!("unexpected channel-open message {other:?}"),
                    });
                }
            }
        }
    }

    /// Send an `exec` request and wait for its reply.
    pub fn exec(&mut self, transport: &mut Transport, command: &[u8]) -> Result<(), Error> {
        let mut request = Vec::new();
        MessageType::ChannelRequest.encode(&mut request);
        request.extend_from_slice(&self.remote_id.to_be_bytes());
        put_string(&mut request, b"exec");
        request.push(1);
        put_string(&mut request, command);
        transport.mark_submitted();
        transport.send_raw(&request)?;

        loop {
            let packet = transport.recv()?;
            match packet.message_type {
                MessageType::ChannelSuccess => return Ok(()),
                MessageType::ChannelFailure => {
                    return Err(Error::Channel {
                        detail: "server rejected the exec request".to_owned(),
                    });
                }
                MessageType::ChannelWindowAdjust => {
                    let mut reader = SshReader::new(&packet.payload);
                    let _recipient = reader.u32()?;
                    let added = reader.u32()?;
                    reader.finish()?;
                    self.remote_window = self.remote_window.saturating_add(added);
                }
                MessageType::GlobalRequest => {
                    handle_global_request(transport, &packet.payload)?;
                    continue;
                }
                MessageType::Ignore | MessageType::Debug | MessageType::Unimplemented => continue,
                MessageType::Disconnect => {
                    return Err(Error::Submitted {
                        detail: "server disconnected after the exec request".to_owned(),
                    });
                }
                other => {
                    return Err(Error::Protocol {
                        detail: format!("unexpected exec reply {other:?}"),
                    });
                }
            }
        }
    }

    /// Send data on the channel, respecting the peer's window and packet size.
    pub fn send_data(&mut self, transport: &mut Transport, mut data: &[u8]) -> Result<(), Error> {
        while !data.is_empty() {
            if self.remote_window == 0 {
                return Err(Error::Channel {
                    detail: "peer receive window is exhausted".to_owned(),
                });
            }
            let limit = usize::try_from(self.remote_window.min(self.remote_max_packet))
                .unwrap_or(usize::MAX)
                .min(data.len());
            let (chunk, rest) = data.split_at(limit);
            let mut packet = Vec::new();
            MessageType::ChannelData.encode(&mut packet);
            packet.extend_from_slice(&self.remote_id.to_be_bytes());
            put_string(&mut packet, chunk);
            transport.send_raw(&packet)?;
            self.remote_window -= limit as u32;
            data = rest;
        }
        Ok(())
    }

    /// Send EOF on the channel.
    pub fn send_eof(&self, transport: &mut Transport) -> Result<(), Error> {
        let mut packet = Vec::new();
        MessageType::ChannelEof.encode(&mut packet);
        packet.extend_from_slice(&self.remote_id.to_be_bytes());
        transport.send_raw(&packet)
    }

    /// Send close on the channel.
    pub fn send_close(&self, transport: &mut Transport) -> Result<(), Error> {
        let mut packet = Vec::new();
        MessageType::ChannelClose.encode(&mut packet);
        packet.extend_from_slice(&self.remote_id.to_be_bytes());
        transport.send_raw(&packet)
    }

    /// Extend the peer's send window after receiving data.
    pub fn acknowledge(
        &self,
        transport: &mut Transport,
        bytes: usize,
    ) -> Result<(), Error> {
        if bytes == 0 {
            return Ok(());
        }
        let mut packet = Vec::new();
        MessageType::ChannelWindowAdjust.encode(&mut packet);
        packet.extend_from_slice(&self.remote_id.to_be_bytes());
        packet.extend_from_slice(&(bytes as u32).to_be_bytes());
        transport.send_raw(&packet)
    }
}
