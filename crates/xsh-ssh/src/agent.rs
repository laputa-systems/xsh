use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use crate::error::Error;
use crate::wire::{SshReader, put_string};

const REQUEST_IDENTITIES: u8 = 11;
const IDENTITIES_ANSWER: u8 = 12;
const SIGN_REQUEST: u8 = 13;
const SIGN_RESPONSE: u8 = 14;
const MAX_MESSAGE: usize = 256 * 1024;
const MAX_IDENTITIES: u32 = 1024;
const ED25519_SIGNATURE: usize = 64;

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
}

impl Agent {
    /// Connect to the agent socket; every exchange is bounded by `timeout`.
    pub fn connect(socket: &Path, timeout: Duration) -> Result<Self, Error> {
        let stream = UnixStream::connect(socket)
            .map_err(|error| agent_error(format!("cannot connect to {}: {error}", socket.display())))?;
        stream
            .set_read_timeout(Some(timeout))
            .and_then(|()| stream.set_write_timeout(Some(timeout)))
            .map_err(|error| agent_error(format!("cannot bound agent I/O: {error}")))?;
        Ok(Self { stream })
    }

    fn exchange(&mut self, request: &[u8]) -> Result<Vec<u8>, Error> {
        let mut framed = (request.len() as u32).to_be_bytes().to_vec();
        framed.extend_from_slice(request);
        self.stream
            .write_all(&framed)
            .map_err(|error| agent_error(format!("agent write failed: {error}")))?;
        let mut length = [0u8; 4];
        self.stream
            .read_exact(&mut length)
            .map_err(|error| agent_error(format!("agent read failed: {error}")))?;
        let length = u32::from_be_bytes(length) as usize;
        if length == 0 || length > MAX_MESSAGE {
            return Err(agent_error(format!("agent reply of {length} bytes is out of bounds")));
        }
        let mut reply = vec![0u8; length];
        self.stream
            .read_exact(&mut reply)
            .map_err(|error| agent_error(format!("agent read failed: {error}")))?;
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
