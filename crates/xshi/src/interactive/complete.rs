#![allow(clippy::single_call_fn)]

use super::session::Session;
use rustix::{
    event as revent, fs as rfs, io as rio, pipe as rpipe, process as rprocess, time as rtime,
};
use std::ffi::CString;
use std::fs;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub(super) struct CompEntry {
    mtime: i64,
    name_start: u32,
    name_len: u8,
    name_display_width: u8,
    flags: u8,
}

impl CompEntry {
    pub(super) fn display_width(&self) -> usize {
        self.name_display_width as usize
            + if self.is_dir() || self.is_host() {
                1
            } else {
                0
            }
    }

    pub(super) fn is_dir(&self) -> bool {
        self.flags & 1 != 0
    }

    pub(super) fn is_link(&self) -> bool {
        self.flags & 2 != 0
    }

    pub(super) fn is_exec(&self) -> bool {
        self.flags & 4 != 0
    }

    pub(super) fn is_host(&self) -> bool {
        self.flags & 8 != 0
    }
}

fn pack_flags(is_dir: bool, is_link: bool, is_exec: bool) -> u8 {
    (is_dir as u8) | ((is_link as u8) << 1) | ((is_exec as u8) << 2)
}

#[derive(Clone, Debug, Default)]
pub(super) struct Completions {
    pub(super) names: String,
    pub(super) entries: Vec<CompEntry>,
}

