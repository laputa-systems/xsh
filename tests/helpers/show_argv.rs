#![allow(clippy::single_call_fn)]

use std::os::unix::ffi::OsStrExt;

fn main() {
    for arg in std::env::args_os().skip(1) {
        println!("{}", hex(arg.as_os_str().as_bytes()));
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
