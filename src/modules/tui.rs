use rustix::io as rio;
use rustix::termios::{self, LocalModes, OptionalActions};
use rustix::{fd::BorrowedFd, stdio};
use std::io::{self, Write};

pub(crate) fn read_secret(prompt: &str) -> io::Result<String> {
    let mut out = io::stdout().lock();
    write!(out, "{prompt}")?;
    out.flush()?;

    let stdin = stdio::stdin();
    let old_attrs = if termios::isatty(stdin) {
        termios::tcgetattr(stdin).ok()
    } else {
        None
    };
    if let Some(ref attrs) = old_attrs {
        let mut new_attrs = attrs.clone();
        new_attrs.local_modes &= !LocalModes::ECHO;
        let _ = termios::tcsetattr(stdin, OptionalActions::Now, &new_attrs);
    }

    let mut bytes = Vec::new();
    let read_result = read_line_bytes(stdin, &mut bytes);

    if let Some(ref attrs) = old_attrs {
        let _ = termios::tcsetattr(stdin, OptionalActions::Now, attrs);
        writeln!(out)?;
        out.flush()?;
    }

    read_result?;
    while matches!(bytes.last(), Some(b'\n' | b'\r')) {
        bytes.pop();
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn read_line_bytes(stdin: BorrowedFd<'_>, out: &mut Vec<u8>) -> io::Result<()> {
    loop {
        let mut byte = [0u8];
        match rio::read(stdin, &mut byte) {
            Ok(0) => return Ok(()),
            Ok(_) => {
                out.push(byte[0]);
                if byte[0] == b'\n' {
                    return Ok(());
                }
            }
            Err(err) if err == rio::Errno::INTR => continue,
            Err(err) => return Err(io::Error::from(err)),
        }
    }
}

pub(crate) fn sequence(name: Sequence) -> &'static str {
    match name {
        Sequence::Reset => "\x1b[0m",
        Sequence::Bold => "\x1b[1m",
        Sequence::Dim => "\x1b[2m",
        Sequence::Red => "\x1b[31m",
        Sequence::Green => "\x1b[32m",
        Sequence::Yellow => "\x1b[33m",
        Sequence::Blue => "\x1b[34m",
        Sequence::Magenta => "\x1b[35m",
        Sequence::Cyan => "\x1b[36m",
        Sequence::White => "\x1b[37m",
        Sequence::Gray => "\x1b[90m",
        Sequence::Clear => "\x1b[2J",
        Sequence::Home => "\x1b[H",
        Sequence::EraseLine => "\x1b[2K",
        Sequence::HideCursor => "\x1b[?25l",
        Sequence::ShowCursor => "\x1b[?25h",
    }
}

pub(crate) fn left_pad(text: &str, width: i64) -> String {
    pad(text, width, true)
}

pub(crate) fn right_pad(text: &str, width: i64) -> String {
    pad(text, width, false)
}

fn pad(text: &str, width: i64, left: bool) -> String {
    let width = width.max(0) as usize;
    let visible = visible_width(text);

    if visible >= width {
        return text.to_string();
    }

    let spaces = " ".repeat(width - visible);

    if left {
        format!("{spaces}{text}")
    } else {
        format!("{text}{spaces}")
    }
}

fn visible_width(text: &str) -> usize {
    let mut width = 0usize;
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for inner in chars.by_ref() {
                if ('@'..='~').contains(&inner) {
                    break;
                }
            }
        } else if ch != '\r' && ch != '\n' {
            width += 1;
        }
    }

    width
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Sequence {
    Reset,
    Bold,
    Dim,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    Gray,
    Clear,
    Home,
    EraseLine,
    HideCursor,
    ShowCursor,
}