impl Completions {
    pub(super) fn new() -> Self {
        Self {
            names: String::new(),
            entries: Vec::new(),
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(super) fn name(&self, idx: usize) -> &str {
        let entry = &self.entries[idx];
        &self.names[entry.name_start as usize..][..entry.name_len as usize]
    }

    pub(super) fn entry_name(&self, entry: &CompEntry) -> &str {
        &self.names[entry.name_start as usize..][..entry.name_len as usize]
    }

    pub(super) fn push(&mut self, name: &str, is_dir: bool, is_link: bool, is_exec: bool) {
        self.push_with_mtime(name, is_dir, is_link, is_exec, 0);
    }

    fn push_host(&mut self, name: &str) {
        let start = self.names.len() as u32;
        self.names.push_str(name);
        self.entries.push(CompEntry {
            mtime: 0,
            name_start: start,
            name_len: name.len().min(255) as u8,
            name_display_width: str_width(name).min(255) as u8,
            flags: 8,
        });
    }

    fn push_with_mtime(
        &mut self,
        name: &str,
        is_dir: bool,
        is_link: bool,
        is_exec: bool,
        mtime: i64,
    ) {
        let start = self.names.len() as u32;
        self.names.push_str(name);
        self.entries.push(CompEntry {
            mtime,
            name_start: start,
            name_len: name.len().min(255) as u8,
            name_display_width: str_width(name).min(255) as u8,
            flags: pack_flags(is_dir, is_link, is_exec),
        });
    }

    pub(super) fn begin_entry(&self) -> u32 {
        self.names.len() as u32
    }

    pub(super) fn finish_entry(&mut self, start: u32, is_dir: bool, is_link: bool, is_exec: bool) {
        let name = &self.names[start as usize..];
        self.entries.push(CompEntry {
            mtime: 0,
            name_start: start,
            name_len: name.len().min(255) as u8,
            name_display_width: str_width(name).min(255) as u8,
            flags: pack_flags(is_dir, is_link, is_exec),
        });
    }

    pub(super) fn sort_entries(&mut self) {
        let names = self.names.as_bytes();
        self.entries
            .sort_unstable_by(|a, b| cmp_icase_arena(names, a, b));
    }

    fn sort_by_mtime(&mut self) {
        let names = self.names.as_bytes();
        self.entries.sort_unstable_by(|a, b| {
            b.mtime
                .cmp(&a.mtime)
                .then_with(|| cmp_icase_arena(names, a, b))
        });
    }

    pub(super) fn dedup_sorted(&mut self) {
        let mut i = 1;
        while i < self.entries.len() {
            let prev = &self.entries[i - 1];
            let curr = &self.entries[i];
            if self.names[prev.name_start as usize..][..prev.name_len as usize]
                == self.names[curr.name_start as usize..][..curr.name_len as usize]
            {
                self.entries.remove(i);
            } else {
                i += 1;
            }
        }
    }
}

fn cmp_icase_arena(names: &[u8], a: &CompEntry, b: &CompEntry) -> std::cmp::Ordering {
    let a_bytes = &names[a.name_start as usize..][..a.name_len as usize];
    let b_bytes = &names[b.name_start as usize..][..b.name_len as usize];
    let len = a_bytes.len().min(b_bytes.len());
    for i in 0..len {
        let ab = a_bytes[i].to_ascii_lowercase();
        let bb = b_bytes[i].to_ascii_lowercase();
        if ab != bb {
            return ab.cmp(&bb);
        }
    }
    a_bytes.len().cmp(&b_bytes.len())
}

#[derive(Clone, Debug, Default)]
pub(super) struct CompletionState {
    pub(super) comp: Completions,
    pub(super) selected: usize,
    pub(super) cols: usize,
    pub(super) rows: usize,
    pub(super) scroll: usize,
    pub(super) term_cols: u16,
    pub(super) dir_prefix: String,
    pub(super) in_quote: bool,
}

impl CompletionState {
    pub(super) fn selected_name(&self) -> Option<&str> {
        (self.selected < self.comp.len()).then(|| self.comp.name(self.selected))
    }

    pub(super) fn selected_entry(&self) -> Option<&CompEntry> {
        self.comp.entries.get(self.selected)
    }

    pub(super) fn move_up(&mut self) {
        if self.rows == 0 || self.comp.entries.is_empty() {
            return;
        }
        if self.selected >= self.comp.entries.len() {
            self.selected = self.comp.entries.len() - 1;
            return;
        }
        let row = self.selected % self.rows;
        let col = self.selected / self.rows;
        if row == 0 {
            if col > 0 {
                let prev_col = col - 1;
                let idx = prev_col * self.rows + self.rows - 1;
                self.selected = idx.min(self.comp.entries.len() - 1);
            } else {
                let last_col = (self.comp.entries.len().saturating_sub(1)) / self.rows;
                let idx = last_col * self.rows + self.rows - 1;
                self.selected = idx.min(self.comp.entries.len() - 1);
            }
        } else {
            self.selected -= 1;
        }
    }

    pub(super) fn move_down(&mut self) {
        if self.rows == 0 || self.comp.entries.is_empty() {
            return;
        }
        if self.selected >= self.comp.entries.len() {
            self.selected = 0;
            return;
        }
        let row = self.selected % self.rows;
        let col = self.selected / self.rows;
        if row + 1 >= self.rows || self.selected + 1 >= self.comp.entries.len() {
            let next_col = col + 1;
            let idx = next_col * self.rows;
            if idx < self.comp.entries.len() {
                self.selected = idx;
            } else {
                self.selected = 0;
            }
        } else {
            self.selected += 1;
        }
    }

    pub(super) fn move_left(&mut self) {
        if self.rows == 0 || self.comp.entries.is_empty() {
            return;
        }
        if self.selected >= self.comp.entries.len() {
            self.selected = self.comp.entries.len() - 1;
            return;
        }
        let col = self.selected / self.rows;
        let row = self.selected % self.rows;
        if col == 0 {
            let last_col = (self.comp.entries.len().saturating_sub(1)) / self.rows;
            let idx = last_col * self.rows + row;
            self.selected = idx.min(self.comp.entries.len() - 1);
        } else {
            self.selected -= self.rows;
        }
    }

    pub(super) fn move_right(&mut self) {
        if self.rows == 0 || self.comp.entries.is_empty() {
            return;
        }
        if self.selected >= self.comp.entries.len() {
            self.selected = 0;
            return;
        }
        let row = self.selected % self.rows;
        let next = (self.selected / self.rows + 1) * self.rows + row;
        if next < self.comp.entries.len() {
            self.selected = next;
        } else {
            self.selected = row.min(self.comp.entries.len() - 1);
        }
    }
}

pub(super) struct CompletionRequest<'a> {
    pub(super) text: &'a str,
    pub(super) cursor: usize,
    pub(super) term_cols: u16,
}

pub(super) fn start_completion(
    session: &Session,
    request: CompletionRequest<'_>,
) -> CompletionState {
    let mut comp = Completions::new();
    let before_cursor = &request.text[..request.cursor];
    let (word_start, in_single) = find_comp_word_start(before_cursor);
    let existing = existing_cmd_args(before_cursor, word_start);
    let raw_word = &before_cursor[word_start..];
    let (partial, in_quote): (String, bool) = if in_single {
        (raw_word.strip_prefix('\'').unwrap_or(raw_word).into(), true)
    } else if let Some(rest) = raw_word.strip_prefix("~/'") {
        let unquoted = rest.strip_suffix('\'').unwrap_or(rest);
        (format!("~/{unquoted}"), true)
    } else if let Some(inner) = raw_word.strip_prefix('\'') {
        (inner.strip_suffix('\'').unwrap_or(inner).into(), true)
    } else {
        (raw_word.into(), false)
    };

    if let Some(env_prefix) = partial.strip_prefix('$') {
        for key in session
            .env
            .keys()
            .filter_map(|key| std::str::from_utf8(key).ok())
        {
            if key.starts_with(env_prefix) {
                comp.push(&format!("${key}"), false, false, false);
            }
        }
        comp.sort_entries();
        return state(comp, request.term_cols, String::new(), in_quote);
    }

    if !partial.is_empty()
        && !partial.contains('/')
        && is_command_position(before_cursor, word_start)
    {
        for builtin in [
            "cd", "set", "unset", "alias", "z", "denv", "c", "l", "w", "which",
        ] {
            if builtin.starts_with(partial.as_str()) {
                comp.push(builtin, false, false, false);
            }
        }
        for name in session.aliases.keys() {
            if name.starts_with(partial.as_str()) {
                comp.push(name, false, false, false);
            }
        }
        complete_commands(session, &partial, &mut comp);
        complete_path_from_root(session, Path::new("."), &partial, true, &mut comp);
        comp.sort_entries();
        comp.dedup_sorted();
        return state(comp, request.term_cols, String::new(), false);
    }

    let first_word = request.text.split_whitespace().next().unwrap_or("");
    let dirs_only = matches!(first_word, "cd" | "z") && word_start > 0;

    const SSH_CMDS: &[&str] = &["ssh", "scp", "rsync", "sftp", "mosh"];
    if word_start > 0 && SSH_CMDS.contains(&first_word) {
        if let Some(colon_pos) = partial.find(':') {
            let host = &partial[..colon_pos];
            let remote_path = &partial[colon_pos + 1..];
            complete_remote_path(host, remote_path, &mut comp);
            if !comp.is_empty() {
                let dir_prefix = if let Some(slash) = remote_path.rfind('/') {
                    format!("{}:{}", host, &remote_path[..=slash])
                } else {
                    format!("{host}:")
                };
                return state(comp, request.term_cols, dir_prefix, in_quote);
            }
        } else if !partial.contains('/') {
            if let Some(home) = session.home.as_deref() {
                complete_hostnames(&partial, home, &mut comp);
            }
            complete_path(session, &partial, false, &mut comp);
            comp.sort_entries();
            comp.dedup_sorted();
            filter_existing_args(&mut comp, &existing, "");
            if !comp.is_empty() {
                return state(comp, request.term_cols, String::new(), in_quote);
            }
        }
    }

    let (lookup_root, expanded, user_root) = expand_completion_path(session, &partial);
    let dir_prefix = partial
        .rfind('/')
        .map(|slash_pos| partial[..=slash_pos].to_string())
        .unwrap_or_default();
    complete_path_from_root(session, &lookup_root, &expanded, dirs_only, &mut comp);
    filter_existing_args(&mut comp, &existing, &dir_prefix);
    if !comp.is_empty() {
        return state(comp, request.term_cols, dir_prefix, in_quote);
    }

    let (partial_comp, groups) = complete_partial_path(&lookup_root, &expanded, dirs_only);
    if !groups.is_empty() {
        for (resolved_dir, start, count) in &groups {
            let rel_dir = resolved_dir
                .strip_prefix(&lookup_root)
                .ok()
                .and_then(|path| path.to_str())
                .unwrap_or_else(|| resolved_dir.to_str().unwrap_or(""));
            for i in *start..*start + *count {
                let entry = &partial_comp.entries[i];
                let mark = comp.begin_entry();
                comp.names.push_str(rel_dir);
                if !rel_dir.is_empty() && !rel_dir.ends_with('/') {
                    comp.names.push('/');
                }
                comp.names.push_str(partial_comp.entry_name(entry));
                comp.finish_entry(mark, entry.is_dir(), entry.is_link(), entry.is_exec());
            }
        }
        filter_existing_args(&mut comp, &existing, &user_root);
        return state(comp, request.term_cols, user_root, in_quote);
    }

    CompletionState {
        term_cols: request.term_cols,
        ..CompletionState::default()
    }
}

fn state(comp: Completions, term_cols: u16, dir_prefix: String, in_quote: bool) -> CompletionState {
    let (cols, rows) = compute_grid(&comp.entries, term_cols);
    CompletionState {
        comp,
        selected: 0,
        cols,
        rows,
        scroll: 0,
        term_cols,
        dir_prefix,
        in_quote,
    }
}

pub(super) fn completion_replacement(state: &CompletionState) -> Option<String> {
    let entry = state.selected_entry()?;
    let name = state.selected_name()?;
    let mut inner = state.dir_prefix.clone();
    inner.push_str(name);
    if entry.is_dir() {
        inner.push('/');
    } else if entry.is_host() {
        inner.push(':');
    }
    if inner.starts_with('$') {
        return Some(inner);
    }
    if state.in_quote || needs_quoting(&inner) || inner.contains('\'') {
        let escaped = inner.replace('\'', "'\\''");
        if let Some(rest) = escaped.strip_prefix("~/") {
            Some(format!("~/'{rest}'"))
        } else {
            Some(format!("'{escaped}'"))
        }
    } else {
        Some(inner)
    }
}

pub(super) fn find_comp_word_start(s: &str) -> (usize, bool) {
    let mut in_single = false;
    let mut in_double = false;
    let mut word_start = 0;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' if !in_double => {
                in_single = !in_single;
                i += 1;
            }
            b'"' if !in_single => {
                in_double = !in_double;
                i += 1;
            }
            b'\\' if !in_single && i + 1 < bytes.len() => i += 2,
            b' ' | b'\t' | b'\n' if !in_single && !in_double => {
                i += 1;
                word_start = i;
            }
            _ => i += 1,
        }
    }
    (word_start, in_single)
}

