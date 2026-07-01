#![allow(clippy::single_call_fn)]

use super::session::{CwdEntry, Session};
use std::fs;
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub(super) fn run(
    session: &Session,
    args: &[String],
    stdout: &mut Vec<u8>,
    stderr: &mut Vec<u8>,
) -> i32 {
    let targets = if args.is_empty() {
        vec![session.cwd.clone()]
    } else {
        args.iter()
            .map(|arg| {
                let path = PathBuf::from(arg);
                if path.is_absolute() {
                    path
                } else {
                    session.cwd.join(path)
                }
            })
            .collect()
    };

    let mut status = 0;
    let mut file_entries = Vec::new();
    let mut dir_entries = Vec::new();
    let mut cached_now = None;
    if args.is_empty()
        && let Some(snapshot) = &session.cwd_snapshot
    {
        cached_now = Some(snapshot.captured_at);
        dir_entries.push((
            snapshot.path.clone(),
            snapshot
                .entries
                .iter()
                .map(ListEntry::from_cwd_entry)
                .collect(),
        ));
    } else {
        for path in targets {
            match fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.is_dir() => match read_dir_entries(&path) {
                    Ok(entries) => dir_entries.push((path, entries)),
                    Err(err) => {
                        writeln!(stderr, "l: {}: {err}", path.display()).ok();
                        status = 1;
                    }
                },
                Ok(metadata) => file_entries.push(ListEntry::new(path, metadata)),
                Err(err) => {
                    writeln!(stderr, "l: {}: {err}", path.display()).ok();
                    status = 1;
                }
            }
        }
    }

    file_entries.sort_unstable_by(|left, right| left.path.cmp(&right.path));
    let now = cached_now.unwrap_or_else(SystemTime::now);
    let widths = ListWidths::from_entries(
        session,
        file_entries
            .iter()
            .chain(dir_entries.iter().flat_map(|(_, entries)| entries.iter())),
        now,
    );
    for entry in file_entries {
        render_entry(session, &entry, &widths, now, stdout);
    }

    let show_headings = !dir_entries.is_empty() && (!args.is_empty() && args.len() > 1);
    for (index, (dir, mut entries)) in dir_entries.into_iter().enumerate() {
        if index > 0 || !stdout.is_empty() {
            stdout.push(b'\n');
        }
        if show_headings {
            writeln!(stdout, "{}:", dir.display()).ok();
        }
        entries.sort_unstable_by(|left, right| left.name.cmp(&right.name));
        for entry in entries {
            render_entry(session, &entry, &widths, now, stdout);
        }
    }
    status
}

#[derive(Clone, Debug)]
struct ListEntry {
    path: PathBuf,
    name: String,
    metadata: fs::Metadata,
    link_target: Option<PathBuf>,
}

impl ListEntry {
    fn new(path: PathBuf, metadata: fs::Metadata) -> Self {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let link_target = metadata
            .file_type()
            .is_symlink()
            .then(|| fs::read_link(&path).ok())
            .flatten();
        Self {
            path,
            name,
            metadata,
            link_target,
        }
    }
}

impl ListEntry {
    fn from_cwd_entry(entry: &CwdEntry) -> Self {
        Self {
            path: entry.path.clone(),
            name: entry.name.clone(),
            metadata: entry.metadata.clone(),
            link_target: entry.link_target.clone(),
        }
    }
}

fn read_dir_entries(path: &Path) -> Result<Vec<ListEntry>, std::io::Error> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name();
        if name.as_bytes() == b"." || name.as_bytes() == b".." {
            continue;
        }
        let path = entry.path();
        entries.push(ListEntry::new(path, fs::symlink_metadata(entry.path())?));
    }
    Ok(entries)
}

#[derive(Clone, Copy, Debug)]
struct ListWidths {
    owner: usize,
    size: usize,
    age: usize,
}

