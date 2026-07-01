#![allow(clippy::single_call_fn)]

use std::io::Write;

fn main() {
    let mut bytes = Vec::new();
    for arg in std::env::args().skip(1) {
        let cleaned = arg
            .bytes()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect::<Vec<_>>();
        if cleaned.len() % 2 != 0 {
            eprintln!("hex input has an odd number of digits");
            std::process::exit(2);
        }
        let mut index = 0;
        while index < cleaned.len() {
            let Some(high) = nibble(cleaned[index]) else {
                eprintln!("invalid hex input");
                std::process::exit(2);
            };
            let Some(low) = nibble(cleaned[index + 1]) else {
                eprintln!("invalid hex input");
                std::process::exit(2);
            };
            bytes.push((high << 4) | low);
            index += 2;
        }
    }
    std::io::stdout().write_all(&bytes).expect("write stdout");
}

fn nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