fn needs_quoting(s: &str) -> bool {
    s.bytes().any(|b| {
        matches!(
            b,
            b' ' | b'\t'
                | b'('
                | b')'
                | b'$'
                | b'*'
                | b'?'
                | b'['
                | b']'
                | b'|'
                | b'&'
                | b'>'
                | b'<'
                | b';'
                | b'#'
                | b'\\'
                | b'"'
                | b'`'
        )
    })
}

fn current_cmd_start(s: &str) -> usize {
    let mut in_single = false;
    let bytes = s.as_bytes();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' => {
                in_single = !in_single;
                i += 1;
            }
            b'|' | b';' if !in_single => {
                i += 1;
                start = i;
            }
            b'&' if !in_single && i + 1 < bytes.len() && bytes[i + 1] == b'&' => {
                i += 2;
                start = i;
            }
            _ => i += 1,
        }
    }
    start
}

fn shell_tokens(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut in_single = false;
    for ch in s.chars() {
        match ch {
            '\'' => in_single = !in_single,
            ' ' | '\t' if !in_single => {
                if !cur.is_empty() {
                    tokens.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(ch),
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    tokens
}

fn existing_cmd_args(before_cursor: &str, word_start: usize) -> Vec<String> {
    let prefix = &before_cursor[..word_start];
    let cmd_start = current_cmd_start(prefix);
    let tokens = shell_tokens(&prefix[cmd_start..]);
    if tokens.len() > 1 {
        tokens[1..].to_vec()
    } else {
        Vec::new()
    }
}

fn filter_existing_args(comp: &mut Completions, existing: &[String], dir_prefix: &str) {
    if existing.is_empty() {
        return;
    }
    let to_remove: Vec<bool> = comp
        .entries
        .iter()
        .map(|entry| {
            let full = format!("{}{}", dir_prefix, comp.entry_name(entry));
            existing.iter().any(|arg| {
                *arg == full || (entry.is_dir() && arg.strip_suffix('/') == Some(full.as_str()))
            })
        })
        .collect();
    let mut idx = 0;
    comp.entries.retain(|_| {
        let remove = to_remove[idx];
        idx += 1;
        !remove
    });
}

fn is_command_position(before_cursor: &str, word_start: usize) -> bool {
    if word_start == 0 {
        return true;
    }
    let before_word = before_cursor[..word_start].trim_end();
    if before_word.ends_with('|') || before_word.ends_with(';') || before_word.ends_with("&&") {
        return true;
    }
    let prev_word = before_word
        .rsplit_once(|ch: char| ch.is_whitespace() || ch == '|' || ch == ';')
        .map(|(_, word)| word)
        .unwrap_or(before_word);
    matches!(prev_word, "sudo" | "doas" | "su")
}

fn expand_completion_path(session: &Session, partial: &str) -> (PathBuf, String, String) {
    if partial == "~" {
        if let Some(home) = &session.home {
            return (home.clone(), String::new(), "~/".to_string());
        }
    } else if let Some(rest) = partial.strip_prefix("~/")
        && let Some(home) = &session.home
    {
        return (home.clone(), rest.to_string(), "~/".to_string());
    }
    (session.cwd.clone(), partial.to_string(), String::new())
}

fn complete_path(session: &Session, partial: &str, dirs_only: bool, comp: &mut Completions) {
    let (root, expanded, _) = expand_completion_path(session, partial);
    complete_path_from_root(session, &root, &expanded, dirs_only, comp);
}

fn complete_path_from_root(
    session: &Session,
    root: &Path,
    partial: &str,
    dirs_only: bool,
    comp: &mut Completions,
) {
    let (dir, prefix) = split_path(partial);
    if dir.is_empty()
        && (root == Path::new(".") || root == session.cwd)
        && let Some(snapshot) = &session.cwd_snapshot
    {
        complete_from_cwd_snapshot(snapshot, prefix, dirs_only, comp);
        return;
    }
    let dir_path = if dir.is_empty() {
        root.to_path_buf()
    } else if Path::new(dir).is_absolute() {
        PathBuf::from(dir)
    } else {
        root.join(dir)
    };
    if let Some(snapshot) = session.completion_dir_snapshot(&dir_path) {
        complete_from_dir_snapshot(&snapshot, prefix, dirs_only, comp);
        return;
    }
    complete_path_into(root, partial, dirs_only, comp);
}

fn complete_path_into(root: &Path, partial: &str, dirs_only: bool, comp: &mut Completions) {
    let (dir, prefix) = split_path(partial);
    let dir_path = if dir.is_empty() {
        root.to_path_buf()
    } else if Path::new(dir).is_absolute() {
        PathBuf::from(dir)
    } else {
        root.join(dir)
    };
    complete_in_dir(&dir_path, prefix, dirs_only, comp);
}

#[cfg(test)]
pub(super) fn completion_test_dir(files: &[&str]) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    for file in files {
        fs::write(root.path().join(file), "").unwrap();
    }
    root
}

fn complete_from_cwd_snapshot(
    snapshot: &super::session::CwdSnapshot,
    prefix: &str,
    dirs_only: bool,
    comp: &mut Completions,
) {
    let prefix_bytes = prefix.as_bytes();
    let before = comp.entries.len();
    let mut prefix_count = 0_usize;
    for entry in &snapshot.entries {
        let name_bytes = entry.name_bytes.as_slice();
        if name_bytes.first() == Some(&b'.') && !prefix_bytes.starts_with(b".") {
            continue;
        }
        let is_prefix = name_bytes.starts_with(prefix_bytes);
        if !is_prefix && (prefix_bytes.is_empty() || !contains_icase(name_bytes, prefix_bytes)) {
            continue;
        }
        let is_dir = entry.completion_is_dir();
        if dirs_only && !is_dir {
            continue;
        }
        let Ok(name) = std::str::from_utf8(name_bytes) else {
            continue;
        };
        comp.push_with_mtime(
            name,
            is_dir,
            entry.metadata.file_type().is_symlink(),
            entry.completion_is_exec(),
            entry
                .target_metadata
                .as_ref()
                .unwrap_or(&entry.metadata)
                .mtime(),
        );
        if is_prefix {
            prefix_count += 1;
        }
    }
    keep_prefix_matches(comp, before, prefix_bytes, prefix_count);
    comp.sort_by_mtime();
}

fn complete_from_dir_snapshot(
    snapshot: &super::session::DirCompletionSnapshot,
    prefix: &str,
    dirs_only: bool,
    comp: &mut Completions,
) {
    let prefix_bytes = prefix.as_bytes();
    let before = comp.entries.len();
    let mut prefix_count = 0_usize;
    for entry in &snapshot.entries {
        let name_bytes = entry.name_bytes.as_slice();
        if name_bytes.first() == Some(&b'.') && !prefix_bytes.starts_with(b".") {
            continue;
        }
        let is_prefix = name_bytes.starts_with(prefix_bytes);
        if !is_prefix && (prefix_bytes.is_empty() || !contains_icase(name_bytes, prefix_bytes)) {
            continue;
        }
        if dirs_only && !entry.is_dir {
            continue;
        }
        let Ok(name) = std::str::from_utf8(name_bytes) else {
            continue;
        };
        comp.push_with_mtime(
            name,
            entry.is_dir,
            entry.is_link,
            entry.is_exec,
            entry.mtime,
        );
        if is_prefix {
            prefix_count += 1;
        }
    }
    keep_prefix_matches(comp, before, prefix_bytes, prefix_count);
    comp.sort_by_mtime();
}

fn complete_in_dir(dir: &Path, prefix: &str, dirs_only: bool, comp: &mut Completions) {
    let dir_bytes = dir.as_os_str().as_bytes();
    let Ok(dir_fd) = rfs::open(
        dir,
        rfs::OFlags::RDONLY | rfs::OFlags::DIRECTORY,
        rfs::Mode::empty(),
    ) else {
        return;
    };
    let Ok(dir_iter) = rfs::Dir::read_from(&dir_fd) else {
        return;
    };

    let prefix_bytes = prefix.as_bytes();
    let before = comp.entries.len();
    let mut prefix_count = 0_usize;
    let mut path_buf = [0_u8; 4096];
    let mut dir_prefix_len = dir_bytes.len();
    if dir_prefix_len >= path_buf.len() {
        return;
    }
    path_buf[..dir_prefix_len].copy_from_slice(dir_bytes);
    if dir_prefix_len > 0 && path_buf[dir_prefix_len - 1] != b'/' {
        path_buf[dir_prefix_len] = b'/';
        dir_prefix_len += 1;
    }

    for entry in dir_iter.flatten() {
        let name_bytes = entry.file_name().to_bytes();
        if name_bytes == b"." || name_bytes == b".." {
            continue;
        }
        if name_bytes.iter().any(|&b| b < b' ' || b == 0x7f) {
            continue;
        }
        if name_bytes.first() == Some(&b'.') && !prefix_bytes.starts_with(b".") {
            continue;
        }
        let is_prefix = name_bytes.starts_with(prefix_bytes);
        if !is_prefix && (prefix_bytes.is_empty() || !contains_icase(name_bytes, prefix_bytes)) {
            continue;
        }

        let total = dir_prefix_len + name_bytes.len();
        if total >= path_buf.len() {
            continue;
        }
        path_buf[dir_prefix_len..total].copy_from_slice(name_bytes);

        use std::os::unix::ffi::OsStrExt as _;
        let entry_path = std::ffi::OsStr::from_bytes(&path_buf[..total]);
        let Ok(st) = rfs::statat(rfs::CWD, entry_path, rfs::AtFlags::empty()) else {
            continue;
        };
        let is_dir = st.st_mode & 0xF000 == 0x4000;
        if dirs_only && !is_dir {
            continue;
        }
        let is_link = rfs::statat(rfs::CWD, entry_path, rfs::AtFlags::SYMLINK_NOFOLLOW)
            .map(|lst| lst.st_mode & 0xF000 == 0xA000)
            .unwrap_or(false);
        let is_exec = !is_dir && st.st_mode & 0o111 != 0;
        let Ok(name) = std::str::from_utf8(name_bytes) else {
            continue;
        };
        comp.push_with_mtime(name, is_dir, is_link, is_exec, st.st_mtime);
        if is_prefix {
            prefix_count += 1;
        }
    }

    keep_prefix_matches(comp, before, prefix_bytes, prefix_count);
    comp.sort_by_mtime();
}

fn keep_prefix_matches(
    comp: &mut Completions,
    before: usize,
    prefix_bytes: &[u8],
    prefix_count: usize,
) {
    let added = comp.entries.len() - before;
    if prefix_count == 0 || prefix_count >= added {
        return;
    }
    let mut i = before;
    while i < comp.entries.len() {
        let entry = &comp.entries[i];
        let name = &comp.names.as_bytes()[entry.name_start as usize..][..entry.name_len as usize];
        if name.starts_with(prefix_bytes) {
            i += 1;
        } else {
            comp.entries.remove(i);
        }
    }
}

fn complete_partial_path(
    root: &Path,
    partial: &str,
    dirs_only: bool,
) -> (Completions, Vec<(PathBuf, usize, usize)>) {
    let (dir, prefix) = split_path(partial);
    if dir.is_empty() {
        return (Completions::new(), Vec::new());
    }
    let dir_path = if Path::new(dir).is_absolute() {
        PathBuf::from(dir)
    } else {
        root.join(dir)
    };
    if is_dir(&dir_path) {
        return (Completions::new(), Vec::new());
    }
    let resolved_dirs = resolve_partial_dir(root, dir.trim_end_matches('/'));
    let mut comp = Completions::new();
    let mut groups = Vec::new();
    for resolved_dir in resolved_dirs {
        let start = comp.entries.len();
        complete_in_dir(&resolved_dir, prefix, dirs_only, &mut comp);
        let count = comp.entries.len() - start;
        if count > 0 {
            groups.push((resolved_dir, start, count));
        }
    }
    (comp, groups)
}

fn is_dir(path: &Path) -> bool {
    rfs::statat(rfs::CWD, path, rfs::AtFlags::empty())
        .map(|st| st.st_mode & 0xF000 == 0x4000)
        .unwrap_or(false)
}

fn resolve_partial_dir(root: &Path, dir: &str) -> Vec<PathBuf> {
    let dir = dir.trim_end_matches('/');
    if dir.is_empty() {
        return Vec::new();
    }
    let path = if Path::new(dir).is_absolute() {
        PathBuf::from(dir)
    } else {
        root.join(dir)
    };
    if is_dir(&path) {
        return vec![path];
    }
    let (parent, component) = match dir.rfind('/') {
        Some(0) => ("/", &dir[1..]),
        Some(index) => (&dir[..index], &dir[index + 1..]),
        None => ("", dir),
    };
    if component.is_empty() {
        return Vec::new();
    }
    let parents = if parent.is_empty() {
        vec![root.to_path_buf()]
    } else {
        resolve_partial_dir(root, parent)
    };
    let mut results = Vec::new();
    for parent in parents {
        if let Ok(entries) = fs::read_dir(&parent) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if !name.starts_with(component) {
                    continue;
                }
                if name.starts_with('.') && !component.starts_with('.') {
                    continue;
                }
                let path = parent.join(name.as_ref());
                if is_dir(&path) {
                    results.push(path);
                }
            }
        }
        if results.len() > 64 {
            results.truncate(64);
            break;
        }
    }
    results
}

fn split_path(partial: &str) -> (&str, &str) {
    match partial.rfind('/') {
        Some(index) => (&partial[..=index], &partial[index + 1..]),
        None => ("", partial),
    }
}

pub(super) fn compute_grid(entries: &[CompEntry], term_cols: u16) -> (usize, usize) {
    let n = entries.len();
    if n == 0 {
        return (0, 0);
    }
    let max_cols = 6.min(n);
    let term_w = term_cols as usize;
    for cols in (1..=max_cols).rev() {
        let rows = n.div_ceil(cols);
        let mut col_widths = [0_usize; 6];
        for (i, entry) in entries.iter().enumerate() {
            let col = i / rows;
            if col < cols {
                col_widths[col] = col_widths[col].max(entry.display_width());
            }
        }
        let total = col_widths[..cols].iter().sum::<usize>() + cols.saturating_sub(1) * 2;
        if total <= term_w {
            return (cols, rows);
        }
    }
    (1, n)
}

fn parse_ssh_hosts(home: &Path) -> Vec<String> {
    let mut hosts = Vec::new();
    if let Ok(data) = fs::read_to_string(home.join(".ssh/config")) {
        for line in data.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed
                .strip_prefix("Host ")
                .or_else(|| trimmed.strip_prefix("Host\t"))
            {
                for host in rest.split_whitespace() {
                    if !host.contains('*') && !host.contains('?') && host != "." {
                        hosts.push(host.to_string());
                    }
                }
            }
        }
    }
    if let Ok(data) = fs::read_to_string(home.join(".ssh/known_hosts")) {
        for line in data.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('|') {
                continue;
            }
            if let Some(host_field) = trimmed.split_whitespace().next() {
                for entry in host_field.split(',') {
                    let host = entry
                        .strip_prefix('[')
                        .and_then(|value| value.split(']').next())
                        .unwrap_or(entry);
                    if !host.is_empty() && !host.contains('*') {
                        hosts.push(host.to_string());
                    }
                }
            }
        }
    }
    hosts.sort_unstable();
    hosts.dedup();
    hosts
}

