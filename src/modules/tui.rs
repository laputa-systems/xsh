use rustix::io as rio;
use rustix::termios::{self, LocalModes, OptionalActions};
use rustix::{fd::BorrowedFd, stdio};
use std::io::{self, Write};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub(crate) fn read_secret(prompt: &str) -> io::Result<String> {
    crate::startup_stdio::check(0)?;
    crate::startup_stdio::check(1)?;
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

pub(crate) fn left_pad(text: &str, width: i64) -> String {
    pad(text, width, true)
}

pub(crate) fn right_pad(text: &str, width: i64) -> String {
    pad(text, width, false)
}

// Cell-aware operations measure the sanitized display value, never terminal control text.
pub(crate) fn cell_width(text: &str) -> i64 {
    let safe = safe_single_line(text);
    i64::try_from(cell_text_width(&safe)).unwrap_or(i64::MAX)
}

pub(crate) fn cell_clip(text: &str, width: i64) -> String {
    let width = width.max(0) as usize;
    if width == 0 {
        return String::new();
    }

    let safe = safe_single_line(text);
    let mut clipped = String::new();
    let mut used = 0usize;
    let mut start = 0usize;
    while start < safe.len() {
        let end = next_cell_cluster_end(&safe, start);
        let cluster = &safe[start..end];
        let cluster_width = UnicodeWidthStr::width(cluster);
        if used.saturating_add(cluster_width) > width {
            break;
        }
        if cluster_width > 0 {
            clipped.push_str(cluster);
            used += cluster_width;
        }
        start = end;
    }
    clipped
}

pub(crate) fn cell_slice(text: &str, start: i64, width: i64) -> String {
    let start = start.max(0) as usize;
    let width = width.max(0) as usize;
    if width == 0 {
        return String::new();
    }

    let safe = safe_single_line(text);
    let slice_end = start.saturating_add(width);
    let mut cells = 0usize;
    let mut byte_start = 0usize;
    let mut slice = String::new();
    while byte_start < safe.len() && cells < slice_end {
        let byte_end = next_cell_cluster_end(&safe, byte_start);
        let cluster = &safe[byte_start..byte_end];
        let cluster_width = UnicodeWidthStr::width(cluster);
        if cluster_width == 0 {
            byte_start = byte_end;
            continue;
        }

        let cluster_end = cells.saturating_add(cluster_width);
        let overlap_start = cells.max(start);
        let overlap_end = cluster_end.min(slice_end);
        if overlap_start < overlap_end {
            if cells >= start && cluster_end <= slice_end {
                slice.push_str(cluster);
            } else {
                slice.push_str(&" ".repeat(overlap_end - overlap_start));
            }
        }
        cells = cluster_end;
        byte_start = byte_end;
    }
    slice
}

pub(crate) fn cell_left_pad(text: &str, width: i64) -> String {
    pad_cells(text, width, true)
}

pub(crate) fn cell_right_pad(text: &str, width: i64) -> String {
    pad_cells(text, width, false)
}

pub(crate) fn safe_single_line(text: &str) -> String {
    let mut safe = String::with_capacity(text.len());
    let mut has_base = false;
    for ch in text.chars() {
        if ch.is_control() || matches!(ch, '\u{2028}' | '\u{2029}') {
            safe.push(' ');
            has_base = true;
        } else if is_bidi_control(ch) || (UnicodeWidthChar::width(ch) == Some(0) && !has_base) {
            safe.push('\u{fffd}');
            has_base = true;
        } else {
            safe.push(ch);
            if UnicodeWidthChar::width(ch).is_some_and(|width| width > 0) {
                has_base = true;
            }
        }
    }
    safe
}

fn pad_cells(text: &str, width: i64, left: bool) -> String {
    let safe = safe_single_line(text);
    let width = width.max(0) as usize;
    let visible = cell_text_width(&safe);
    if visible >= width {
        return safe;
    }
    let spaces = " ".repeat(width - visible);
    if left {
        format!("{spaces}{safe}")
    } else {
        format!("{safe}{spaces}")
    }
}

fn cell_text_width(text: &str) -> usize {
    let mut width = 0usize;
    let mut start = 0usize;
    while start < text.len() {
        let end = next_cell_cluster_end(text, start);
        width = width.saturating_add(UnicodeWidthStr::width(&text[start..end]));
        start = end;
    }
    width
}

// Keep common combining and emoji sequences intact when a cell boundary falls inside them.
fn next_cell_cluster_end(text: &str, start: usize) -> usize {
    let mut chars = text[start..].char_indices();
    let (_, first) = chars.next().expect("cluster starts at a character boundary");
    let mut end = start + first.len_utf8();
    let mut join_next_emoji = false;
    let mut regional_indicators = usize::from(is_regional_indicator(first));
    let mut has_emoji_base = is_emoji_base(first);

    for (offset, ch) in chars {
        let zero_width = ch != '\u{200d}' && UnicodeWidthChar::width(ch) == Some(0);
        let joined_emoji = if join_next_emoji {
            is_emoji_base(ch)
        } else {
            false
        };
        let emoji_joiner = ch == '\u{200d}'
            && has_emoji_base
            && chars_have_emoji_base(text, start + offset + ch.len_utf8());
        let emoji_modifier = is_emoji_modifier(ch) && has_emoji_base;
        let regional_pair = is_regional_indicator(ch) && regional_indicators == 1;
        if !(zero_width || joined_emoji || emoji_joiner || emoji_modifier || regional_pair) {
            break;
        }

        end = start + offset + ch.len_utf8();
        if ch == '\u{200d}' {
            join_next_emoji = true;
        } else if joined_emoji {
            join_next_emoji = false;
        }
        if is_regional_indicator(ch) {
            regional_indicators += 1;
        }
        has_emoji_base |= is_emoji_base(ch);
    }
    end
}

fn chars_have_emoji_base(text: &str, start: usize) -> bool {
    text[start..]
        .chars()
        .find(|ch| UnicodeWidthChar::width(*ch) != Some(0))
        .is_some_and(is_emoji_base)
}

fn is_emoji_base(ch: char) -> bool {
    matches!(
        ch as u32,
        0x2300..=0x23ff | 0x2600..=0x27ff | 0x1f000..=0x1faff
    )
}

fn is_emoji_modifier(ch: char) -> bool {
    matches!(ch as u32, 0x1f3fb..=0x1f3ff)
}

fn is_regional_indicator(ch: char) -> bool {
    matches!(ch as u32, 0x1f1e6..=0x1f1ff)
}

fn is_bidi_control(ch: char) -> bool {
    matches!(
        ch as u32,
        0x061c | 0x200e..=0x200f | 0x202a..=0x202e | 0x2066..=0x206f
    )
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
