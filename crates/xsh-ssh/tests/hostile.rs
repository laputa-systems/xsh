mod support;

use std::io::{self, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use support::*;
use xsh_ssh::{
    Certainty, ClientConfig, Connection, Error, ExecOptions, ExecStatus, HostTrust, Identity,
    RekeyLimits, StreamOptions,
};

const TIMEOUT: Duration = Duration::from_secs(5);
const MEMORY_BUDGET: usize = 24 * 1024 * 1024;

fn exclusive() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn within<T: Send + 'static>(limit: Duration, run: impl FnOnce() -> T + Send + 'static) -> T {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(run());
    });
    match receiver.recv_timeout(limit) {
        Ok(value) => value,
        Err(RecvTimeoutError::Timeout) => panic!("the client did not return within {limit:?}"),
        Err(RecvTimeoutError::Disconnected) => panic!("the client thread panicked"),
    }
}

fn serve_exec(
    behavior: Behavior,
    window: u32,
    body: impl FnOnce(&mut Link, u32) + Send + 'static,
) -> Peer {
    spawn(behavior, move |link| {
        link.accept_authentication();
        let opened = link.accept_exec(window, 32 * 1024);
        body(link, opened.client_channel);
    })
}

fn data_packet(channel: u32, length: usize) -> Vec<u8> {
    message(CHANNEL_DATA, &[&number(channel), &text(&vec![0x41; length])])
}

#[test]
fn a_file_option_does_not_lift_the_capture_limit_of_the_other_streams() {
    let _guard = exclusive();
    let directory = tempfile::tempdir().expect("tempdir");
    let sink = directory.path().join("stderr");
    let peer = serve_exec(Behavior::default(), 1 << 20, |link, channel| {
        for _ in 0..8 {
            link.data(channel, &[1; 4096]);
        }
    });
    let mut connection = peer.connect(TIMEOUT);
    let error = connection
        .exec(
            "command",
            &ExecOptions {
                max_output: 1024,
                stdin_file: Some(std::fs::File::open("/dev/null").expect("/dev/null")),
                stderr_file: Some(std::fs::File::create(&sink).expect("stderr file")),
                ..ExecOptions::default()
            },
        )
        .expect_err("stdout is still captured under the limit");
    assert!(matches!(error, Error::OutputLimit { stream: "stdout" }), "{error:?}");
    drop(connection);
    peer.finish();
}

#[test]
fn output_sent_to_a_file_is_not_limited_and_not_captured() {
    let _guard = exclusive();
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("stdout");
    let peer = serve_exec(Behavior::default(), 1 << 20, |link, channel| {
        for _ in 0..8 {
            link.data(channel, &[1; 4096]);
        }
        link.exit_status(channel, 0);
        link.eof(channel);
        link.close(channel);
        link.expect_close();
    });
    let mut connection = peer.connect(TIMEOUT);
    let result = connection
        .exec(
            "command",
            &ExecOptions {
                max_output: 1024,
                stdout_file: Some(std::fs::File::create(&path).expect("stdout file")),
                ..ExecOptions::default()
            },
        )
        .expect("a file takes any amount");
    assert_eq!(result.status, ExecStatus::Exited(0));
    assert!(result.stdout.is_empty());
    assert_eq!(std::fs::metadata(&path).expect("file").len(), 8 * 4096);
    drop(connection);
    peer.finish();
}

#[test]
fn traffic_queued_before_a_rekey_is_bounded_by_the_window() {
    let _guard = exclusive();
    let peer = serve_exec(Behavior::default(), 1 << 20, |link, channel| {
        let packet = data_packet(channel, 32 * 1024);
        for _ in 0..4 {
            link.send(&packet);
        }
        thread::sleep(Duration::from_millis(400));
        for _ in 0..8192 {
            if !link.try_send(&packet) {
                return;
            }
        }
        thread::sleep(Duration::from_secs(2));
    });
    let mut config = peer.config(Duration::from_secs(1));
    config.rekey = RekeyLimits {
        bytes: 64 * 1024,
        ..RekeyLimits::default()
    };
    let (outcome, growth) = peak_memory_growth(|| {
        let mut connection = Connection::connect(&config).expect("connect");
        connection.authenticate().expect("authenticate");
        connection.exec("command", &ExecOptions::default())
    });
    assert!(growth < MEMORY_BUDGET, "queued {growth} bytes");
    let error = outcome.expect_err("a server that never answers the key exchange fails the command");
    assert!(error.to_string().contains("channel windows allow"), "{error:?}");
    peer.finish();
}

