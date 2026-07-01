#![allow(clippy::single_call_fn)]

use std::os::unix::ffi::OsStrExt;

fn main() {
    let mut missing = false;
    for name in std::env::args_os().skip(1) {
        match std::env::var_os(&name) {
            Some(value) => println!(
                "{}={}",
                String::from_utf8_lossy(name.as_os_str().as_bytes()),
                hex(value.as_os_str().as_bytes())
            ),
            None => {
                missing = true;
                eprintln!(
                    "{} is not set",
                    String::from_utf8_lossy(name.as_os_str().as_bytes())
                );
            }
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
