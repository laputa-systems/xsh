use std::time::Duration;

use xsh_ssh::{ClientConfig, Connection, Error, HostKeyPin};

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

fn fixture(suffix: &str) -> Fixture {
    Fixture {
        host: variable(&format!("XINFRA_FIXTURE_{suffix}_HOST")),
        port: variable(&format!("XINFRA_FIXTURE_{suffix}_PORT"))
            .parse()
            .expect("fixture port is a number"),
        pin: variable(&format!("XINFRA_FIXTURE_{suffix}_HOST_KEY")),
        user: variable("XINFRA_FIXTURE_USER"),
        key: variable("XINFRA_FIXTURE_KEY"),
    }
}

fn config(fixture: &Fixture, pin: &str) -> ClientConfig {
    ClientConfig::new(
        fixture.host.clone(),
        fixture.port,
        fixture.user.clone(),
        fixture.key.clone(),
        pin,
        Duration::from_secs(10),
    )
    .expect("test pin is valid")
}

#[test]
fn correct_pin_connects_to_both_fixtures() {
    for suffix in ["A", "B"] {
        let fixture = fixture(suffix);
        let connection = Connection::connect(&config(&fixture, &fixture.pin))
            .expect("pinned connection succeeds");
        assert_eq!(connection.fingerprint(), fixture.pin);
        assert!(connection.strict(), "strict key exchange is mandatory");
        assert!(!connection.session_id().is_empty());
        assert!(!connection.host_key_blob().is_empty());
    }
}

#[test]
fn another_fixture_pin_is_rejected() {
    let a = fixture("A");
    let b = fixture("B");
    match Connection::connect(&config(&a, &b.pin)) {
        Err(Error::HostKeyMismatch { expected, actual }) => {
            assert_eq!(expected, b.pin);
            assert_eq!(actual, a.pin);
        }
        Ok(_) => panic!("expected a host key mismatch, but the connection succeeded"),
        Err(other) => panic!("expected a host key mismatch, got {other:?}"),
    }
}

#[test]
fn changed_pin_is_rejected() {
    let fixture = fixture("A");
    let mut changed = fixture.pin.clone();
    let last = changed.pop().expect("pin has a fingerprint");
    changed.push(if last == 'A' { 'B' } else { 'A' });
    match Connection::connect(&config(&fixture, &changed)) {
        Err(Error::HostKeyMismatch { actual, .. }) => assert_eq!(actual, fixture.pin),
        Ok(_) => panic!("expected a host key mismatch, but the connection succeeded"),
        Err(other) => panic!("expected a host key mismatch, got {other:?}"),
    }
}

#[test]
fn missing_pin_is_rejected() {
    assert!(matches!(
        HostKeyPin::parse(""),
        Err(Error::InvalidHostKeyPin { .. })
    ));
}

#[test]
fn malformed_pin_is_rejected() {
    assert!(matches!(
        HostKeyPin::parse("SHA256:not-base64"),
        Err(Error::InvalidHostKeyPin { .. })
    ));
    assert!(matches!(
        HostKeyPin::parse("sha256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
        Err(Error::InvalidHostKeyPin { .. })
    ));
}