#[test]
fn the_server_may_not_send_connection_layer_traffic_after_its_kexinit() {
    let _guard = exclusive();
    let peer = serve_exec(Behavior::default(), 1 << 20, |link, channel| {
        link.send(&kexinit());
        link.send(&data_packet(channel, 16));
        thread::sleep(Duration::from_secs(1));
    });
    let error = within(Duration::from_secs(10), {
        let mut connection = peer.connect(Duration::from_millis(500));
        move || connection.exec("command", &ExecOptions::default()).expect_err("refused")
    });
    assert!(error.to_string().contains("after its key-exchange init"), "{error}");
    peer.finish();
}

#[test]
fn a_stalled_write_does_not_buffer_the_peers_traffic_without_bound() {
    let _guard = exclusive();
    let peer = serve_exec(Behavior::default(), u32::MAX / 2, |link, channel| {
        thread::sleep(Duration::from_millis(600));
        let packet = data_packet(channel, 32 * 1024);
        for _ in 0..4096 {
            if !link.try_send(&packet) {
                return;
            }
        }
        thread::sleep(Duration::from_secs(2));
    });
    let mut config = peer.config(Duration::from_millis(800));
    config.rekey = RekeyLimits {
        interval: Duration::from_millis(300),
        ..RekeyLimits::default()
    };
    let mut connection = Connection::connect(&config).expect("connect");
    connection.authenticate().expect("authenticate");
    let (outcome, growth) = peak_memory_growth(|| {
        connection.exec_stream(
            "command",
            &StreamOptions::default(),
            Some(&mut Endless),
            &mut io::sink(),
            &mut io::sink(),
        )
    });
    let error = outcome.expect_err("the stalled write ends in a timeout");
    assert!(matches!(error, Error::Timeout { certainty: Certainty::Submitted }), "{error:?}");
    assert!(growth < MEMORY_BUDGET, "buffered {growth} bytes");
    drop(connection);
    peer.finish();
}

#[test]
fn reads_do_not_reset_the_deadline_of_a_stalled_write() {
    let _guard = exclusive();
    let peer = serve_exec(Behavior::default(), u32::MAX / 2, |link, _| {
        let ignore = message(IGNORE, &[&text(b"keep alive")]);
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(4) {
            if !link.try_send(&ignore) {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
    });
    let mut connection = peer.connect(Duration::from_millis(500));
    let started = Instant::now();
    let error = connection
        .exec_stream(
            "command",
            &StreamOptions::default(),
            Some(&mut Endless),
            &mut io::sink(),
            &mut io::sink(),
        )
        .expect_err("the stalled write times out");
    assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());
    assert!(matches!(error, Error::Timeout { certainty: Certainty::Submitted }), "{error:?}");
    drop(connection);
    peer.finish();
}

#[test]
fn replies_to_a_flood_of_requests_are_not_queued_while_the_peer_does_not_read() {
    let _guard = exclusive();
    let peer = serve_exec(Behavior::default(), 1 << 20, |link, _| {
        let request = message(GLOBAL_REQUEST, &[&text(b"flood@example"), &[1]]);
        for _ in 0..1_500_000 {
            if !link.try_send(&request) {
                return;
            }
        }
        thread::sleep(Duration::from_secs(1));
    });
    let mut connection = peer.connect(Duration::from_millis(500));
    let (outcome, growth) = peak_memory_growth(|| {
        connection.exec(
            "command",
            &ExecOptions {
                timeout: Some(Duration::from_secs(20)),
                ..ExecOptions::default()
            },
        )
    });
    let error = outcome.expect_err("a peer that does not read its replies is cut off");
    assert!(matches!(error, Error::Timeout { certainty: Certainty::Submitted }), "{error:?}");
    assert!(growth < MEMORY_BUDGET, "queued {growth} bytes of replies");
    drop(connection);
    peer.finish();
}

#[test]
fn channel_request_replies_are_not_queued_while_the_peer_does_not_read() {
    let _guard = exclusive();
    let peer = serve_exec(Behavior::default(), 1 << 20, |link, channel| {
        let request = message(
            CHANNEL_REQUEST,
            &[&number(channel), &text(b"flood@example"), &[1]],
        );
        for _ in 0..1_500_000 {
            if !link.try_send(&request) {
                return;
            }
        }
        thread::sleep(Duration::from_secs(1));
    });
    let mut connection = peer.connect(Duration::from_millis(500));
    let (outcome, growth) = peak_memory_growth(|| {
        connection.exec(
            "command",
            &ExecOptions {
                timeout: Some(Duration::from_secs(20)),
                ..ExecOptions::default()
            },
        )
    });
    let error = outcome.expect_err("a peer that does not read its replies is cut off");
    assert!(matches!(error, Error::Timeout { certainty: Certainty::Submitted }), "{error:?}");
    assert!(growth < MEMORY_BUDGET, "queued {growth} bytes of replies");
    drop(connection);
    peer.finish();
}

struct Endless;

impl Read for Endless {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        buffer.fill(7);
        Ok(buffer.len())
    }
}