fn complete_hostnames(prefix: &str, home: &Path, comp: &mut Completions) {
    for host in parse_ssh_hosts(home) {
        if host.starts_with(prefix) {
            comp.push_host(&host);
        }
    }
}

fn complete_remote_path(host: &str, path_prefix: &str, comp: &mut Completions) {
    let cmd = format!(
        "ssh -o BatchMode=yes -o ConnectTimeout=2 {} 'ls -dp {}* 2>/dev/null'",
        host,
        shell_escape(path_prefix),
    );
    let (pid, pipe_r) = match spawn_command_subst(&cmd) {
        Ok(value) => value,
        Err(_) => return,
    };
    let deadline_ns = monotonic_ns() + 3_000_000_000;
    let mut output = String::new();
    let mut buf = [0_u8; 4096];
    let _ = set_pipe_nonblocking(&pipe_r);
    loop {
        match rio::read(&pipe_r, &mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if let Ok(text) = std::str::from_utf8(&buf[..n]) {
                    output.push_str(text);
                }
            }
            Err(err) => {
                let err = std::io::Error::from(err);
                if err.raw_os_error() == Some(rio::Errno::AGAIN.raw_os_error())
                    || err.raw_os_error() == Some(rio::Errno::WOULDBLOCK.raw_os_error())
                {
                    if monotonic_ns() >= deadline_ns {
                        let _ = rprocess::kill_process(pid, rprocess::Signal::KILL);
                        break;
                    }
                    let mut pfd = [revent::PollFd::new(&pipe_r, revent::PollFlags::IN)];
                    let timeout = revent::Timespec::try_from(std::time::Duration::from_millis(100))
                        .expect("remote completion timeout fits Timespec");
                    let _ = revent::poll(&mut pfd, Some(&timeout));
                    continue;
                }
                if err.kind() != std::io::ErrorKind::Interrupted {
                    break;
                }
            }
        }
    }
    let _ = rprocess::waitpid(Some(pid), rprocess::WaitOptions::empty());

    let dir_prefix = match path_prefix.rfind('/') {
        Some(index) => &path_prefix[..=index],
        None => "",
    };
    for line in output.lines() {
        if line.is_empty() {
            continue;
        }
        let is_dir = line.ends_with('/');
        let path = line.trim_end_matches('/');
        let name = if !dir_prefix.is_empty() {
            path.strip_prefix(dir_prefix).unwrap_or(path)
        } else {
            path.rsplit('/').next().unwrap_or(path)
        };
        if !name.is_empty() {
            comp.push(name, is_dir, false, false);
        }
    }
}

