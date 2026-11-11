use std::cmp::Ordering;
use std::fmt;
use std::io::IsTerminal;
use std::os::unix::fs::MetadataExt;
use std::time::{SystemTime, UNIX_EPOCH};

/// Native directory listing: `l` hides dotfiles, `ll` shows them.
/// Each entry is `name size age`; directories and symlinks omit the size.
/// Directories are blue, links cyan (bold red when broken), executables
/// bold green. Names keep their type color in a faint form once entries pass
/// a week old; ages fade through a grey ramp from one day. On a terminal,
/// names wrap to the available width in at most three lines and then end in
/// an ellipsis; other output keeps one line per entry. Appends output to
/// `stdout`. Returns 0 on success, 1 on error.
pub(super) fn list_dir(
    path: &str,
    stdout: &mut Vec<u8>,
    stderr: &mut Vec<u8>,
) -> i32 {
    list_dir_with_hidden(path, false, stdout, stderr)
}

pub(super) fn list_dir_with_hidden(
    path: &str,
    show_hidden: bool,
    stdout: &mut Vec<u8>,
    stderr: &mut Vec<u8>,
) -> i32 {
    use std::io::Write as _;
    let path = std::path::Path::new(path);
    let now = now_secs();
    let entries = match read_entries(path, show_hidden, now) {
        Ok(e) => e,
        Err(e) => {
            let cmd = if show_hidden { "ll" } else { "l" };
            writeln!(stderr, "xshi: {cmd}: {}: {e}", path.display()).ok();
            return 1;
        }
    };

    if entries.is_empty() {
        return 0;
    }

    let terminal_cols =
        std::io::stdout()
            .is_terminal()
            .then(|| usize::from(super::term::term_size().1));
    stdout.extend_from_slice(render(&entries, terminal_cols).as_bytes());
    0
}

const MAX_NAME_LINES: usize = 3;
const MIN_NAME_WIDTH: usize = 8;

fn render(entries: &[Entry], terminal_cols: Option<usize>) -> String {
    let prefix_width = |e: &Entry| {
        let mut width = str_width(&e.display_name);
        if let Some(ref target) = e.link_target {
            width += " -> ".len() + str_width(target);
        }
        width
    };
    let max_name_w = entries.iter().map(prefix_width).max().unwrap_or(0);
    let size_w = entries
        .iter()
        .filter_map(|e| e.size.as_ref())
        .map(Field::len)
        .max()
        .unwrap_or(0);
    let age_w = entries.iter().map(|e| e.age.len()).max().unwrap_or(0);
    let tail_w = 2 + if size_w > 0 { size_w + 2 } else { 0 } + age_w;
    let name_w = match terminal_cols {
        Some(cols) => max_name_w.min(cols.saturating_sub(tail_w).max(MIN_NAME_WIDTH)),
        None => max_name_w,
    };

    // One allocation for the whole listing: visible text plus slack for
    // separators, wrapping, and escape codes.
    let mut cap = entries.len() * (32 * MAX_NAME_LINES + 1);
    for e in entries {
        cap += e.display_name.len()
            + e.link_target.as_ref().map_or(0, |target| target.len() + 4)
            + e.size.as_ref().map_or(0, Field::len)
            + e.age.len()
            + e.name_start().len() * MAX_NAME_LINES
            + e.age_shade.len() * 2;
    }
    let mut out = String::with_capacity(cap);
    for e in entries {
        write_entry(&mut out, e, name_w, size_w, age_w);
    }
    out
}