fn dripping_server(interval: Duration) -> (u16, Arc<AtomicBool>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
    let port = listener.local_addr().expect("address").port();
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = Arc::clone(&stop);
    thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut bytes = b"SSH-2.0-".iter().copied().chain(std::iter::repeat(b'x'));
        while !stopped.load(Ordering::SeqCst) {
            let byte = bytes.next().expect("endless");
            if stream.write_all(&[byte]).is_err() {
                return;
            }
            thread::sleep(interval);
        }
    });
    (port, stop)
}

fn drip_config(port: u16) -> ClientConfig {
    ClientConfig::new(
        "127.0.0.1",
        port,
        "tester",
        Identity::KeyFile("/nonexistent/key".into()),
        HostTrust::Pinned(
            xsh_ssh::Fingerprint::parse("SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
                .expect("fingerprint"),
        ),
        Duration::from_millis(500),
    )
}

#[test]
fn a_peer_that_drips_bytes_cannot_hold_connect_past_its_deadline() {
    let _guard = exclusive();
    let (port, stop) = dripping_server(Duration::from_millis(200));
    let mut config = drip_config(port);
    config.deadline = Duration::from_millis(1500);
    let started = Instant::now();
    let outcome = within(Duration::from_secs(8), move || Connection::connect(&config));
    stop.store(true, Ordering::SeqCst);
    let error = outcome.expect_err("deadline");
    assert!(started.elapsed() < Duration::from_secs(5), "{:?}", started.elapsed());
    assert!(matches!(error, Error::Timeout { certainty: Certainty::NotSubmitted }), "{error:?}");
}

#[test]
fn the_caller_can_cancel_a_connect_that_the_peer_keeps_alive() {
    let _guard = exclusive();
    let (port, stop) = dripping_server(Duration::from_millis(100));
    let cancelled = Arc::new(AtomicBool::new(false));
    let mut config = drip_config(port);
    config.interrupt = Some({
        let cancelled = Arc::clone(&cancelled);
        Arc::new(move || cancelled.load(Ordering::SeqCst))
    });
    let flag = Arc::clone(&cancelled);
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(400));
        flag.store(true, Ordering::SeqCst);
    });
    let started = Instant::now();
    let outcome = within(Duration::from_secs(8), move || Connection::connect(&config));
    stop.store(true, Ordering::SeqCst);
    let error = outcome.expect_err("cancelled");
    assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
    assert!(matches!(error, Error::Cancelled { certainty: Certainty::NotSubmitted }), "{error:?}");
}