fn set_pipe_nonblocking(pipe: &OwnedFd) -> std::io::Result<()> {
    let mut flags = rfs::fcntl_getfl(pipe).map_err(std::io::Error::from)?;
    flags.insert(rfs::OFlags::NONBLOCK);
    rfs::fcntl_setfl(pipe, flags).map_err(std::io::Error::from)
}

fn shell_escape(s: &str) -> String {
    if !s.contains('\'') {
        return s.to_string();
    }
    s.replace('\'', "'\\''")
}

fn spawn_command_subst(cmd: &str) -> Result<(rprocess::Pid, OwnedFd), std::io::Error> {
    let (pipe_r, pipe_w) = pipe_cloexec()?;
    let mut file_actions: libc::posix_spawn_file_actions_t = unsafe { std::mem::zeroed() };
    unsafe {
        libc::posix_spawn_file_actions_init(&mut file_actions);
        libc::posix_spawn_file_actions_adddup2(&mut file_actions, pipe_w.as_raw_fd(), 1);
        let dev_null = CString::new("/dev/null").unwrap();
        libc::posix_spawn_file_actions_addopen(
            &mut file_actions,
            0,
            dev_null.as_ptr(),
            libc::O_RDONLY,
            0,
        );
    }
    let mut attrs: libc::posix_spawnattr_t = unsafe { std::mem::zeroed() };
    unsafe {
        libc::posix_spawnattr_init(&mut attrs);
        libc::posix_spawnattr_setflags(&mut attrs, libc::POSIX_SPAWN_SETSIGDEF as libc::c_short);
        let mut sigset: libc::sigset_t = std::mem::zeroed();
        libc::sigfillset(&mut sigset);
        libc::posix_spawnattr_setsigdefault(&mut attrs, &sigset);
    }
    let sh = CString::new("/bin/sh").unwrap();
    let c_flag = CString::new("-c").unwrap();
    let c_cmd = CString::new(cmd)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "NUL in command"))?;
    let argv: [*mut libc::c_char; 4] = [
        sh.as_ptr() as *mut _,
        c_flag.as_ptr() as *mut _,
        c_cmd.as_ptr() as *mut _,
        std::ptr::null_mut(),
    ];
    let mut pid: libc::pid_t = 0;
    let rc = unsafe {
        libc::posix_spawnp(
            &mut pid,
            sh.as_ptr(),
            &file_actions,
            &attrs,
            argv.as_ptr(),
            environ(),
        )
    };
    unsafe {
        libc::posix_spawn_file_actions_destroy(&mut file_actions);
        libc::posix_spawnattr_destroy(&mut attrs);
    }
    drop(pipe_w);
    if rc != 0 {
        return Err(std::io::Error::from_raw_os_error(rc));
    }
    let Some(pid) = rprocess::Pid::from_raw(pid) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "posix_spawn returned invalid pid",
        ));
    };
    Ok((pid, pipe_r))
}