fn write_entry(out: &mut String, e: &Entry, name_w: usize, size_w: usize, age_w: usize) {
    let name_start = e.name_start();
    let width = str_width(&e.display_name)
        + e.link_target
            .as_ref()
            .map_or(0, |target| " -> ".len() + str_width(target));
    if width <= name_w {
        out.push_str(name_start);
        out.push_str(&e.display_name);
        if !name_start.is_empty() {
            out.push_str("\x1b[0m");
        }
        if let Some(ref target) = e.link_target {
            out.push_str(" -> ");
            out.push_str(target);
        }
        for _ in width..name_w {
            out.push(' ');
        }
        write_metadata(out, e, size_w, age_w);
        out.push('\n');
        return;
    }

    let mut text = String::with_capacity(width);
    text.push_str(&e.display_name);
    if let Some(ref target) = e.link_target {
        text.push_str(" -> ");
        text.push_str(target);
    }
    let name_bytes = e.display_name.len();
    let wrapped = wrap_name(&text, name_w);
    for i in 0..wrapped.len {
        let (start, line) = wrapped.lines[i];
        let end = start + line.len();
        let colored_end = if start < name_bytes {
            end.min(name_bytes)
        } else {
            start
        };
        if start < colored_end {
            out.push_str(name_start);
            out.push_str(&text[start..colored_end]);
            if !name_start.is_empty() {
                out.push_str("\x1b[0m");
            }
        }
        if colored_end < end {
            out.push_str(&text[colored_end..end]);
        }
        if i + 1 == wrapped.len {
            let mut line_width = str_width(line);
            if wrapped.truncated {
                out.push('…');
                line_width += 1;
            }
            for _ in line_width..name_w {
                out.push(' ');
            }
            write_metadata(out, e, size_w, age_w);
        }
        out.push('\n');
    }
}

struct WrappedName<'a> {
    lines: [(usize, &'a str); MAX_NAME_LINES],
    len: usize,
    truncated: bool,
}

fn wrap_name(text: &str, width: usize) -> WrappedName<'_> {
    let mut wrapped = WrappedName {
        lines: [(0, ""); MAX_NAME_LINES],
        len: 0,
        truncated: false,
    };
    let mut start = 0;
    let mut used = 0;
    let mut after_joiner = false;
    for (i, ch) in text.char_indices() {
        let w = if after_joiner { 0 } else { char_width(ch) };
        after_joiner = ch == '\u{200D}';
        if used > 0 && used + w > width {
            if wrapped.len == MAX_NAME_LINES {
                let (line_start, line) = wrapped.lines[MAX_NAME_LINES - 1];
                wrapped.lines[MAX_NAME_LINES - 1] =
                    (line_start, truncate_to_width(line, width.saturating_sub(1)));
                wrapped.truncated = true;
                return wrapped;
            }
            wrapped.lines[wrapped.len] = (start, &text[start..i]);
            wrapped.len += 1;
            start = i;
            used = 0;
        }
        used += w;
    }
    if wrapped.len == 0 {
        wrapped.lines[0] = (start, &text[start..]);
        wrapped.len = 1;
    } else if start < text.len() {
        if wrapped.len == MAX_NAME_LINES {
            let (line_start, line) = wrapped.lines[MAX_NAME_LINES - 1];
            wrapped.lines[MAX_NAME_LINES - 1] =
                (line_start, truncate_to_width(line, width.saturating_sub(1)));
            wrapped.truncated = true;
        } else {
            wrapped.lines[wrapped.len] = (start, &text[start..]);
            wrapped.len += 1;
        }
    }
    wrapped
}

fn truncate_to_width(text: &str, max_width: usize) -> &str {
    let mut used = 0;
    let mut after_joiner = false;
    for (i, ch) in text.char_indices() {
        let w = if after_joiner { 0 } else { char_width(ch) };
        after_joiner = ch == '\u{200D}';
        if used + w > max_width {
            return &text[..i];
        }
        used += w;
    }
    text
}

/// Display width with the reference listing's rules: variation selectors and
/// emoji skin-tone modifiers occupy no columns, and neither does a character
/// joined to the previous one by ZERO WIDTH JOINER.
fn str_width(text: &str) -> usize {
    let mut width = 0;
    let mut after_joiner = false;
    for ch in text.chars() {
        if !after_joiner {
            width += char_width(ch);
        }
        after_joiner = ch == '\u{200D}';
    }
    width
}

fn char_width(ch: char) -> usize {
    match u32::from(ch) {
        0xFE00..=0xFE0F | 0x1F3FB..=0x1F3FF => 0,
        _ => super::line::char_width(ch),
    }
}

