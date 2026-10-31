use std::fs;
use std::path::Path;
use std::str::FromStr;

use data_encoding::BASE64;
use oxish_proto::auth::SignatureData;
use oxish_proto::crypto::{CryptoProvider, SigningKey};
use oxish_proto::named::{MethodName, PublicKeyAlgorithm, ServiceName};
use oxish_proto::{Encode, MessageType, OpenSshKeyV1};

use crate::error::Error;
use crate::wire::{SshReader, Transport, put_string};

const SERVICE_USERAUTH: &str = "ssh-userauth";

/// Load an unencrypted Ed25519 key from an OpenSSH private-key file.
pub fn load_key(
    path: &Path,
    provider: &dyn CryptoProvider,
) -> Result<Box<dyn SigningKey>, Error> {
    let text = fs::read_to_string(path).map_err(|error| Error::KeyFile {
        detail: format!("cannot read {}: {error}", path.display()),
    })?;
    if is_encrypted(&text) {
        return Err(Error::EncryptedKey);
    }
    let file = OpenSshKeyV1::from_str(&text).map_err(|error| Error::KeyFile {
        detail: format!("cannot parse {}: {error}", path.display()),
    })?;
    let pkcs8 = file
        .pkcs8_keys()
        .next()
        .ok_or_else(|| Error::KeyFile {
            detail: "key file contains no keys".to_owned(),
        })?;
    let key = provider
        .signing_key_from_pkcs8(&pkcs8)
        .map_err(|error| Error::KeyFile {
            detail: format!("cannot load {}: {error}", path.display()),
        })?;
    if key.algorithm() != PublicKeyAlgorithm::Ed25519 {
        return Err(Error::KeyFile {
            detail: "only Ed25519 keys are supported".to_owned(),
        });
    }
    Ok(key)
}

fn is_encrypted(text: &str) -> bool {
    if text.contains("Proc-Type: 4,ENCRYPTED") {
        return true;
    }
    let Some(body) = text
        .split("-----BEGIN OPENSSH PRIVATE KEY-----")
        .nth(1)
        .and_then(|rest| rest.split("-----END OPENSSH PRIVATE KEY-----").next())
    else {
        return false;
    };
    let compact: String = body.chars().filter(|character| !character.is_whitespace()).collect();
    let Ok(decoded) = BASE64.decode(compact.as_bytes()) else {
        return false;
    };
    let Some(rest) = decoded.strip_prefix(b"openssh-key-v1\0") else {
        return false;
    };
    let Some(length) = rest.get(..4) else {
        return false;
    };
    let length = u32::from_be_bytes([length[0], length[1], length[2], length[3]]) as usize;
    let Some(cipher) = rest.get(4..4 + length) else {
        return false;
    };
    cipher != b"none"
}

/// Authenticate the configured user with the loaded Ed25519 key.
pub fn authenticate(
    transport: &mut Transport,
    user: &str,
    key_path: &Path,
    session_id: &[u8],
    provider: &dyn CryptoProvider,
) -> Result<(), Error> {
    let key = load_key(key_path, provider)?;

    transport.send_raw(&service_request(SERVICE_USERAUTH))?;
    loop {
        let packet = transport.recv()?;
        match packet.message_type {
            MessageType::ServiceAccept => break,
            MessageType::UserAuthBanner => continue,
            MessageType::Disconnect => {
                return Err(Error::Authentication {
                    detail: "server disconnected before authentication".to_owned(),
                });
            }
            other => {
                return Err(Error::Protocol {
                    detail: format!("expected service accept, got {other:?}"),
                });
            }
        }
    }

    let algorithm = key.algorithm();
    let mut public_key = Vec::new();
    algorithm.encode(&mut public_key);
    key.public_key().encode(&mut public_key);

    let signature_input = SignatureData {
        session_id,
        user_name: user,
        service_name: ServiceName::Connection,
        algorithm: algorithm.clone(),
        public_key: &public_key,
    }
    .encode();
    let raw_signature = key.sign(signature_input.as_bytes());
    let mut signature = Vec::new();
    algorithm.encode(&mut signature);
    raw_signature.as_slice().encode(&mut signature);

    let mut request = Vec::new();
    MessageType::UserAuthRequest.encode(&mut request);
    put_string(&mut request, user.as_bytes());
    ServiceName::Connection.encode(&mut request);
    MethodName::PublicKey.encode(&mut request);
    request.push(1);
    algorithm.encode(&mut request);
    put_string(&mut request, &public_key);
    put_string(&mut request, &signature);
    transport.send_raw(&request)?;

    loop {
        let packet = transport.recv()?;
        match packet.message_type {
            MessageType::UserAuthSuccess => return Ok(()),
            MessageType::UserAuthBanner => continue,
            MessageType::UserAuthFailure => {
                let mut reader = SshReader::new(&packet.payload);
                let methods = reader.name_list()?.join(",");
                let partial = reader.u8()? != 0;
                return Err(Error::Authentication {
                    detail: format!("server rejected publickey; methods={methods} partial={partial}"),
                });
            }
            MessageType::UserAuthPkOk => continue,
            MessageType::Disconnect => {
                return Err(Error::Authentication {
                    detail: "server disconnected during authentication".to_owned(),
                });
            }
            other => {
                return Err(Error::Protocol {
                    detail: format!("unexpected authentication message {other:?}"),
                });
            }
        }
    }
}

fn service_request(service: &str) -> Vec<u8> {
    let mut request = Vec::new();
    MessageType::ServiceRequest.encode(&mut request);
    put_string(&mut request, service.as_bytes());
    request
}
