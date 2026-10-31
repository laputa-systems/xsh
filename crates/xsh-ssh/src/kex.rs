use graviola::key_agreement::x25519;
use oxish_proto::crypto::{
    CryptoProvider, Digest, HandshakeBuffer, KeyDerivation, KeySourceSide, SharedSecret,
};
use oxish_proto::key_exchange::encode_mpint;
use oxish_proto::named::{EncryptionAlgorithm, KeyExchangeAlgorithm, Named, PublicKeyAlgorithm};
use oxish_proto::{Encode, MessageType};

use crate::error::Error;
use crate::host_key::{HostKeyPin, verify as verify_host_key};
use crate::wire::{Packet, SshReader, Transport};

const KEX_ALGORITHMS: &[&str] = &["curve25519-sha256", STRICT_KEX_CLIENT];
const HOST_KEY_ALGORITHMS: &[&str] = &["ssh-ed25519"];
const ENCRYPTION_ALGORITHMS: &[&str] = &[
    "aes128-gcm@openssh.com",
    "chacha20-poly1305@openssh.com",
];
const MAC_ALGORITHMS: &[&str] = &[
    "hmac-sha2-256-etm@openssh.com",
    "hmac-sha2-512-etm@openssh.com",
    "hmac-sha2-256",
    "hmac-sha2-512",
];
const COMPRESSION_ALGORITHMS: &[&str] = &["none"];
const STRICT_KEX_CLIENT: &str = "kex-strict-c-v00@openssh.com";
const STRICT_KEX_SERVER: &str = "kex-strict-s-v00@openssh.com";

#[derive(Debug)]
struct ServerKexInit {
    kex: Vec<String>,
    host_key: Vec<String>,
    encryption_client_to_server: Vec<String>,
    encryption_server_to_client: Vec<String>,
    mac_client_to_server: Vec<String>,
    first_kex_packet_follows: bool,
}

#[derive(Debug)]
struct Negotiated {
    kex: KeyExchangeAlgorithm<'static>,
    host_key: PublicKeyAlgorithm<'static>,
    encryption_client_to_server: EncryptionAlgorithm<'static>,
    encryption_server_to_client: EncryptionAlgorithm<'static>,
    strict: bool,
}

/// The authenticated transport facts established by key exchange.
#[derive(Debug)]
pub struct Established {
    /// The session identifier used by authentication.
    pub session_id: Digest,
    /// The pinned host-key fingerprint.
    pub fingerprint: String,
    /// The server's encoded host key blob.
    pub host_key_blob: Vec<u8>,
    /// Whether strict key exchange was negotiated.
    pub strict: bool,
}

enum ClientEphemeral {
    X25519 {
        private: x25519::PrivateKey,
        public: [u8; 32],
    },
}

impl ClientEphemeral {
    fn new(algorithm: &KeyExchangeAlgorithm<'static>) -> Result<Self, Error> {
        match algorithm {
            KeyExchangeAlgorithm::Curve25519Sha256 => {
                let private = x25519::PrivateKey::new_random().map_err(|error| Error::Protocol {
                    detail: format!("cannot generate X25519 key: {error}"),
                })?;
                let public = private.public_key().as_bytes();
                Ok(Self::X25519 { private, public })
            }
            other => Err(Error::UnsupportedAlgorithm {
                category: "key exchange",
                name: other.name().to_owned(),
            }),
        }
    }

    fn public_key(&self) -> &[u8] {
        match self {
            Self::X25519 { public, .. } => public,
        }
    }

    fn complete(self, peer_public_key: &[u8]) -> Result<Vec<u8>, Error> {
        match self {
            Self::X25519 { private, .. } => {
                let peer = x25519::PublicKey::try_from_slice(peer_public_key).map_err(|error| {
                    Error::Protocol {
                        detail: format!("invalid X25519 peer key: {error}"),
                    }
                })?;
                let shared = private
                    .diffie_hellman(&peer)
                    .map_err(|error| Error::Protocol {
                        detail: format!("X25519 key agreement failed: {error}"),
                    })?;
                Ok(shared.as_bytes().to_vec())
            }
        }
    }
}

fn put_name_list(buffer: &mut Vec<u8>, names: &[&str]) {
    let value = names.join(",");
    buffer.extend_from_slice(&(value.len() as u32).to_be_bytes());
    buffer.extend_from_slice(value.as_bytes());
}

fn put_string(buffer: &mut Vec<u8>, value: &[u8]) {
    buffer.extend_from_slice(&(value.len() as u32).to_be_bytes());
    buffer.extend_from_slice(value);
}

