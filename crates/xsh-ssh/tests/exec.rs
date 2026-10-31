use std::net::TcpListener;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

use xsh_ssh::{Certainty, ClientConfig, Connection, Error, ExecOptions, ExecStatus};

struct Fixture {
    host: String,
    port: u16,
    pin: String,
    user: String,
    key: String,
}

fn variable(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| {
        panic!("{name} is not set; run this test through dev/fixtures.xsh exec")
    })
}

fn fixture() -> Fixture {
    Fixture {
        host: variable("XINFRA_FIXTURE_A_HOST"),
        port: variable("XINFRA_FIXTURE_A_PORT")
            .parse()
            .expect("fixture port is a number"),
        pin: variable("XINFRA_FIXTURE_A_HOST_KEY"),
        user: variable("XINFRA_FIXTURE_USER"),
        key: variable("XINFRA_FIXTURE_KEY"),
    }
}

fn config(fixture: &Fixture, user: &str, key: &Path) -> ClientConfig {
    ClientConfig::new(
        fixture.host.clone(),
        fixture.port,
        user,
        key,
        &fixture.pin,
        Duration::from_secs(10),
    )
    .expect("fixture pin is valid")
}

fn authenticated(fixture: &Fixture) -> Connection {
    let mut connection = Connection::connect(&config(
        fixture,
        &fixture.user,
        Path::new(&fixture.key),
    ))
    .expect("connection succeeds");
    connection.authenticate().expect("fixture key authenticates");
    connection
}

fn generate_key(directory: &Path, name: &str, passphrase: &str) -> std::path::PathBuf {
    let path = directory.join(name);
    let status = Command::new("ssh-keygen")
        .args([
            "-q",
            "-t",
            "ed25519",
            "-N",
            passphrase,
            "-C",
            "xsh-ssh-test",
            "-f",
        ])
        .arg(&path)
        .status()
        .expect("ssh-keygen runs");
    assert!(status.success(), "ssh-keygen failed");
    path
}

#[test]
fn exact_stdout_stderr_and_status() {
    let fixture = fixture();
    let mut connection = authenticated(&fixture);
    let result = connection
        .exec(
            "printf 'out'; printf 'err' >&2; exit 7",
            &ExecOptions::default(),
        )
        .expect("exec succeeds");
    assert_eq!(result.status, ExecStatus::Exited(7));
    assert_eq!(result.stdout, b"out");
    assert_eq!(result.stderr, b"err");
    assert_eq!(result.certainty, Certainty::Submitted);
}

#[test]
fn binary_output_is_exact() {
    let fixture = fixture();
    let mut connection = authenticated(&fixture);
    let result = connection
        .exec("printf 'A\\000B'", &ExecOptions::default())
        .expect("exec succeeds");
    assert_eq!(result.status, ExecStatus::Exited(0));
    assert_eq!(result.stdout, b"A\0B");
    assert!(result.stderr.is_empty());
}

#[test]
fn remote_zero_status_is_data() {
    let fixture = fixture();
    let mut connection = authenticated(&fixture);
    let result = connection
        .exec("true", &ExecOptions::default())
        .expect("exec succeeds");
    assert_eq!(result.status, ExecStatus::Exited(0));
}

#[test]
fn signal_is_an_outcome() {
    let fixture = fixture();
    let mut connection = authenticated(&fixture);
    let result = connection
        .exec("kill -TERM $$", &ExecOptions::default())
        .expect("exec succeeds");
    assert_eq!(result.status, ExecStatus::Signaled("TERM".to_owned()));
}

#[test]
fn wrong_key_is_rejected() {
    let fixture = fixture();
    let directory = tempfile::tempdir().expect("tempdir");
    let key = generate_key(directory.path(), "wrong", "");
    let mut connection = Connection::connect(&config(&fixture, &fixture.user, &key))
        .expect("connection succeeds");
    assert!(matches!(
        connection.authenticate(),
        Err(Error::Authentication { .. })
    ));
}

#[test]
fn unknown_user_is_rejected() {
    let fixture = fixture();
    let mut connection = Connection::connect(&config(
        &fixture,
        "xinfra-no-such-user",
        Path::new(&fixture.key),
    ))
    .expect("connection succeeds");
    assert!(matches!(
        connection.authenticate(),
        Err(Error::Authentication { .. })
    ));
}

#[test]
fn encrypted_key_is_rejected_explicitly() {
    let fixture = fixture();
    let directory = tempfile::tempdir().expect("tempdir");
    let key = generate_key(directory.path(), "encrypted", "secret");
    let mut connection = Connection::connect(&config(&fixture, &fixture.user, &key))
        .expect("connection succeeds");
    assert!(matches!(connection.authenticate(), Err(Error::EncryptedKey)));
}

#[test]
fn timeout_after_submission_is_certain() {
    let fixture = fixture();
    let mut connection = authenticated(&fixture);
    let options = ExecOptions {
        timeout: Some(Duration::from_millis(300)),
        ..ExecOptions::default()
    };
    match connection.exec("sleep 5", &options) {
        Err(Error::Timeout {
            certainty: Certainty::Submitted,
        }) => {}
        other => panic!("expected a submitted timeout, got {other:?}"),
    }
}

#[test]
fn timeout_before_submission_is_certain() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
    let port = listener.local_addr().expect("address").port();
    thread::spawn(move || {
        if let Ok((_stream, _address)) = listener.accept() {
            thread::sleep(Duration::from_secs(2));
        }
    });
    let fixture = fixture();
    let config = ClientConfig::new(
        "127.0.0.1",
        port,
        &fixture.user,
        &fixture.key,
        &fixture.pin,
        Duration::from_millis(200),
    )
    .expect("pin");
    match Connection::connect(&config) {
        Err(Error::Timeout {
            certainty: Certainty::NotSubmitted,
        }) => {}
        other => panic!("expected a pre-submission timeout, got {other:?}"),
    }
}
