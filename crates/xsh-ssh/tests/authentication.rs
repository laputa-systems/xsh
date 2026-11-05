mod support;

use std::time::Duration;

use support::*;
use xsh_ssh::{Connection, Error};

const TIMEOUT: Duration = Duration::from_secs(5);
const DEBUG: u8 = 4;
const UNIMPLEMENTED: u8 = 3;

fn transport_noise() -> Vec<Vec<u8>> {
    vec![
        message(IGNORE, &[&text(b"padding")]),
        message(DEBUG, &[&[1], &text(b"note"), &text(b"")]),
        message(UNIMPLEMENTED, &[&number(0)]),
    ]
}

#[test]
fn transport_messages_before_the_service_accept_do_not_break_authentication() {
    for noise in transport_noise() {
        let peer = spawn(Behavior::default(), move |link| {
            link.expect(SERVICE_REQUEST);
            link.send(&noise);
            link.send(&message(SERVICE_ACCEPT, &[&text(b"ssh-userauth")]));
            link.expect(USERAUTH_REQUEST);
            link.send(&[USERAUTH_SUCCESS]);
        });
        let mut connection = Connection::connect(&peer.config(TIMEOUT)).expect("connect");
        connection.authenticate().expect("noise is skipped");
        drop(connection);
        peer.finish();
    }
}

#[test]
fn transport_messages_while_the_server_decides_do_not_break_authentication() {
    for noise in transport_noise() {
        let peer = spawn(Behavior::default(), move |link| {
            link.accept_service();
            link.expect(USERAUTH_REQUEST);
            link.send(&noise);
            link.send(&[USERAUTH_SUCCESS]);
        });
        let mut connection = Connection::connect(&peer.config(TIMEOUT)).expect("connect");
        connection.authenticate().expect("noise is skipped");
        drop(connection);
        peer.finish();
    }
}

#[test]
fn a_service_accept_for_another_service_is_refused() {
    let peer = spawn(Behavior::default(), |link| {
        link.expect(SERVICE_REQUEST);
        link.send(&message(SERVICE_ACCEPT, &[&text(b"ssh-connection")]));
    });
    let mut connection = Connection::connect(&peer.config(TIMEOUT)).expect("connect");
    match connection.authenticate() {
        Err(Error::Protocol { detail }) => assert!(detail.contains("different service"), "{detail}"),
        other => panic!("expected a protocol failure, got {other:?}"),
    }
    drop(connection);
    peer.finish();
}

fn authenticate_with_key_at(prepare: impl FnOnce(&std::path::Path)) -> Result<(), Error> {
    let peer = spawn(Behavior::default(), |link| link.linger());
    prepare(&peer.key);
    let mut connection = Connection::connect(&peer.config(TIMEOUT)).expect("connect");
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let outcome = connection.authenticate();
        let _ = sender.send(outcome);
    });
    let outcome = receiver
        .recv_timeout(Duration::from_secs(3))
        .expect("authentication returns instead of blocking on the key file");
    drop(peer);
    outcome
}

fn key_file_failure(outcome: Result<(), Error>) -> String {
    match outcome {
        Err(Error::KeyFile { detail }) => detail,
        other => panic!("expected a key file failure, got {other:?}"),
    }
}

#[test]
fn a_private_key_readable_by_others_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    for mode in [0o640, 0o604, 0o644, 0o660] {
        let detail = key_file_failure(authenticate_with_key_at(|path| {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("chmod");
        }));
        assert!(detail.contains("too open"), "{mode:o}: {detail}");
    }
}

#[test]
fn a_fifo_at_the_key_path_is_refused_without_blocking() {
    let detail = key_file_failure(authenticate_with_key_at(|path| {
        std::fs::remove_file(path).expect("remove key");
        let status = std::process::Command::new("mkfifo").arg(path).status().expect("mkfifo");
        assert!(status.success());
    }));
    assert!(detail.contains("not a regular file"), "{detail}");
}

#[test]
fn a_key_file_past_the_size_bound_is_refused() {
    let detail = key_file_failure(authenticate_with_key_at(|path| {
        std::fs::write(path, vec![b'a'; 1 << 20]).expect("write");
    }));
    assert!(detail.contains("larger than"), "{detail}");
}