fn kexinit_payload(provider: &dyn CryptoProvider) -> Result<Vec<u8>, Error> {
    let mut buffer = Vec::new();
    MessageType::KeyExchangeInit.encode(&mut buffer);
    let mut cookie = [0u8; 16];
    provider
        .secure_random()
        .fill(&mut cookie)
        .map_err(|error| Error::Protocol {
            detail: format!("cannot generate KEXINIT cookie: {error}"),
        })?;
    buffer.extend_from_slice(&cookie);
    put_name_list(&mut buffer, KEX_ALGORITHMS);
    put_name_list(&mut buffer, HOST_KEY_ALGORITHMS);
    put_name_list(&mut buffer, ENCRYPTION_ALGORITHMS);
    put_name_list(&mut buffer, ENCRYPTION_ALGORITHMS);
    put_name_list(&mut buffer, MAC_ALGORITHMS);
    put_name_list(&mut buffer, MAC_ALGORITHMS);
    put_name_list(&mut buffer, COMPRESSION_ALGORITHMS);
    put_name_list(&mut buffer, COMPRESSION_ALGORITHMS);
    put_name_list(&mut buffer, &[]);
    put_name_list(&mut buffer, &[]);
    buffer.push(0);
    buffer.extend_from_slice(&0u32.to_be_bytes());
    Ok(buffer)
}

fn parse_kexinit(payload: &[u8]) -> Result<ServerKexInit, Error> {
    let mut reader = SshReader::new(payload);
    let mut cookie = [0u8; 16];
    for byte in &mut cookie {
        *byte = reader.u8()?;
    }
    let kex = reader.name_list()?;
    let host_key = reader.name_list()?;
    let encryption_client_to_server = reader.name_list()?;
    let encryption_server_to_client = reader.name_list()?;
    let mac_client_to_server = reader.name_list()?;
    let _mac_server_to_client = reader.name_list()?;
    let _compression_client_to_server = reader.name_list()?;
    let _compression_server_to_client = reader.name_list()?;
    let _language_client_to_server = reader.name_list()?;
    let _language_server_to_client = reader.name_list()?;
    let first_kex_packet_follows = reader.u8()? != 0;
    let _reserved = reader.u32()?;
    reader.finish()?;
    Ok(ServerKexInit {
        kex,
        host_key,
        encryption_client_to_server,
        encryption_server_to_client,
        mac_client_to_server,
        first_kex_packet_follows,
    })
}

fn first_common(client: &[&str], server: &[String], category: &'static str) -> Result<String, Error> {
    client
        .iter()
        .find(|name| server.iter().any(|candidate| candidate == *name))
        .map(|name| (*name).to_owned())
        .ok_or_else(|| Error::UnsupportedAlgorithm {
            category,
            name: client.first().copied().unwrap_or("").to_owned(),
        })
}

fn negotiate(server: &ServerKexInit) -> Result<Negotiated, Error> {
    if server.first_kex_packet_follows {
        return Err(Error::Protocol {
            detail: "server sent a guessed key-exchange packet".to_owned(),
        });
    }
    let kex_name = first_common(KEX_ALGORITHMS, &server.kex, "key exchange")?;
    let kex = match kex_name.as_str() {
        "curve25519-sha256" => KeyExchangeAlgorithm::Curve25519Sha256,
        other => {
            return Err(Error::UnsupportedAlgorithm {
                category: "key exchange",
                name: other.to_owned(),
            });
        }
    };
    let host_key_name = first_common(HOST_KEY_ALGORITHMS, &server.host_key, "host key")?;
    let host_key = match host_key_name.as_str() {
        "ssh-ed25519" => PublicKeyAlgorithm::Ed25519,
        other => {
            return Err(Error::UnsupportedAlgorithm {
                category: "host key",
                name: other.to_owned(),
            });
        }
    };
    let c2s_name = first_common(
        ENCRYPTION_ALGORITHMS,
        &server.encryption_client_to_server,
        "client-to-server encryption",
    )?;
    let s2c_name = first_common(
        ENCRYPTION_ALGORITHMS,
        &server.encryption_server_to_client,
        "server-to-client encryption",
    )?;
    let encryption_client_to_server = encryption(&c2s_name)?;
    let encryption_server_to_client = encryption(&s2c_name)?;
    let _mac = first_common(MAC_ALGORITHMS, &server.mac_client_to_server, "MAC")?;
    let strict = server.kex.iter().any(|name| name == STRICT_KEX_SERVER);
    if !strict {
        return Err(Error::StrictKexUnsupported);
    }
    Ok(Negotiated {
        kex,
        host_key,
        encryption_client_to_server,
        encryption_server_to_client,
        strict,
    })
}

