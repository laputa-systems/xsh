use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use oxish_proto::crypto::CryptoProvider;

use crate::auth;
use crate::error::Error;
use crate::exec::{self, ExecOptions, ExecResult};
use crate::host_key::HostKeyPin;
use crate::kex::{self, Established};
use crate::stream::{self, StreamOptions};
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
    config: ClientConfig,
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
            config: config.clone(),
            transport,
            established,
        })
    }

    /// Authenticate the configured user with the configured Ed25519 key.
    pub fn authenticate(&mut self) -> Result<(), Error> {
        self.authenticate_with(DEFAULT_PROVIDER)
    }

    /// Authenticate with an explicit cryptographic provider.
    pub fn authenticate_with(&mut self, provider: &dyn CryptoProvider) -> Result<(), Error> {
        auth::authenticate(
            &mut self.transport,
            &self.config.user,
            &self.config.key,
            self.established.session_id.as_ref(),
            provider,
        )
    }

    /// Run one remote command with bounded captured output.
    pub fn exec(&mut self, command: &str, options: &ExecOptions) -> Result<ExecResult, Error> {
        exec::exec(&mut self.transport, command, options)
    }

    /// Run one remote command with streaming stdin and independent output sinks.
    pub fn exec_stream(
        &mut self,
        command: &str,
        options: &StreamOptions,
        stdin: Option<&mut dyn std::io::Read>,
        stdout: &mut dyn std::io::Write,
        stderr: &mut dyn std::io::Write,
    ) -> Result<ExecResult, Error> {
        let pin = self.config.host_key.clone();
        let session_id = self.established.session_id.clone();
        let strict_kex = self.established.strict;
        stream::exec_stream(
            &mut self.transport,
            command,
            options,
            stdin,
            stdout,
            stderr,
            &pin,
            DEFAULT_PROVIDER,
            session_id.as_ref(),
            strict_kex,
        )
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

impl fmt::Debug for Connection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Connection")
            .field("host", &self.config.host)
            .field("port", &self.config.port)
            .field("user", &self.config.user)
            .field("fingerprint", &self.established.fingerprint)
            .finish_non_exhaustive()
    }
}
