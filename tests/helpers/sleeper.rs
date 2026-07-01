#![allow(clippy::single_call_fn)]

use std::time::Duration;

fn main() {
    if let Some(marker) = std::env::args_os().nth(1) {
        std::fs::write(marker, b"ready").expect("write readiness marker");
    }

    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}