fn pipe_cloexec() -> Result<(OwnedFd, OwnedFd), std::io::Error> {
    let (read, write) = rpipe::pipe().map_err(std::io::Error::from)?;
    set_fd_cloexec(&read)?;
    set_fd_cloexec(&write)?;
    Ok((read, write))
}

fn set_fd_cloexec(fd: &OwnedFd) -> std::io::Result<()> {
    let mut flags = rio::fcntl_getfd(fd).map_err(std::io::Error::from)?;
    flags.insert(rio::FdFlags::CLOEXEC);
    rio::fcntl_setfd(fd, flags).map_err(std::io::Error::from)
}

fn environ() -> *const *mut libc::c_char {
    unsafe extern "C" {
        static environ: *const *mut libc::c_char;
    }
    unsafe { environ }
}

fn monotonic_ns() -> u64 {
    let ts = rtime::clock_gettime(rtime::ClockId::Monotonic);
    ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
}

fn complete_commands(session: &Session, prefix: &str, comp: &mut Completions) {
    for command in &session.path_commands {
        if command.name.starts_with(prefix) {
            comp.push(&command.name, false, false, true);
        }
    }
}

fn contains_icase(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > haystack.len() {
        return false;
    }
    let first = needle[0].to_ascii_lowercase();
    for i in 0..=(haystack.len() - needle.len()) {
        if haystack[i].to_ascii_lowercase() == first
            && haystack[i..i + needle.len()]
                .iter()
                .zip(needle)
                .all(|(a, b)| a.eq_ignore_ascii_case(b))
        {
            return true;
        }
    }
    false
}

