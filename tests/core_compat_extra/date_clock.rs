#![cfg(target_os = "linux")]

use super::{applet, cp::require_disposable_qemu_guest, Running};
use std::sync::Mutex;

static CLOCK_SET: Mutex<()> = Mutex::new(());

fn set_date(input: &str, expected: &str) {
    require_disposable_qemu_guest();
    let _clock = CLOCK_SET.lock().expect("serialize realtime changes in the guest");
    let directory = tempfile::tempdir().unwrap();
    let mut command = applet("date", directory.path());
    command.args(["--set", input]).env("TZ", "UTC0");
    let output = Running::spawn(&mut command).finish();
    assert_eq!(output.status.code(), Some(0), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, expected.as_bytes());
    assert!(output.stderr.is_empty());
}

// These cases change CLOCK_REALTIME. Keep them ignored in ordinary gates and
// require the disposable guest identity before starting the applet.
// origin: uutils test_date::test_date_set_valid
#[test]
#[ignore = "requires a disposable QEMU guest"]
fn test_date_set_valid() {
    set_date("2020-03-12 13:30:00+08:00", "Thu Mar 12 05:30:00 UTC 2020\n");
}

// origin: uutils test_date::test_date_set_valid_3
#[test]
#[ignore = "requires a disposable QEMU guest"]
fn test_date_set_valid_3() {
    set_date("Sat 20 Mar 2021 14:53:01", "Sat Mar 20 14:53:01 UTC 2021\n");
}

// origin: uutils test_date::test_date_set_valid_4
#[test]
#[ignore = "requires a disposable QEMU guest"]
fn test_date_set_valid_4() {
    set_date("2020-03-11 21:45:00", "Wed Mar 11 21:45:00 UTC 2020\n");
}