fn flooding_banner_peer() -> Peer {
    spawn(Behavior::default(), |link| {
        link.expect(SERVICE_REQUEST);
        link.send(&message(SERVICE_ACCEPT, &[&text(b"ssh-userauth")]));
        link.expect(USERAUTH_REQUEST);
        let banner = message(USERAUTH_BANNER, &[&text(b"welcome"), &text(b"")]);
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(6) {
            if !link.try_send(&banner) {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
    })
}

#[test]
fn a_flood_of_banners_cannot_hold_authentication_past_its_deadline() {
    let _guard = exclusive();
    let peer = flooding_banner_peer();
    let mut config = peer.config(Duration::from_millis(500));
    config.deadline = Duration::from_millis(1200);
    let mut connection = Connection::connect(&config).expect("connect");
    let started = Instant::now();
    let error = within(Duration::from_secs(8), move || {
        let outcome = connection.authenticate();
        (outcome, connection)
    })
    .0
    .expect_err("deadline");
    assert!(started.elapsed() < Duration::from_secs(4), "{:?}", started.elapsed());
    assert!(matches!(error, Error::Timeout { certainty: Certainty::NotSubmitted }), "{error:?}");
    peer.finish();
}

#[test]
fn the_caller_can_cancel_authentication() {
    let _guard = exclusive();
    let peer = flooding_banner_peer();
    let cancelled = Arc::new(AtomicBool::new(false));
    let mut config = peer.config(Duration::from_millis(500));
    config.interrupt = Some({
        let cancelled = Arc::clone(&cancelled);
        Arc::new(move || cancelled.load(Ordering::SeqCst))
    });
    let mut connection = Connection::connect(&config).expect("connect");
    let flag = Arc::clone(&cancelled);
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(300));
        flag.store(true, Ordering::SeqCst);
    });
    let started = Instant::now();
    let error = within(Duration::from_secs(8), move || {
        let outcome = connection.authenticate();
        (outcome, connection)
    })
    .0
    .expect_err("cancelled");
    assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
    assert!(matches!(error, Error::Cancelled { certainty: Certainty::NotSubmitted }), "{error:?}");
    peer.finish();
}

#[test]
fn a_rekey_with_a_different_host_key_is_refused_even_when_both_are_trusted() {
    let _guard = exclusive();
    let peer = serve_exec(
        Behavior {
            rekey_with_another_host_key: true,
            ..Behavior::default()
        },
        1 << 20,
        |link, channel| {
            for _ in 0..40 {
                link.data(channel, &[9; 4096]);
            }
            thread::sleep(Duration::from_secs(1));
            link.exit_status(channel, 0);
            link.eof(channel);
            link.close(channel);
        },
    );
    let mut config = peer.known_hosts_config(TIMEOUT);
    config.rekey = RekeyLimits {
        bytes: 32 * 1024,
        ..RekeyLimits::default()
    };
    let mut connection = Connection::connect(&config).expect("connect");
    connection.authenticate().expect("authenticate");
    let error = connection
        .exec("command", &ExecOptions::default())
        .expect_err("the host key changed");
    assert!(error.to_string().contains("host key mismatch"), "{error:?}");
    assert!(!connection.is_usable());
    drop(connection);
    peer.finish();
}

const HOSTILE_TEXT: &str = "\u{1b}[2J\u{1b}]0;owned\u{7}line\nbreak\rreturn\u{0}nul";

fn assert_plain(text: &str) {
    assert!(
        !text.chars().any(char::is_control),
        "server text reached the message with control characters: {text:?}"
    );
}

#[test]
fn a_disconnect_description_is_stripped_of_control_characters() {
    let _guard = exclusive();
    let peer = serve_exec(Behavior::default(), 1 << 20, |link, _| {
        link.send(&message(
            DISCONNECT,
            &[&number(11), &text(HOSTILE_TEXT.as_bytes()), &text(b"")],
        ));
    });
    let error = peer
        .connect(TIMEOUT)
        .exec("command", &ExecOptions::default())
        .expect_err("disconnect");
    assert_plain(&error.to_string());
    assert!(error.to_string().contains("breakreturn") || error.to_string().contains("line"));
    peer.finish();
}