impl ListWidths {
    fn from_entries<'a>(
        session: &Session,
        entries: impl Iterator<Item = &'a ListEntry>,
        now: SystemTime,
    ) -> Self {
        let mut widths = Self {
            owner: 1,
            size: 1,
            age: 1,
        };
        for entry in entries {
            widths.owner = widths
                .owner
                .max(owner_name(session, entry.metadata.uid()).len());
            widths.size = widths.size.max(human_size(entry.metadata.len()).len());
            widths.age = widths
                .age
                .max(age(entry.metadata.modified().ok(), now).len());
        }
        widths
    }
}

fn render_entry(
    session: &Session,
    entry: &ListEntry,
    widths: &ListWidths,
    now: SystemTime,
    out: &mut Vec<u8>,
) {
    let mode = mode_string(&entry.metadata);
    let owner = owner_name(session, entry.metadata.uid());
    let size = human_size(entry.metadata.len());
    let age = age(entry.metadata.modified().ok(), now);
    let mut name = entry.name.clone();
    if entry.metadata.is_dir() {
        name.push('/');
    } else if is_executable(&entry.metadata)
        && !entry.metadata.file_type().is_symlink()
        && !session.colors
    {
        name.push('*');
    }
    let name = if session.colors {
        color_name(&name, &entry.metadata)
    } else {
        name
    };
    write!(
        out,
        "{mode} {owner:<owner_width$} {size:>size_width$} {age:>age_width$} {name}",
        owner_width = widths.owner,
        size_width = widths.size,
        age_width = widths.age,
    )
    .ok();
    if let Some(target) = &entry.link_target {
        write!(out, " -> {}", target.display()).ok();
    }
    out.push(b'\n');
}

fn owner_name(session: &Session, uid: u32) -> String {
    session
        .uid_names
        .get(&uid)
        .cloned()
        .unwrap_or_else(|| uid.to_string())
}

fn color_name(name: &str, metadata: &fs::Metadata) -> String {
    if metadata.file_type().is_symlink() {
        format!("\x1b[36m{name}\x1b[0m")
    } else if metadata.is_dir() {
        format!("\x1b[34m{name}\x1b[0m")
    } else if is_executable(metadata) {
        format!("\x1b[32m{name}\x1b[0m")
    } else {
        name.to_string()
    }
}

fn mode_string(metadata: &fs::Metadata) -> String {
    let file_type = metadata.file_type();
    let first = if file_type.is_dir() {
        'd'
    } else if file_type.is_symlink() {
        'l'
    } else if file_type.is_socket() {
        's'
    } else if file_type.is_fifo() {
        'p'
    } else if file_type.is_char_device() {
        'c'
    } else if file_type.is_block_device() {
        'b'
    } else {
        '-'
    };
    let mode = metadata.permissions().mode();
    let mut out = String::with_capacity(10);
    out.push(first);
    for shift in [6, 3, 0] {
        out.push(if mode & (0o4 << shift) != 0 { 'r' } else { '-' });
        out.push(if mode & (0o2 << shift) != 0 { 'w' } else { '-' });
        out.push(if mode & (0o1 << shift) != 0 { 'x' } else { '-' });
    }
    out
}

fn is_executable(metadata: &fs::Metadata) -> bool {
    metadata.permissions().mode() & 0o111 != 0
}

fn human_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "K", "M", "G", "T", "P"];
    let mut value = size as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{}{}", size, UNITS[unit])
    } else if value >= 10.0 {
        format!("{value:.0}{}", UNITS[unit])
    } else {
        format!("{value:.1}{}", UNITS[unit])
    }
}

fn age(modified: Option<SystemTime>, now: SystemTime) -> String {
    let Some(modified) = modified else {
        return "?".to_string();
    };
    let elapsed = now.duration_since(modified).unwrap_or(Duration::ZERO);
    let secs = elapsed.as_secs();
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else if secs < 2_592_000 {
        let days = secs / 86_400;
        let hours = (secs % 86_400) / 3600;
        if hours == 0 {
            format!("{days}d ago")
        } else {
            format!("{days}d {hours}h ago")
        }
    } else if secs < 31_536_000 {
        let months = secs / 2_592_000;
        let days = (secs % 2_592_000) / 86_400;
        if days == 0 {
            format!("{months}mo ago")
        } else {
            format!("{months}mo {days}d ago")
        }
    } else {
        format!("{}y ago", secs / 31_536_000)
    }
}
