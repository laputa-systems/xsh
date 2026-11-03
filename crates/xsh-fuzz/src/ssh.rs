//! Fuzzing of the native SSH client's untrusted-input surface: the packet decoder, the wire
//! reader, the key-exchange proposal parser and negotiator, `known_hosts` and fingerprint
//! parsing, and the agent reply parsers. Every input is a pure function of the seed, so a
//! failure reproduces with `xsh-fuzz ssh --seed N --iterations 1`.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::net::UnixListener;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

use xsh_ssh::agent::Agent;
use xsh_ssh::wire::{SshReader, Transport};
use xsh_ssh::{DEFAULT_PROVIDER, Fingerprint, KnownHosts, kex};

use crate::rng::Rng;

/// A decoder case opens a TCP connection, so it is rare enough not to exhaust ephemeral ports.
const DECODER_ONE_IN: usize = 2048;

const KEXINIT_LISTS: [&str; 10] = [
    "mlkem768x25519-sha256,curve25519-sha256,kex-strict-s-v00@openssh.com",
    "ssh-ed25519",
    "aes128-gcm@openssh.com,chacha20-poly1305@openssh.com",
    "aes128-gcm@openssh.com,chacha20-poly1305@openssh.com",
    "hmac-sha2-256-etm@openssh.com",
    "hmac-sha2-256-etm@openssh.com",
    "none",
    "none",
    "",
    "",
];

const KNOWN_HOSTS_LINES: [&str; 7] = [
    "example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    "[host]:2222,*.lab,!bad ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    "@revoked h ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    "@cert-authority * ssh-ed25519 AAAA",
    "|1|c2FsdA==|aGFzaA== ssh-ed25519 AAAA",
    "# comment",
    "",
];

/// What one fuzz campaign did.
#[derive(Debug, Default)]
pub struct Summary {
    pub iterations: u64,
    pub failures: Vec<String>,
}

/// Run iterations from `seed` until `iterations` or `duration` is reached.
pub fn run(seed: u64, iterations: Option<u64>, duration: Option<Duration>) -> Summary {
    let started = Instant::now();
    let agent = AgentServer::start();
    let mut summary = Summary::default();
    while iterations.is_none_or(|limit| summary.iterations < limit)
        && duration.is_none_or(|limit| started.elapsed() < limit)
    {
        let case_seed = seed.wrapping_add(summary.iterations);
        summary.iterations += 1;
        let outcome = catch_unwind(AssertUnwindSafe(|| one_case(case_seed, &agent)));
        if let Err(panic) = outcome {
            let message = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|text| (*text).to_owned()))
                .unwrap_or_else(|| "panic".to_owned());
            summary.failures.push(format!("seed {case_seed}: {message}"));
        }
    }
    summary
}

fn one_case(seed: u64, agent: &AgentServer) {
    let mut rng = Rng::new(seed);
    if rng.below(DECODER_ONE_IN) == 0 {
        return decoder_case(&mut rng);
    }
    match rng.below(5) {
        0 => reader_case(&mut rng),
        1 => kexinit_case(&mut rng),
        2 => known_hosts_case(&mut rng),
        3 => fingerprint_case(&mut rng),
        _ => agent_case(&mut rng, agent),
    }
}

fn random_bytes(rng: &mut Rng, limit: usize) -> Vec<u8> {
    let length = rng.below(limit + 1);
    (0..length).map(|_| rng.next_u64() as u8).collect()
}

fn mutate(rng: &mut Rng, mut bytes: Vec<u8>) -> Vec<u8> {
    for _ in 0..=rng.below(4) {
        if bytes.is_empty() {
            bytes.push(rng.next_u64() as u8);
            continue;
        }
        let at = rng.below(bytes.len());
        match rng.below(5) {
            0 => bytes[at] ^= 1 << rng.below(8),
            1 => bytes[at] = rng.next_u64() as u8,
            2 => {
                bytes.remove(at);
            }
            3 => bytes.insert(at, rng.next_u64() as u8),
            _ => bytes.truncate(at),
        }
    }
    bytes
}

fn reader_case(rng: &mut Rng) {
    let bytes = if rng.chance(50) {
        random_bytes(rng, 96)
    } else {
        let mut valid = Vec::new();
        for _ in 0..rng.below(5) {
            let value = random_bytes(rng, 12);
            valid.extend_from_slice(&(value.len() as u32).to_be_bytes());
            valid.extend_from_slice(&value);
        }
        mutate(rng, valid)
    };
    let mut reader = SshReader::new(&bytes);
    for _ in 0..16 {
        let _ = match rng.below(5) {
            0 => reader.u8().map(drop),
            1 => reader.u32().map(drop),
            2 => reader.string().map(drop),
            3 => reader.text().map(drop),
            _ => reader.name_list().map(drop),
        };
        let _ = reader.remaining();
    }
    let _ = reader.finish();
}

fn valid_kexinit() -> Vec<u8> {
    let mut payload = vec![0u8; 16];
    for list in KEXINIT_LISTS {
        payload.extend_from_slice(&(list.len() as u32).to_be_bytes());
        payload.extend_from_slice(list.as_bytes());
    }
    payload.push(0);
    payload.extend_from_slice(&0u32.to_be_bytes());
    payload
}

fn kexinit_case(rng: &mut Rng) {
    let payload = match rng.below(3) {
        0 => random_bytes(rng, 160),
        _ => mutate(rng, valid_kexinit()),
    };
    let _ = kex::negotiate_payload(&payload, rng.chance(50));
}