fn write_metadata(out: &mut String, e: &Entry, size_w: usize, age_w: usize) {
    out.push_str("  ");
    if size_w > 0 {
        if let Some(ref size) = e.size {
            for _ in size.len()..size_w {
                out.push(' ');
            }
            out.push_str(size.as_str());
        } else {
            for _ in 0..size_w {
                out.push(' ');
            }
        }
        out.push_str("  ");
    }
    for _ in e.age.len()..age_w {
        out.push(' ');
    }
    if !e.age_shade.is_empty() {
        out.push_str(e.age_shade);
    }
    out.push_str(e.age.as_str());
    if !e.age_shade.is_empty() {
        out.push_str("\x1b[0m");
    }
}

struct Entry {
    size: Option<Field>,
    age: Field,
    age_shade: &'static str,
    name_dim: bool,
    display_name: String,
    link_target: Option<String>,
    color_start: &'static str,
    dim_start: &'static str,
}

impl Entry {
    /// Old entries keep the color that identifies their kind, in its faint
    /// form; recent entries stay at full brightness.
    fn name_start(&self) -> &'static str {
        if self.name_dim {
            self.dim_start
        } else {
            self.color_start
        }
    }
}

/// Stack-allocated ASCII field for sizes and ages: formatted with no heap
/// use. All writers emit ASCII, so byte length is the display width.
#[derive(Debug)]
struct Field {
    buf: [u8; 24],
    len: u8,
}

impl Field {
    fn len(&self) -> usize {
        self.len as usize
    }

    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.buf[..self.len as usize]).expect("listing fields are ASCII")
    }
}

struct FieldWriter {
    buf: [u8; 24],
    len: usize,
}

impl fmt::Write for FieldWriter {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let bytes = text.as_bytes();
        if self.len + bytes.len() > self.buf.len() {
            return Err(fmt::Error);
        }
        self.buf[self.len..self.len + bytes.len()].copy_from_slice(bytes);
        self.len += bytes.len();
        Ok(())
    }
}

fn field(args: fmt::Arguments) -> Field {
    let mut out = FieldWriter {
        buf: [0; 24],
        len: 0,
    };
    fmt::write(&mut out, args).expect("listing field fits in 24 bytes");
    Field {
        buf: out.buf,
        len: out.len as u8,
    }
}

fn read_entries(
    path: &std::path::Path,
    show_hidden: bool,
    now: i64,
) -> Result<Vec<Entry>, std::io::Error> {
    let top_meta = std::fs::symlink_metadata(path)?;
    let top_is_dir = if top_meta.file_type().is_symlink() {
        std::fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false)
    } else {
        top_meta.is_dir()
    };
    if !top_is_dir {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        return Ok(vec![build_entry(&name, Some(path), &top_meta, now)]);
    }

    let mut entries = Vec::new();
    for dir_entry in std::fs::read_dir(path)? {
        let dir_entry = dir_entry?;
        let os_name = dir_entry.file_name();
        let name = os_name.to_string_lossy();

        if name == "." || name == ".." {
            continue;
        }
        if !show_hidden && name.starts_with('.') {
            continue;
        }

        let lmeta = match dir_entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let store;
        let for_link = if lmeta.file_type().is_symlink() {
            store = dir_entry.path();
            Some(store.as_path())
        } else {
            None
        };
        entries.push(build_entry(&name, for_link, &lmeta, now));
    }

    entries.sort_by(|a, b| caseless_cmp(&a.display_name, &b.display_name));

    Ok(entries)
}

/// Case-insensitive ordering without allocating: comparing the scalar streams
/// of `to_lowercase` matches comparing the lowered strings, since UTF-8 byte
/// order preserves scalar order.
fn caseless_cmp(a: &str, b: &str) -> Ordering {
    a.chars()
        .flat_map(char::to_lowercase)
        .cmp(b.chars().flat_map(char::to_lowercase))
}

