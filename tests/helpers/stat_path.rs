#![allow(clippy::single_call_fn)]

use std::os::unix::ffi::OsStrExt;

fn main() {
    let mut missing = false;
    for path in std::env::args_os().skip(1) {
        if std::fs::symlink_metadata(&path).is_ok() {
            println!("{}", hex(path.as_os_str().as_bytes()));
        } else {
            missing = true;
            eprintln!(
                "missing {}",
                String::from_utf8_lossy(path.as_os_str().as_bytes())
            );
        }
    }
    if missing {
        std::process::exit(1);
    }
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    output
}