pub(super) fn str_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

pub(super) fn char_width(ch: char) -> usize {
    let cp = ch as u32;
    if cp < 0x7f {
        return if cp >= 0x20 { 1 } else { 0 };
    }
    if cp < 0xa0 || cp == 0xad {
        return 0;
    }
    if matches!(cp, 0x200b..=0x200f | 0x2028..=0x202e | 0x2060..=0x2064 | 0xfeff) {
        return 0;
    }
    if matches!(
        cp,
        0x0300..=0x036f | 0x1ab0..=0x1aff | 0x1dc0..=0x1dff | 0x20d0..=0x20ff | 0xfe20..=0xfe2f
    ) {
        return 0;
    }
    if is_wide(cp) { 2 } else { 1 }
}

fn is_wide(cp: u32) -> bool {
    matches!(
        cp,
        0x1100..=0x115f
            | 0x231a..=0x231b
            | 0x2329..=0x232a
            | 0x23e9..=0x23f3
            | 0x23f8..=0x23fa
            | 0x25fd..=0x25fe
            | 0x2614..=0x2615
            | 0x2648..=0x2653
            | 0x267f
            | 0x2693
            | 0x26a1
            | 0x26aa..=0x26ab
            | 0x26bd..=0x26be
            | 0x26c4..=0x26c5
            | 0x26ce
            | 0x26d4
            | 0x26ea
            | 0x26f2..=0x26f3
            | 0x26f5
            | 0x26fa
            | 0x26fd
            | 0x2702
            | 0x2705
            | 0x2708..=0x270d
            | 0x270f
            | 0x2712
            | 0x2714
            | 0x2716
            | 0x271d
            | 0x2721
            | 0x2728
            | 0x2733..=0x2734
            | 0x2744
            | 0x2747
            | 0x274c
            | 0x274e
            | 0x2753..=0x2755
            | 0x2757
            | 0x2763..=0x2764
            | 0x2795..=0x2797
            | 0x27a1
            | 0x27b0
            | 0x27bf
            | 0x2934..=0x2935
            | 0x2b05..=0x2b07
            | 0x2b1b..=0x2b1c
            | 0x2b50
            | 0x2b55
            | 0x2e80..=0x303e
            | 0x3040..=0x33bf
            | 0x3400..=0x4dbf
            | 0x4e00..=0xa4cf
            | 0xac00..=0xd7a3
            | 0xf900..=0xfaff
            | 0xfe10..=0xfe19
            | 0xfe30..=0xfe6f
            | 0xff00..=0xff60
            | 0xffe0..=0xffe6
            | 0x1f300..=0x1f64f
            | 0x1f900..=0x1f9ff
            | 0x20000..=0x3fffd
    )
}