fn build_entry(
    name: &str,
    full_path: Option<&std::path::Path>,
    lmeta: &std::fs::Metadata,
    now: i64,
) -> Entry {
    let mode = lmeta.mode();
    let is_link = lmeta.file_type().is_symlink();
    let target_meta = match full_path {
        Some(p) if is_link => std::fs::metadata(p).ok(),
        _ => None,
    };
    let is_dir = if is_link {
        target_meta.as_ref().is_some_and(|m| m.is_dir())
    } else {
        lmeta.is_dir()
    };
    let is_exec = !is_dir && mode & 0o111 != 0;

    let link_target = match full_path {
        Some(p) if is_link => std::fs::read_link(p)
            .ok()
            .map(|p| sanitize_name(&p.to_string_lossy())),
        _ => None,
    };

    let safe_name = sanitize_name(name);
    let display_name = if is_dir {
        format!("{safe_name}/")
    } else {
        safe_name
    };

    let (color_start, dim_start): (&'static str, &'static str) = if is_link && target_meta.is_none()
    {
        ("\x1b[1;31m", "\x1b[2;31m")
    } else if is_link {
        ("\x1b[36m", "\x1b[2;36m")
    } else if is_dir {
        ("\x1b[34m", "\x1b[2;34m")
    } else if mode & 0o4000 != 0 {
        ("\x1b[31m", "\x1b[2;31m")
    } else if is_exec {
        ("\x1b[1;32m", "\x1b[2;32m")
    } else {
        ("", "\x1b[2m")
    };

    let diff = age_diff(lmeta.mtime(), now);

    Entry {
        size: if lmeta.is_file() {
            Some(human_size(lmeta.len()))
        } else {
            None
        },
        age: relative_age_str(diff),
        age_shade: age_shade(diff),
        name_dim: name_dimmed(diff),
        display_name,
        link_target,
        color_start,
        dim_start,
    }
}

fn sanitize_name(name: &str) -> String {
    if name.bytes().all(|b| b >= b' ' && b != 0x7f) {
        return name.to_string();
    }
    name.chars()
        .map(|c| if c.is_control() { '\u{FFFD}' } else { c })
        .collect()
}