#[test]
fn a_channel_open_failure_description_is_stripped_of_control_characters() {
    let _guard = exclusive();
    let peer = spawn(Behavior::default(), |link| {
        link.accept_authentication();
        let open = link.expect(CHANNEL_OPEN);
        let client = open.u32_at(4 + open.u32_at(0) as usize);
        link.send(&message(
            CHANNEL_OPEN_FAILURE,
            &[&number(client), &number(2), &text(HOSTILE_TEXT.as_bytes()), &text(b"")],
        ));
    });
    let error = peer
        .connect(TIMEOUT)
        .exec("command", &ExecOptions::default())
        .expect_err("open failure");
    assert_plain(&error.to_string());
    peer.finish();
}

#[test]
fn an_exit_signal_name_is_stripped_of_control_characters() {
    let _guard = exclusive();
    let peer = serve_exec(Behavior::default(), 1 << 20, |link, channel| {
        link.send(&message(
            CHANNEL_REQUEST,
            &[
                &number(channel),
                &text(b"exit-signal"),
                &[0],
                &text(HOSTILE_TEXT.as_bytes()),
                &[0],
                &text(b""),
                &text(b""),
            ],
        ));
        link.eof(channel);
        link.close(channel);
        link.expect_close();
    });
    let result = peer
        .connect(TIMEOUT)
        .exec("command", &ExecOptions::default())
        .expect("exec");
    match result.status {
        ExecStatus::Signaled(name) => assert_plain(&name),
        other => panic!("expected a signal, got {other:?}"),
    }
    peer.finish();
}

#[test]
fn rejected_authentication_methods_are_stripped_of_control_characters() {
    let _guard = exclusive();
    let peer = spawn(Behavior::default(), |link| {
        link.expect(SERVICE_REQUEST);
        link.send(&message(SERVICE_ACCEPT, &[&text(b"ssh-userauth")]));
        link.expect(USERAUTH_REQUEST);
        link.send(&message(USERAUTH_FAILURE, &[&text(HOSTILE_TEXT.as_bytes()), &[0]]));
    });
    let mut connection = Connection::connect(&peer.config(TIMEOUT)).expect("connect");
    let error = connection.authenticate().expect_err("rejected");
    assert!(matches!(error, Error::Authentication { .. }), "{error:?}");
    assert_plain(&error.to_string());
    peer.finish();
}

fn exec_reply_for_another_channel(reply: u8, with_environment: bool) -> Error {
    let peer = spawn(Behavior::default(), move |link| {
        link.accept_authentication();
        let open = link.expect(CHANNEL_OPEN);
        let client = open.u32_at(4 + open.u32_at(0) as usize);
        link.send(&message(
            CHANNEL_CONFIRMATION,
            &[&number(client), &number(SERVER_CHANNEL), &number(1 << 20), &number(32768)],
        ));
        link.expect(CHANNEL_REQUEST);
        link.send(&message(reply, &[&number(client + 1)]));
        if with_environment {
            link.send(&message(reply, &[&number(client + 1)]));
        }
    });
    let options = ExecOptions {
        env: if with_environment {
            vec![("LANG".to_owned(), "C".to_owned())]
        } else {
            Vec::new()
        },
        ..ExecOptions::default()
    };
    let error = peer
        .connect(TIMEOUT)
        .exec("command", &options)
        .expect_err("a reply for another channel is refused");
    peer.finish();
    error
}

#[test]
fn an_exec_reply_for_another_channel_is_refused() {
    let _guard = exclusive();
    for reply in [CHANNEL_SUCCESS, CHANNEL_FAILURE] {
        let error = exec_reply_for_another_channel(reply, false);
        assert!(error.to_string().contains("unknown channel"), "{error:?}");
    }
}

#[test]
fn an_env_reply_for_another_channel_is_refused() {
    let _guard = exclusive();
    for reply in [CHANNEL_SUCCESS, CHANNEL_FAILURE] {
        let error = exec_reply_for_another_channel(reply, true);
        assert!(error.to_string().contains("unknown channel"), "{error:?}");
    }
}

#[derive(Clone, Copy)]
enum SignBehavior {
    Hang,
    Drip,
}