fn known_hosts_case(rng: &mut Rng) {
    let mut text = String::new();
    for _ in 0..=rng.below(4) {
        let line = if rng.chance(70) {
            (*rng.pick(&KNOWN_HOSTS_LINES)).to_owned()
        } else {
            String::from_utf8_lossy(&random_bytes(rng, 60)).into_owned()
        };
        let bytes = if rng.chance(40) { mutate(rng, line.into_bytes()) } else { line.into_bytes() };
        text.push_str(&String::from_utf8_lossy(&bytes));
        text.push('\n');
    }
    if let Ok(hosts) = KnownHosts::parse(&text) {
        for name in ["example.com", "host", "x.lab", "bad", ""] {
            let _ = hosts.judge(name, if rng.chance(50) { 22 } else { 2222 }, &random_bytes(rng, 40));
        }
    }
}

fn fingerprint_case(rng: &mut Rng) {
    let value = if rng.chance(50) {
        let mut text = String::from("SHA256:");
        for _ in 0..rng.below(50) {
            text.push(*rng.pick(&['A', 'z', '0', '+', '/', '=', '-', '_']));
        }
        text
    } else {
        String::from_utf8_lossy(&random_bytes(rng, 60)).into_owned()
    };
    let _ = Fingerprint::parse(&value);
}

fn frame(payload: &[u8], padding: usize) -> Vec<u8> {
    let length = 1 + payload.len() + padding;
    let mut packet = (length as u32).to_be_bytes().to_vec();
    packet.push(padding as u8);
    packet.extend_from_slice(payload);
    packet.extend(std::iter::repeat_n(0u8, padding));
    packet
}

fn decoder_case(rng: &mut Rng) {
    let bytes = match rng.below(3) {
        0 => random_bytes(rng, 256),
        _ => {
            let mut stream = Vec::new();
            for _ in 0..=rng.below(4) {
                let mut payload = random_bytes(rng, 48);
                if rng.chance(70) {
                    payload.insert(0, [1u8, 2, 3, 4, 20, 21, 31, 80, 94, 98][rng.below(10)]);
                }
                stream.extend_from_slice(&frame(&payload, 4 + rng.below(12)));
            }
            mutate(rng, stream)
        }
    };
    let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
        return;
    };
    let port = listener.local_addr().expect("address").port();
    let feeder = thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let _ = stream.write_all(&bytes);
        }
    });
    let connected = Transport::connect(
        "127.0.0.1",
        port,
        Duration::from_millis(200),
        Duration::from_secs(2),
        None,
        DEFAULT_PROVIDER,
    );
    if let Ok(mut transport) = connected {
        for _ in 0..4096 {
            match transport.try_recv() {
                Ok(Some(_)) => {}
                Ok(None) => match transport.wait(false).and_then(|_| transport.fill_nonblocking()) {
                    Ok(_) => {}
                    Err(_) => break,
                },
                Err(_) => break,
            }
        }
    }
    let _ = feeder.join();
}

struct AgentServer {
    replies: Sender<Vec<u8>>,
    socket: PathBuf,
    _directory: tempfile::TempDir,
}

impl AgentServer {
    fn start() -> Self {
        let directory = tempfile::tempdir().expect("tempdir");
        let socket = directory.path().join("agent");
        let listener = UnixListener::bind(&socket).expect("agent socket");
        let (replies, queue) = channel::<Vec<u8>>();
        thread::spawn(move || serve_agent(&listener, &queue));
        Self {
            replies,
            socket,
            _directory: directory,
        }
    }
}

fn serve_agent(listener: &UnixListener, replies: &Receiver<Vec<u8>>) {
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { return };
        let Ok(reply) = replies.recv() else { return };
        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
        let mut length = [0u8; 4];
        if stream.read_exact(&mut length).is_ok() {
            let mut request = vec![0u8; (u32::from_be_bytes(length) as usize).min(1 << 20)];
            let _ = stream.read_exact(&mut request);
            let _ = stream.write_all(&reply);
        }
    }
}

fn agent_case(rng: &mut Rng, server: &AgentServer) {
    let body = random_bytes(rng, 80);
    let mut reply = match rng.below(3) {
        0 => {
            let mut answer = vec![12u8];
            answer.extend_from_slice(&(rng.below(4) as u32).to_be_bytes());
            answer.extend_from_slice(&body);
            answer
        }
        1 => {
            let mut answer = vec![14u8];
            answer.extend_from_slice(&(body.len() as u32).to_be_bytes());
            answer.extend_from_slice(&body);
            answer
        }
        _ => body,
    };
    if rng.chance(30) {
        reply = mutate(rng, reply);
    }
    let mut framed = if rng.chance(85) {
        (reply.len() as u32).to_be_bytes().to_vec()
    } else {
        (rng.next_u64() as u32).to_be_bytes().to_vec()
    };
    framed.extend_from_slice(&reply);
    server.replies.send(framed).expect("agent server runs");
    if let Ok(mut agent) = Agent::connect(Path::new(&server.socket), Duration::from_millis(500)) {
        let _ = if rng.chance(50) {
            agent.identities().map(drop)
        } else {
            agent.sign(&random_bytes(rng, 40), &random_bytes(rng, 40)).map(drop)
        };
    }
}