fn human_size(size: u64) -> Field {
    if size < 1024 {
        field(format_args!("{size}B"))
    } else if size < 1024 * 1024 {
        field(format_args!("{:.1}K", size as f64 / 1024.0))
    } else if size < 1024 * 1024 * 1024 {
        field(format_args!("{:.1}M", size as f64 / (1024.0 * 1024.0)))
    } else {
        field(format_args!(
            "{:.1}G",
            size as f64 / (1024.0 * 1024.0 * 1024.0)
        ))
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn age_diff(mtime: i64, now: i64) -> i64 {
    now.saturating_sub(mtime).max(0)
}

/// A binary faint step is much heavier than the age column's first greys,
/// so names stay at full brightness until entries are a week old.
fn name_dimmed(diff: i64) -> bool {
    const WEEK: i64 = 7 * 86400;
    diff >= WEEK
}

fn relative_age_str(diff: i64) -> Field {
    const MINUTE: i64 = 60;
    const HOUR: i64 = 3600;
    const DAY: i64 = 86400;
    const YEAR: i64 = 365 * DAY;
    if diff < MINUTE {
        field(format_args!("{diff}s"))
    } else if diff < HOUR {
        field(format_args!("{}m", diff / MINUTE))
    } else if diff < DAY {
        field(format_args!("{}h", diff / HOUR))
    } else if diff < YEAR {
        field(format_args!("{}d", diff / DAY))
    } else {
        field(format_args!("{}y", diff / YEAR))
    }
}

/// Grey ramp for the age column, keyed on absolute age. Files from the last
/// day keep the default color, then fade in small steps across the first
/// week; week-old files are already clearly grey and older files keep
/// fading toward dark grey, floored at a shade that stays readable on dark
/// terminals.
fn age_shade(diff: i64) -> &'static str {
    const DAY: i64 = 86400;
    if diff < DAY {
        ""
    } else if diff < 2 * DAY {
        "\x1b[38;5;252m"
    } else if diff < 4 * DAY {
        "\x1b[38;5;250m"
    } else if diff < 7 * DAY {
        "\x1b[38;5;248m"
    } else if diff < 30 * DAY {
        "\x1b[38;5;246m"
    } else if diff < 90 * DAY {
        "\x1b[38;5;242m"
    } else {
        "\x1b[38;5;240m"
    }
}

#[cfg(test)]
mod tests {
    use super::{age_shade, name_dimmed, read_entries, relative_age_str};
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn tempdir(files: &[&str]) -> std::path::PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("xshi_ls_test_{}_{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for f in files {
            std::fs::write(dir.join(f), "x").unwrap();
        }
        dir
    }

    fn relative_age(mtime: i64, now: i64) -> String {
        relative_age_str(super::age_diff(mtime, now)).as_str().to_owned()
    }

    #[test]
    fn entry_names_replace_terminal_controls() {
        assert_eq!(super::sanitize_name("plain name"), "plain name");
        assert_eq!(
            super::sanitize_name("line\nbreak\t\u{0085}"),
            "line\u{FFFD}break\u{FFFD}\u{FFFD}"
        );
    }

    #[test]
    fn relative_age_boundaries() {
        assert_eq!(relative_age(100, 100), "0s");
        assert_eq!(relative_age(200, 100), "0s");
        assert_eq!(relative_age(96, 100), "4s");
        assert_eq!(relative_age(0, 59), "59s");
        assert_eq!(relative_age(0, 60), "1m");
        assert_eq!(relative_age(0, 5 * 60), "5m");
        assert_eq!(relative_age(0, 3599), "59m");
        assert_eq!(relative_age(0, 3600), "1h");
        assert_eq!(relative_age(0, 2 * 3600), "2h");
        assert_eq!(relative_age(0, 86399), "23h");
        assert_eq!(relative_age(0, 86400), "1d");
        assert_eq!(relative_age(0, 3 * 86400), "3d");
        assert_eq!(relative_age(0, 40 * 86400), "40d");
        assert_eq!(relative_age(0, 364 * 86400), "364d");
        assert_eq!(relative_age(0, 365 * 86400), "1y");
        assert_eq!(relative_age(0, 800 * 86400), "2y");
    }

    #[test]
    fn hidden_entries_filter_on_flag() {
        let dir = tempdir(&["visible", ".hidden"]);
        let now = super::now_secs();
        let visible = read_entries(&dir, false, now).unwrap();
        assert_eq!(visible.len(), 1);
        assert!(visible[0].display_name.contains("visible"));
        let all = read_entries(&dir, true, now).unwrap();
        assert_eq!(all.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dirs_omit_size_files_keep_it() {
        let dir = tempdir(&["file.txt"]);
        std::fs::create_dir(dir.join("subdir")).unwrap();
        let entries = read_entries(&dir, false, super::now_secs()).unwrap();
        assert_eq!(entries.len(), 2);
        let file = entries
            .iter()
            .find(|e| e.display_name == "file.txt")
            .unwrap();
        assert!(file.size.is_some());
        let sub = entries
            .iter()
            .find(|e| e.display_name == "subdir/")
            .unwrap();
        assert!(sub.size.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn age_shade_steps() {
        const DAY: i64 = 86400;
        assert_eq!(age_shade(0), "");
        assert_eq!(age_shade(23 * 3600), "");
        assert_eq!(age_shade(DAY), "\x1b[38;5;252m");
        assert_eq!(age_shade(2 * DAY - 1), "\x1b[38;5;252m");
        assert_eq!(age_shade(2 * DAY), "\x1b[38;5;250m");
        assert_eq!(age_shade(4 * DAY - 1), "\x1b[38;5;250m");
        assert_eq!(age_shade(4 * DAY), "\x1b[38;5;248m");
        assert_eq!(age_shade(6 * DAY), "\x1b[38;5;248m");
        assert_eq!(age_shade(7 * DAY), "\x1b[38;5;246m");
        assert_eq!(age_shade(29 * DAY), "\x1b[38;5;246m");
        assert_eq!(age_shade(30 * DAY), "\x1b[38;5;242m");
        assert_eq!(age_shade(89 * DAY), "\x1b[38;5;242m");
        assert_eq!(age_shade(90 * DAY), "\x1b[38;5;240m");
        assert_eq!(age_shade(800 * DAY), "\x1b[38;5;240m");
    }

    #[test]
    fn names_dim_at_one_week() {
        const DAY: i64 = 86400;
        assert!(!name_dimmed(0));
        assert!(!name_dimmed(6 * DAY + 23 * 3600));
        assert!(name_dimmed(7 * DAY));
        assert!(name_dimmed(8 * DAY));
    }

    fn strip_ansi(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for c in chars.by_ref() {
                    if c == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    fn field(text: &str) -> super::Field {
        super::field(format_args!("{text}"))
    }

    fn entry(display_name: &str, size: Option<&str>, age: &str) -> super::Entry {
        super::Entry {
            size: size.map(field),
            age: field(age),
            age_shade: "",
            name_dim: false,
            display_name: display_name.to_owned(),
            link_target: None,
            color_start: "",
            dim_start: "",
        }
    }

    #[test]
    fn rendered_columns_align() {
        let entries = vec![
            entry("a.txt", Some("10B"), "2h"),
            entry("longname.txt", Some("10.6K"), "40d"),
            entry("d/", None, "3d"),
        ];
        let out = super::render(&entries, None);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 3);
        let visible: Vec<String> = lines.iter().map(|l| strip_ansi(l)).collect();
        assert_eq!(visible[1], "longname.txt  10.6K  40d");
        assert_eq!(visible[0], "a.txt           10B   2h");
        assert_eq!(visible[2], format!("d/{}3d", " ".repeat(20)));
        let widths: Vec<usize> = visible.iter().map(|l| super::str_width(l)).collect();
        assert!(
            widths.windows(2).all(|w| w[0] == w[1]),
            "columns should align: {widths:?}\n{out}"
        );
    }

    #[test]
    fn narrow_terminal_wraps_names_before_metadata() {
        let name = "abcdefghijklmnopqrstuvwxyz0123456789.txt";
        let entries = vec![
            entry(name, Some("1.0K"), "2h"),
            entry("short", Some("1B"), "3d"),
        ];
        let out = super::render(&entries, Some(40));
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], &name[..30]);
        assert!(lines[1].starts_with(&name[30..]));
        assert!(lines[1].ends_with("1.0K  2h"));
        assert!(lines[2].starts_with("short"));
        assert!(lines[2].ends_with("1B  3d"));
        for line in &lines {
            assert!(super::str_width(line) <= 40, "line exceeds terminal: {line:?}");
        }
    }

    #[test]
    fn names_truncate_after_three_wrapped_lines() {
        for len in [100, 200] {
            let name = "x".repeat(len);
            let entries = vec![entry(&name, Some("1B"), "2h")];
            let out = super::render(&entries, Some(40));
            let lines: Vec<&str> = out.lines().collect();
            assert_eq!(lines.len(), 3, "len={len}");
            assert_eq!(lines[0], "x".repeat(32));
            assert_eq!(lines[1], "x".repeat(32));
            assert!(lines[2].starts_with(&"x".repeat(31)));
            assert!(lines[2].contains('…'));
            for line in &lines {
                assert!(super::str_width(line) <= 40, "line exceeds terminal: {line:?}");
            }
        }
    }

    #[test]
    fn aged_names_keep_the_type_color_in_faint_form() {
        let mut old = entry("old", Some("1B"), "40d");
        old.color_start = "\x1b[1;32m";
        old.dim_start = "\x1b[2;32m";
        old.age_shade = "\x1b[38;5;246m";
        old.name_dim = true;
        let mut recent = entry("new", Some("1B"), "1d");
        recent.color_start = "\x1b[1;32m";
        recent.age_shade = "\x1b[38;5;252m";
        let out = super::render(&[old, recent], None);
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines[0].starts_with("\x1b[2;32mold\x1b[0m"));
        assert!(!lines[0].contains("\x1b[1;32m"));
        assert!(lines[1].starts_with("\x1b[1;32mnew\x1b[0m"));
    }
}