fn stalling_agent(peer: &Peer, behavior: SignBehavior) -> (Identity, tempfile::TempDir) {
    use std::os::unix::net::UnixListener;

    let public = std::fs::read_to_string(peer.key.with_extension("pub")).expect("public key");
    let blob = data_encoding::BASE64
        .decode(public.split_whitespace().nth(1).expect("key field").as_bytes())
        .expect("base64");
    let fingerprint =
        xsh_ssh::host_key::fingerprint(&blob, xsh_ssh::DEFAULT_PROVIDER).expect("fingerprint");
    let directory = tempfile::tempdir().expect("tempdir");
    let socket = directory.path().join("agent.sock");
    let listener = UnixListener::bind(&socket).expect("bind");
    thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        loop {
            let mut length = [0u8; 4];
            if stream.read_exact(&mut length).is_err() {
                return;
            }
            let mut request = vec![0u8; u32::from_be_bytes(length) as usize];
            if stream.read_exact(&mut request).is_err() {
                return;
            }
            if request[0] == 11 {
                let mut body = vec![12u8];
                body.extend_from_slice(&1u32.to_be_bytes());
                body.extend_from_slice(&text(&blob));
                body.extend_from_slice(&text(b"comment"));
                let mut reply = (body.len() as u32).to_be_bytes().to_vec();
                reply.extend_from_slice(&body);
                if stream.write_all(&reply).is_err() {
                    return;
                }
                continue;
            }
            match behavior {
                SignBehavior::Hang => thread::sleep(Duration::from_secs(6)),
                SignBehavior::Drip => {
                    if stream.write_all(&100_000u32.to_be_bytes()).is_err() {
                        return;
                    }
                    for _ in 0..120 {
                        if stream.write_all(&[14]).is_err() {
                            return;
                        }
                        thread::sleep(Duration::from_millis(50));
                    }
                }
            }
            return;
        }
    });
    let identity = Identity::Agent {
        socket,
        fingerprint: xsh_ssh::Fingerprint::parse(&fingerprint).expect("fingerprint"),
    };
    (identity, directory)
}

fn authenticating_peer() -> Peer {
    spawn(Behavior::default(), |link| {
        link.accept_service();
        link.recv();
    })
}

#[test]
fn the_caller_can_cancel_authentication_while_the_agent_is_silent() {
    let _guard = exclusive();
    let peer = authenticating_peer();
    let (identity, _directory) = stalling_agent(&peer, SignBehavior::Hang);
    let cancelled = Arc::new(AtomicBool::new(false));
    let mut config = peer.config(Duration::from_secs(5));
    config.identity = identity;
    config.interrupt = Some({
        let cancelled = Arc::clone(&cancelled);
        Arc::new(move || cancelled.load(Ordering::SeqCst))
    });
    let mut connection = Connection::connect(&config).expect("connect");
    let flag = Arc::clone(&cancelled);
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(300));
        flag.store(true, Ordering::SeqCst);
    });
    let started = Instant::now();
    let (outcome, connection) = within(Duration::from_secs(10), move || {
        let outcome = connection.authenticate();
        (outcome, connection)
    });
    assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());
    let error = outcome.expect_err("cancelled");
    assert!(matches!(error, Error::Cancelled { certainty: Certainty::NotSubmitted }), "{error:?}");
    drop(connection);
    peer.finish();
}

#[test]
fn an_agent_that_drips_its_reply_cannot_hold_authentication_past_its_deadline() {
    let _guard = exclusive();
    let peer = authenticating_peer();
    let (identity, _directory) = stalling_agent(&peer, SignBehavior::Drip);
    let mut config = peer.config(Duration::from_millis(500));
    config.identity = identity;
    config.deadline = Duration::from_millis(1200);
    let mut connection = Connection::connect(&config).expect("connect");
    let started = Instant::now();
    let (outcome, connection) = within(Duration::from_secs(10), move || {
        let outcome = connection.authenticate();
        (outcome, connection)
    });
    assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
    let error = outcome.expect_err("deadline");
    assert!(matches!(error, Error::Timeout { certainty: Certainty::NotSubmitted }), "{error:?}");
    drop(connection);
    peer.finish();
}