fn encryption(name: &str) -> Result<EncryptionAlgorithm<'static>, Error> {
    match name {
        "aes128-gcm@openssh.com" => Ok(EncryptionAlgorithm::Aes128Gcm),
        "chacha20-poly1305@openssh.com" => Ok(EncryptionAlgorithm::Chacha20Poly1305),
        other => Err(Error::UnsupportedAlgorithm {
            category: "encryption",
            name: other.to_owned(),
        }),
    }
}

fn recv_expected(transport: &mut Transport, expected: MessageType) -> Result<Packet, Error> {
    loop {
        let packet = transport.recv()?;
        match packet.message_type {
            message_type if message_type == expected => return Ok(packet),
            MessageType::Ignore | MessageType::Debug | MessageType::Unimplemented => continue,
            MessageType::Disconnect => {
                return Err(Error::Protocol {
                    detail: "server disconnected during key exchange".to_owned(),
                });
            }
            other => {
                return Err(Error::Protocol {
                    detail: format!("expected {expected:?}, got {other:?}"),
                });
            }
        }
    }
}

/// Run client key exchange and verify the pinned host key.
pub fn connect(
    transport: &mut Transport,
    pin: &HostKeyPin,
    provider: &dyn CryptoProvider,
) -> Result<Established, Error> {
    let mut exchange: HandshakeBuffer = transport.handshake_buffer();
    let client_kexinit = kexinit_payload(provider)?;
    exchange.prefixed(&client_kexinit);
    transport.send_raw(&client_kexinit)?;

    let server_kexinit = recv_expected(transport, MessageType::KeyExchangeInit)?;
    exchange.prefixed(&server_kexinit.encoded_payload());
    let server = parse_kexinit(&server_kexinit.payload)?;
    let negotiated = negotiate(&server)?;

    let hash_algorithm = provider.hash(&negotiated.kex)?;
    let mut exchange_hash = exchange.hash(hash_algorithm);

    let ephemeral = ClientEphemeral::new(&negotiated.kex)?;
    let client_public = ephemeral.public_key().to_vec();
    let mut init = Vec::new();
    MessageType::KeyExchangeEcdhInit.encode(&mut init);
    put_string(&mut init, &client_public);
    transport.send_raw(&init)?;

    let reply = recv_expected(transport, MessageType::KeyExchangeEcdhReply)?;
    let mut reader = SshReader::new(&reply.payload);
    let host_key_blob = reader.string()?.to_vec();
    let server_public = reader.string()?.to_vec();
    let signature_blob = reader.string()?.to_vec();
    reader.finish()?;

    exchange_hash.prefixed(&host_key_blob);
    exchange_hash.prefixed(&client_public);
    exchange_hash.prefixed(&server_public);
    let shared = ephemeral.complete(&server_public)?;
    let mut shared_secret = Vec::new();
    encode_mpint(&shared, &mut shared_secret);
    exchange_hash.update(&shared_secret);
    let exchange_digest = exchange_hash.finish();

    let actual = pin.fingerprint(&host_key_blob, provider)?;
    if actual != pin.as_str() {
        return Err(Error::HostKeyMismatch {
            expected: pin.as_str().to_owned(),
            actual,
        });
    }
    verify_host_key(
        &host_key_blob,
        &signature_blob,
        exchange_digest.as_ref(),
        provider,
        &negotiated.host_key,
    )?;

    let derivation = KeyDerivation {
        hash: hash_algorithm,
        shared_secret: SharedSecret::from(shared_secret),
        exchange_hash: exchange_digest.clone(),
        session_id: exchange_digest.clone(),
    };
    let client_to_server =
        KeySourceSide::client_to_server(&derivation, negotiated.encryption_client_to_server)?;
    let server_to_client =
        KeySourceSide::server_to_client(&derivation, negotiated.encryption_server_to_client)?;

    transport.send_newkeys(negotiated.strict)?;
    transport.install_sealer(&client_to_server, provider)?;
    transport.recv_newkeys(negotiated.strict)?;
    transport.install_opener(&server_to_client, provider)?;

    Ok(Established {
        session_id: exchange_digest,
        fingerprint: actual,
        host_key_blob,
        strict: negotiated.strict,
    })
}
