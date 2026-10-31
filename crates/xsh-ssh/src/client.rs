use std::path::PathBuf;
use std::time::Duration;

use oxish_proto::crypto::CryptoProvider;

use crate::error::Error;
use crate::host_key::HostKeyPin;
use crate::kex::{self, Established};
use crate::wire::Transport;
use crate::DEFAULT_PROVIDER;

/// Explicit connection, authentication, and trust options.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// The host name or address.
    pub host: String,
    /// The SSH port.
    pub port: u16,
    /// The remote user name.
    pub user: String,
    /// The unencrypted Ed25519 private key file.
    pub key: PathBuf,
    /// The mandatory server host-key fingerprint.
    pub host_key: HostKeyPin,
    /// The connect, read, and write deadline.
    pub timeout: Duration,
}

impl ClientConfig {
    /// Build a configuration with a parsed host-key pin.
    pub fn new(
        host: impl Into<String>,
        port: u16,
        user: impl Into<String>,
        key: impl Into<PathBuf>,
        host_key: &str,
        timeout: Duration,
    ) -> Result<Self, Error> {
        Ok(Self {
            host: host.into(),
            port,
            user: user.into(),
            key: key.into(),
            host_key: HostKeyPin::parse(host_key)?,
            timeout,
        })
    }
}

/// An authenticated SSH connection.
pub struct Connection {
    transport: Transport,
    established: Established,
}

impl Connection {
    /// Connect and verify the pinned host key.
    pub fn connect(config: &ClientConfig) -> Result<Self, Error> {
        Self::connect_with(config, DEFAULT_PROVIDER)
    }

    /// Connect with an explicit cryptographic provider.
    pub fn connect_with(
        config: &ClientConfig,
        provider: &dyn CryptoProvider,
    ) -> Result<Self, Error> {
        let mut transport =
            Transport::connect(&config.host, config.port, config.timeout, provider)?;
        transport.exchange_identifications("xsh_0.0.1")?;
        let established = kex::connect(&mut transport, &config.host_key, provider)?;
        Ok(Self {
            transport,
            established,
        })
    }

    /// The verified server fingerprint.
    pub fn fingerprint(&self) -> &str {
        &self.established.fingerprint
    }

    /// The session identifier used by user authentication.
    pub fn session_id(&self) -> &[u8] {
        self.established.session_id.as_ref()
    }

    /// The server host key blob.
    pub fn host_key_blob(&self) -> &[u8] {
        &self.established.host_key_blob
    }

    /// Whether strict key exchange was negotiated.
    pub fn strict(&self) -> bool {
        self.established.strict
    }

    /// The underlying transport, for the authentication and channel layers.
    pub fn transport(&mut self) -> &mut Transport {
        &mut self.transport
    }
}