#[cfg(test)]
mod tests {
    use super::{
        CompletionRequest, CompletionState, Completions, Session, completion_replacement,
        compute_grid, fs, start_completion,
    };
    use crate::xshi::interactive::session::set_env_bytes;
    use std::path::PathBuf;

    #[test]
    fn grid_computation_uses_column_major_layout() {
        let mut comp = Completions::new();
        for i in 0..7 {
            comp.push(&format!("file{i}.rs"), false, false, false);
        }
        let (cols, rows) = compute_grid(&comp.entries, 80);
        assert!(cols >= 1);
        assert!(rows >= 1);
        assert!(cols <= 6);
        assert!(cols * rows >= 7);
    }

    #[test]
    fn tilde_completion_uses_home_but_keeps_user_prefix() {
        let root =
            std::env::temp_dir().join(format!("xshi-home-completion-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("dev")).unwrap();
        let mut session = Session::new();
        session.home = Some(root.clone());
        session.cwd = PathBuf::from("/");
        let state = start_completion(
            &session,
            CompletionRequest {
                text: "cd ~/d",
                cursor: "cd ~/d".len(),
                term_cols: 80,
            },
        );
        assert_eq!(state.dir_prefix, "~/");
        assert_eq!(state.comp.name(0), "dev");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn env_completion_keeps_dollar_prefix() {
        let mut session = Session::new();
        session.env.clear();
        set_env_bytes(&mut session.env, b"EDITOR", b"vim");
        let state = start_completion(
            &session,
            CompletionRequest {
                text: "$ED",
                cursor: 3,
                term_cols: 80,
            },
        );
        assert_eq!(state.comp.name(0), "$EDITOR");
    }

    #[test]
    fn completion_replacement_quotes_paths_and_marks_hosts() {
        let mut files = Completions::new();
        files.push("two words.txt", false, false, false);
        let quoted = CompletionState {
            comp: files,
            selected: 0,
            term_cols: 80,
            ..CompletionState::default()
        };
        assert_eq!(
            completion_replacement(&quoted).as_deref(),
            Some("'two words.txt'")
        );

        let mut hosts = Completions::new();
        hosts.push_host("buildbox");
        let host = CompletionState {
            comp: hosts,
            selected: 0,
            term_cols: 80,
            ..CompletionState::default()
        };
        assert_eq!(completion_replacement(&host).as_deref(), Some("buildbox:"));
    }

    #[test]
    fn completion_selection_wraps_by_grid_position() {
        let mut comp = Completions::new();
        for name in ["a", "b", "c", "d", "e"] {
            comp.push(name, false, false, false);
        }
        let mut state = CompletionState {
            comp,
            selected: 0,
            cols: 2,
            rows: 3,
            term_cols: 80,
            ..CompletionState::default()
        };

        state.move_left();
        assert_eq!(state.selected, 3);
        state.move_right();
        assert_eq!(state.selected, 0);
        state.move_up();
        assert_eq!(state.selected, 4);
        state.move_down();
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn ssh_host_completion_reads_config_and_known_hosts() {
        let root = std::env::temp_dir().join(format!("xshi-ssh-completion-{}", std::process::id()));
        let ssh = root.join(".ssh");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&ssh).unwrap();
        fs::write(
            ssh.join("config"),
            "Host xsh-buildbox *.ignored\n  HostName example.invalid\n",
        )
        .unwrap();
        fs::write(
            ssh.join("known_hosts"),
            "[builder.example]:22 ssh-ed25519 AAAA\n",
        )
        .unwrap();
        let mut session = Session::new();
        session.home = Some(root.clone());

        let state = start_completion(
            &session,
            CompletionRequest {
                text: "ssh xsh-bu",
                cursor: "ssh xsh-bu".len(),
                term_cols: 80,
            },
        );

        assert_eq!(state.comp.name(0), "xsh-buildbox");
        assert_eq!(
            completion_replacement(&state).as_deref(),
            Some("xsh-buildbox:")
        );
        let _ = fs::remove_dir_all(root);
    }
}
