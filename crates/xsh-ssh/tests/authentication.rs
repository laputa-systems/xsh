mod support;

use std::time::Duration;

use support::*;
use xsh_ssh::Connection;

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
