#![allow(clippy::single_call_fn)]

use rustc_hash::{FxHashMap, FxHashSet};
use rustix::fs::{self as rfs, FlockOperation};
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};

const CACHE_MAGIC_V1: &[u8; 4] = b"XH\x01\0";
const CACHE_HEADER_SIZE: usize = 12;
const ISH_LOG_RECORD_PREFIX: &str = ":ish-history:v1\t";

#[derive(Clone, Debug)]
pub(super) struct History {
    arena: String,
    offsets: Vec<(u32, u16)>,
    timestamps: Vec<u64>,
    index_by_hash: FxHashMap<u64, usize>,
    path: Option<PathBuf>,
    file_pos: u64,
    local: Vec<bool>,
    session_cutoff: u64,
    cache_dirty: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct FuzzyMatch {
    pub(super) entry_idx: usize,
    pub(super) match_positions: [u16; 32],
    pub(super) match_count: u8,
    pub(super) score: i16,
}

struct ParsedHistoryLine<'a> {
    command: &'a str,
    timestamp: u64,
}

impl History {
    pub(super) fn load(path: Option<PathBuf>) -> Self {
        let Some(path) = path else {
            return Self::empty(None);
        };
        let cache = cache_path_for(&path);
        match Self::load_from_cache(&path, &cache) {
            Ok(Some(mut history)) => {
                history.sync();
                history.session_cutoff = current_unix_millis();
                history
            }
            Ok(None) => {
                let mut history = Self::load_from_text(&path);
                history.file_pos = fs::metadata(&path)
                    .map(|metadata| metadata.len())
                    .unwrap_or(0);
                if !history.offsets.is_empty() {
                    history.save_cache();
                }
                history.session_cutoff = current_unix_millis();
                history
            }
            Err(()) => {
                let mut history = Self::load_from_text(&path);
                history.file_pos = fs::metadata(&path)
                    .map(|metadata| metadata.len())
                    .unwrap_or(0);
                history.cache_dirty = true;
                history.session_cutoff = current_unix_millis();
                history
            }
        }
    }

    pub(super) fn from_entries(entries: Vec<String>) -> Self {
        let timestamp = current_unix_millis();
        let total = entries.iter().map(|entry| entry.len()).sum();
        let mut arena = String::with_capacity(total);
        let mut offsets = Vec::with_capacity(entries.len());
        let mut timestamps = Vec::with_capacity(entries.len());
        let mut index_by_hash =
            FxHashMap::with_capacity_and_hasher(entries.len(), Default::default());
        for entry in entries {
            push_entry(
                &mut arena,
                &mut offsets,
                &mut timestamps,
                &mut index_by_hash,
                &entry,
                timestamp,
            );
        }
        let count = offsets.len();
        Self {
            arena,
            offsets,
            timestamps,
            index_by_hash,
            path: None,
            file_pos: 0,
            local: vec![false; count],
            session_cutoff: timestamp,
            cache_dirty: false,
        }
    }

    pub(super) fn add(&mut self, command: &str, timestamp: u64) {
        let command = command.trim().replace('\n', " ");
        let command = command.trim();
        if command.is_empty() {
            return;
        }
        if self
            .offsets
            .last()
            .is_some_and(|_| self.get(self.offsets.len() - 1) == command)
        {
            return;
        }

        let hash = hash_str(command);
        if self.index_by_hash.contains_key(&hash) {
            self.remove_entries_matching(command);
        }
        push_entry(
            &mut self.arena,
            &mut self.offsets,
            &mut self.timestamps,
            &mut self.index_by_hash,
            command,
            timestamp,
        );
        self.local.push(true);
        self.append_to_file(timestamp, command);
    }

    pub(super) fn sync(&mut self) {
        let Some(path) = &self.path else {
            return;
        };
        let file_size = match fs::metadata(path) {
            Ok(metadata) => metadata.len(),
            Err(_) => return,
        };
        if file_size == self.file_pos {
            return;
        }
        if file_size < self.file_pos {
            self.file_pos = file_size;
            return;
        }

        let mut file = match fs::File::open(path) {
            Ok(file) => file,
            Err(_) => return,
        };
        use std::io::{Read, Seek, SeekFrom};
        if file.seek(SeekFrom::Start(self.file_pos)).is_err() {
            return;
        }
        let mut tail = String::new();
        if file.read_to_string(&mut tail).is_err() {
            return;
        }
        let fallback_ts = current_unix_millis();
        for line in tail.lines() {
            let Some(parsed) = parse_history_line(line, fallback_ts) else {
                continue;
            };
            let hash = hash_str(parsed.command);
            if let Some(index) = self.find_entry_index(hash, parsed.command) {
                if self.is_session_visible(index) {
                    continue;
                }
                self.remove_entry_at(index);
            }
            push_entry(
                &mut self.arena,
                &mut self.offsets,
                &mut self.timestamps,
                &mut self.index_by_hash,
                parsed.command,
                parsed.timestamp,
            );
            self.local.push(false);
        }
        self.file_pos = file_size;
    }

    pub(super) fn compact(&mut self) {
        let Some(path) = &self.path else {
            return;
        };
        let lock = lock_path_for(path);
        if let Some(parent) = lock.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let Ok(lock_file) = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&lock)
        else {
            return;
        };
        if rfs::flock(&lock_file, FlockOperation::NonBlockingLockExclusive).is_err() {
            return;
        }
        self.file_pos = 0;
        self.sync();
        self.save_cache();
    }

    pub(super) fn len(&self) -> usize {
        self.offsets.len()
    }

    pub(super) fn get(&self, index: usize) -> &str {
        self.entry_text(index)
    }

    pub(super) fn prefix_search(&self, prefix: &str, skip: usize) -> Option<&str> {
        self.offsets
            .iter()
            .rev()
            .filter_map(|&(start, len)| {
                let entry = &self.arena[start as usize..start as usize + len as usize];
                (entry.starts_with(prefix) && entry.len() > prefix.len()).then_some(entry)
            })
            .nth(skip)
    }

    pub(super) fn session_prefix_index_before(&self, prefix: &str, before: usize) -> Option<usize> {
        (0..before.min(self.offsets.len())).rev().find(|&index| {
            self.is_session_visible(index) && self.entry_text(index).starts_with(prefix)
        })
    }

    pub(super) fn session_prefix_index_after(&self, prefix: &str, after: usize) -> Option<usize> {
        ((after + 1)..self.offsets.len()).find(|&index| {
            self.is_session_visible(index) && self.entry_text(index).starts_with(prefix)
        })
    }

    #[allow(dead_code)]
    pub(super) fn visible_entry_indices_into(&self, out: &mut Vec<usize>) {
        out.clear();
        out.extend(
            (0..self.offsets.len())
                .rev()
                .filter(|&index| self.is_session_visible(index)),
        );
    }

    #[allow(dead_code)]
    pub(super) fn fuzzy_search_into(
        &self,
        query: &str,
        results: &mut Vec<FuzzyMatch>,
        limit: usize,
    ) {
        results.clear();
        if limit == 0 {
            return;
        }
        if query.is_empty() {
            results.extend(
                (0..self.offsets.len())
                    .rev()
                    .filter(|&index| self.is_session_visible(index))
                    .take(limit)
                    .map(|index| FuzzyMatch {
                        entry_idx: index,
                        match_positions: [0; 32],
                        match_count: 0,
                        score: 0,
                    }),
            );
            return;
        }

        let query_lower = lowercase_query(query);
        for index in (0..self.offsets.len()).rev() {
            if !self.is_session_visible(index) {
                continue;
            }
            let Some(matched) = classify_match(&query_lower, self.entry_text(index), index) else {
                continue;
            };
            let insert_at = results
                .binary_search_by(|existing| compare_fuzzy_match(existing, &matched))
                .unwrap_or_else(|position| position);
            if insert_at >= limit {
                continue;
            }
            results.insert(insert_at, matched);
            if results.len() > limit {
                results.pop();
            }
            if results.len() == limit && results[limit - 1].score == 3 {
                break;
            }
        }
    }

    pub(super) fn latest_subsequence_match(&self, query: &str) -> Option<&str> {
        if query.is_empty() {
            return self
                .offsets
                .iter()
                .enumerate()
                .rev()
                .find(|&(index, _)| self.is_session_visible(index))
                .map(|(index, _)| self.entry_text(index));
        }
        for index in (0..self.offsets.len()).rev() {
            if !self.is_session_visible(index) {
                continue;
            }
            let entry = self.entry_text(index);
            if cheap_subsequence_match(entry, query) {
                return Some(entry);
            }
        }
        None
    }

    #[allow(dead_code)]
    pub(super) fn fuzzy_search_subset_into(
        &self,
        query: &str,
        candidates: &[usize],
        matched_indices: &mut Vec<usize>,
        results: &mut Vec<FuzzyMatch>,
        limit: usize,
    ) {
        matched_indices.clear();
        results.clear();
        if limit == 0 {
            return;
        }
        if query.is_empty() {
            matched_indices.extend_from_slice(candidates);
            results.extend(candidates.iter().take(limit).map(|&index| FuzzyMatch {
                entry_idx: index,
                match_positions: [0; 32],
                match_count: 0,
                score: 0,
            }));
            return;
        }

        let query_lower = lowercase_query(query);
        for &index in candidates {
            let Some(matched) = classify_match(&query_lower, self.entry_text(index), index) else {
                continue;
            };
            matched_indices.push(index);
            let insert_at = results
                .binary_search_by(|existing| compare_fuzzy_match(existing, &matched))
                .unwrap_or_else(|position| position);
            if insert_at >= limit {
                continue;
            }
            results.insert(insert_at, matched);
            if results.len() > limit {
                results.pop();
            }
            if results.len() == limit && results[limit - 1].score == 3 {
                break;
            }
        }
    }

    fn empty(path: Option<PathBuf>) -> Self {
        Self {
            arena: String::new(),
            offsets: Vec::new(),
            timestamps: Vec::new(),
            index_by_hash: FxHashMap::default(),
            path,
            file_pos: 0,
            local: Vec::new(),
            session_cutoff: 0,
            cache_dirty: false,
        }
    }

    fn load_from_text(path: &Path) -> Self {
        let Ok(data) = fs::read(path) else {
            return Self::empty(Some(path.to_path_buf()));
        };
        let line_count = memchr_count(b'\n', &data);
        let fallback_ts = current_unix_millis();
        let mut seen = FxHashSet::with_capacity_and_hasher(line_count, Default::default());
        let mut deduped = Vec::with_capacity(line_count);
        for chunk in data.rsplit(|&byte| byte == b'\n') {
            if let Ok(line) = std::str::from_utf8(chunk)
                && let Some(parsed) = parse_history_line(line, fallback_ts)
            {
                let hash = hash_str(parsed.command);
                if seen.insert(hash) {
                    deduped.push((parsed.command.to_string(), parsed.timestamp));
                }
            }
        }
        deduped.reverse();
        Self::from_deduped(path.to_path_buf(), deduped)
    }

    fn from_deduped(path: PathBuf, entries: Vec<(String, u64)>) -> Self {
        let total = entries.iter().map(|(entry, _)| entry.len()).sum();
        let mut arena = String::with_capacity(total);
        let mut offsets = Vec::with_capacity(entries.len());
        let mut timestamps = Vec::with_capacity(entries.len());
        let mut index_by_hash =
            FxHashMap::with_capacity_and_hasher(entries.len(), Default::default());
        for (entry, timestamp) in entries {
            push_entry(
                &mut arena,
                &mut offsets,
                &mut timestamps,
                &mut index_by_hash,
                &entry,
                timestamp,
            );
        }
        let count = offsets.len();
        Self {
            arena,
            offsets,
            timestamps,
            index_by_hash,
            path: Some(path),
            file_pos: 0,
            local: vec![false; count],
            session_cutoff: 0,
            cache_dirty: false,
        }
    }

    fn load_from_cache(path: &Path, cache: &Path) -> Result<Option<Self>, ()> {
        let data = match fs::read(cache) {
            Ok(data) => data,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(()),
        };
        Self::parse_cache(&data, path).map(Some).ok_or(())
    }

    fn parse_cache(data: &[u8], path: &Path) -> Option<Self> {
        if data.len() < CACHE_HEADER_SIZE || &data[0..4] != CACHE_MAGIC_V1 {
            return None;
        }
        let entry_count = u32::from_le_bytes(data[4..8].try_into().ok()?) as usize;
        let arena_size = u32::from_le_bytes(data[8..12].try_into().ok()?) as usize;
        let expected = CACHE_HEADER_SIZE + entry_count * 8 + arena_size;
        if data.len() != expected {
            return None;
        }

        let mut timestamps = Vec::with_capacity(entry_count);
        for index in 0..entry_count {
            let offset = CACHE_HEADER_SIZE + index * 8;
            timestamps.push(u64::from_le_bytes(
                data[offset..offset + 8].try_into().ok()?,
            ));
        }

        let arena_start = CACHE_HEADER_SIZE + entry_count * 8;
        let arena_bytes = &data[arena_start..arena_start + arena_size];
        let arena_text = std::str::from_utf8(arena_bytes).ok()?;
        let mut arena = String::with_capacity(arena_size);
        let mut offsets = Vec::with_capacity(entry_count);
        let mut index_by_hash =
            FxHashMap::with_capacity_and_hasher(entry_count, Default::default());
        for entry in arena_text.split('\0') {
            if entry.is_empty() {
                continue;
            }
            push_entry_without_timestamp(&mut arena, &mut offsets, &mut index_by_hash, entry);
        }
        if offsets.len() != entry_count {
            return None;
        }
        let count = offsets.len();
        Some(Self {
            arena,
            offsets,
            timestamps,
            index_by_hash,
            path: Some(path.to_path_buf()),
            file_pos: 0,
            local: vec![false; count],
            session_cutoff: 0,
            cache_dirty: false,
        })
    }

    fn save_cache(&self) {
        if self.cache_dirty {
            return;
        }
        let Some(path) = &self.path else {
            return;
        };
        let cache = cache_path_for(path);
        let tmp = cache.with_extension("bin.tmp");
        if let Some(parent) = cache.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let mut arena_buf = Vec::with_capacity(self.arena.len() + self.offsets.len());
        for &(start, len) in &self.offsets {
            arena_buf.extend_from_slice(
                &self.arena.as_bytes()[start as usize..start as usize + len as usize],
            );
            arena_buf.push(0);
        }
        let entry_count = self.offsets.len();
        let total = CACHE_HEADER_SIZE + entry_count * 8 + arena_buf.len();
        let mut data = Vec::with_capacity(total);
        data.extend_from_slice(CACHE_MAGIC_V1);
        data.extend_from_slice(&(entry_count as u32).to_le_bytes());
        data.extend_from_slice(&(arena_buf.len() as u32).to_le_bytes());
        for &timestamp in &self.timestamps {
            data.extend_from_slice(&timestamp.to_le_bytes());
        }
        data.extend_from_slice(&arena_buf);

        if let Ok(existing) = fs::read(&cache)
            && existing.len() >= CACHE_HEADER_SIZE
            && &existing[0..4] == CACHE_MAGIC_V1
        {
            let old_count =
                u32::from_le_bytes(existing[4..8].try_into().unwrap_or_default()) as usize;
            if entry_count < old_count / 2 && old_count > 100 {
                let _ = fs::remove_file(&tmp);
                return;
            }
        }

        if fs::write(&tmp, data).is_ok() && fs::rename(&tmp, cache).is_ok() {
            let _ = fs::File::create(path);
        }
    }

    fn append_to_file(&mut self, timestamp: u64, command: &str) {
        let Some(path) = &self.path else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{timestamp} {command}");
            if let Ok(metadata) = file.metadata() {
                self.file_pos = metadata.len();
            }
        }
    }

    fn entry_text(&self, index: usize) -> &str {
        let (start, len) = self.offsets[index];
        &self.arena[start as usize..start as usize + len as usize]
    }

    fn is_session_visible(&self, index: usize) -> bool {
        self.local[index] || self.timestamps[index] <= self.session_cutoff
    }

    fn find_entry_index(&self, hash: u64, text: &str) -> Option<usize> {
        self.index_by_hash
            .get(&hash)
            .copied()
            .filter(|&index| self.entry_text(index) == text)
            .or_else(|| {
                self.offsets
                    .iter()
                    .enumerate()
                    .find_map(|(index, _)| (self.entry_text(index) == text).then_some(index))
            })
    }

    fn remove_entry_at(&mut self, index: usize) {
        self.offsets.remove(index);
        self.timestamps.remove(index);
        self.local.remove(index);
        self.rebuild_index();
    }

    fn remove_entries_matching(&mut self, text: &str) {
        let mut matches = Vec::new();
        for index in 0..self.offsets.len() {
            if self.entry_text(index) == text {
                matches.push(index);
            }
        }
        for index in matches.into_iter().rev() {
            self.offsets.remove(index);
            self.timestamps.remove(index);
            self.local.remove(index);
        }
        self.rebuild_index();
    }

    fn rebuild_index(&mut self) {
        self.index_by_hash.clear();
        for index in 0..self.offsets.len() {
            self.index_by_hash
                .insert(hash_str(self.entry_text(index)), index);
        }
    }
}

pub(super) fn add_history(session: &mut super::session::Session, command: &str) {
    let timestamp = session.history_timestamp_ms();
    session.history.add(command, timestamp);
}

pub fn parse_history(text: &str) -> Vec<String> {
    let fallback_ts = current_unix_millis();
    text.lines()
        .filter_map(|line| parse_history_line(line, fallback_ts))
        .map(|parsed| parsed.command.to_string())
        .collect()
}

fn push_entry(
    arena: &mut String,
    offsets: &mut Vec<(u32, u16)>,
    timestamps: &mut Vec<u64>,
    index_by_hash: &mut FxHashMap<u64, usize>,
    entry: &str,
    timestamp: u64,
) {
    push_entry_without_timestamp(arena, offsets, index_by_hash, entry);
    timestamps.push(timestamp);
}

fn push_entry_without_timestamp(
    arena: &mut String,
    offsets: &mut Vec<(u32, u16)>,
    index_by_hash: &mut FxHashMap<u64, usize>,
    entry: &str,
) {
    let len = entry.len().min(u16::MAX as usize);
    let available = (u32::MAX as usize).saturating_sub(arena.len());
    let len = len.min(available);
    let entry = &entry[..len];
    let start = arena.len() as u32;
    arena.push_str(entry);
    offsets.push((start, len as u16));
    index_by_hash.insert(hash_str(entry), offsets.len() - 1);
}

fn parse_history_line(line: &str, fallback_ts: u64) -> Option<ParsedHistoryLine<'_>> {
    if line.is_empty() {
        return None;
    }
    if let Some(rest) = line.strip_prefix(ISH_LOG_RECORD_PREFIX) {
        let mut parts = rest.splitn(3, '\t');
        if let (Some(timestamp), Some(session_id), Some(command)) =
            (parts.next(), parts.next(), parts.next())
            && !command.is_empty()
            && let (Ok(timestamp), Ok(_)) = (timestamp.parse::<u64>(), session_id.parse::<u64>())
        {
            return Some(ParsedHistoryLine { command, timestamp });
        }
        return None;
    }
    let mut compact = line.splitn(2, char::is_whitespace);
    if let (Some(timestamp), Some(command)) = (compact.next(), compact.next())
        && let Ok(timestamp) = timestamp.parse::<u64>()
        && !command.is_empty()
    {
        if let Some(command) = command.strip_prefix("0 ") {
            return (!command.is_empty()).then_some(ParsedHistoryLine { command, timestamp });
        }
        return Some(ParsedHistoryLine { command, timestamp });
    }
    let mut legacy = line.splitn(3, char::is_whitespace);
    if let (Some(timestamp), Some(session_id), Some(command)) =
        (legacy.next(), legacy.next(), legacy.next())
        && !command.is_empty()
        && let (Ok(timestamp), Ok(_)) = (timestamp.parse::<u64>(), session_id.parse::<u64>())
    {
        return Some(ParsedHistoryLine { command, timestamp });
    }
    Some(ParsedHistoryLine {
        command: line,
        timestamp: fallback_ts,
    })
}

fn cheap_subsequence_match(candidate: &str, query: &str) -> bool {
    let mut chars = candidate.chars();
    query
        .chars()
        .all(|target| chars.any(|candidate| candidate == target))
}

fn classify_match(query: &[char], text: &str, entry_idx: usize) -> Option<FuzzyMatch> {
    if starts_with_icase(query, text) {
        return Some(contiguous_match(entry_idx, 3, 0, query.len()));
    }
    if let Some(start) = find_substring_icase(query, text, true) {
        return Some(contiguous_match(entry_idx, 2, start, query.len()));
    }
    if let Some(start) = find_substring_icase(query, text, false) {
        return Some(contiguous_match(entry_idx, 1, start, query.len()));
    }
    let (match_positions, match_count) = subsequence_match(query, text)?;
    Some(FuzzyMatch {
        entry_idx,
        match_positions,
        match_count,
        score: 0,
    })
}

fn contiguous_match(entry_idx: usize, score: i16, start: usize, len: usize) -> FuzzyMatch {
    let mut match_positions = [0_u16; 32];
    let count = len.min(match_positions.len()).min(u8::MAX as usize);
    for (offset, slot) in match_positions.iter_mut().take(count).enumerate() {
        *slot = (start + offset) as u16;
    }
    FuzzyMatch {
        entry_idx,
        match_positions,
        match_count: count as u8,
        score,
    }
}

fn compare_fuzzy_match(left: &FuzzyMatch, right: &FuzzyMatch) -> std::cmp::Ordering {
    right
        .score
        .cmp(&left.score)
        .then(right.entry_idx.cmp(&left.entry_idx))
        .then(left.match_count.cmp(&right.match_count))
        .then(left.match_positions.cmp(&right.match_positions))
}

fn starts_with_icase(query: &[char], text: &str) -> bool {
    let mut chars = text.chars();
    for &query_char in query {
        let Some(text_char) = chars.next() else {
            return false;
        };
        if text_char.to_lowercase().next() != Some(query_char) {
            return false;
        }
    }
    true
}

fn find_substring_icase(query: &[char], text: &str, boundary_only: bool) -> Option<usize> {
    if query.is_empty() {
        return Some(0);
    }
    if text.is_ascii() && query.iter().all(|character| character.is_ascii()) {
        return find_substring_icase_ascii(query, text.as_bytes(), boundary_only);
    }
    let chars = text.chars().collect::<Vec<_>>();
    if query.len() > chars.len() {
        return None;
    }
    'start: for start in 0..=chars.len() - query.len() {
        if boundary_only && start > 0 && !is_word_boundary_char(chars[start - 1]) {
            continue;
        }
        for (offset, &query_char) in query.iter().enumerate() {
            if chars[start + offset].to_lowercase().next() != Some(query_char) {
                continue 'start;
            }
        }
        return Some(start);
    }
    None
}

fn find_substring_icase_ascii(query: &[char], text: &[u8], boundary_only: bool) -> Option<usize> {
    if query.len() > text.len() {
        return None;
    }
    'start: for start in 0..=text.len() - query.len() {
        if boundary_only && start > 0 && !is_word_boundary_byte(text[start - 1]) {
            continue;
        }
        for (offset, &query_char) in query.iter().enumerate() {
            if text[start + offset].to_ascii_lowercase() != query_char as u8 {
                continue 'start;
            }
        }
        return Some(start);
    }
    None
}

fn subsequence_match(query: &[char], text: &str) -> Option<([u16; 32], u8)> {
    if query.is_empty() {
        return Some(([0; 32], 0));
    }
    if text.is_ascii() && query.iter().all(|character| character.is_ascii()) {
        return subsequence_match_ascii(query, text);
    }
    subsequence_match_unicode(query, text)
}

fn subsequence_match_ascii(query: &[char], text: &str) -> Option<([u16; 32], u8)> {
    let bytes = text.as_bytes();
    let query_len = query.len();
    let last_query = query[query_len - 1] as u8;
    let mut query_index = 0;
    let mut first_end = 0_usize;
    for (text_index, &byte) in bytes.iter().enumerate() {
        if byte.to_ascii_lowercase() == query[query_index] as u8 {
            query_index += 1;
            if query_index == query_len {
                first_end = text_index;
                break;
            }
        }
    }
    if query_index < query_len {
        return None;
    }

    let mut last_end = first_end;
    for (text_index, &byte) in bytes.iter().enumerate().skip(first_end + 1) {
        if byte.to_ascii_lowercase() == last_query {
            last_end = text_index;
        }
    }
    let (window_start, window_end) = if last_end == first_end {
        (backward_ascii(bytes, query, first_end), first_end)
    } else {
        let start1 = backward_ascii(bytes, query, first_end);
        let start2 = backward_ascii(bytes, query, last_end);
        if last_end - start2 < first_end - start1 {
            (start2, last_end)
        } else {
            (start1, first_end)
        }
    };

    let mut positions = [0_u16; 32];
    let mut query_index = 0;
    for (text_index, &byte) in bytes
        .iter()
        .enumerate()
        .take(window_end + 1)
        .skip(window_start)
    {
        if byte.to_ascii_lowercase() == query[query_index] as u8 {
            positions[query_index] = text_index as u16;
            query_index += 1;
            if query_index == query_len {
                break;
            }
        }
    }
    Some((positions, query_len as u8))
}

fn backward_ascii(bytes: &[u8], query: &[char], end: usize) -> usize {
    let mut query_index = query.len();
    for text_index in (0..=end).rev() {
        if bytes[text_index].to_ascii_lowercase() == query[query_index - 1] as u8 {
            query_index -= 1;
            if query_index == 0 {
                return text_index;
            }
        }
    }
    0
}

fn subsequence_match_unicode(query: &[char], text: &str) -> Option<([u16; 32], u8)> {
    let query_len = query.len();
    let last_query = query[query_len - 1];
    let mut query_index = 0;
    let mut first_end = 0_usize;
    for (text_index, text_char) in text.chars().enumerate() {
        if text_char.to_lowercase().next() == Some(query[query_index]) {
            query_index += 1;
            if query_index == query_len {
                first_end = text_index;
                break;
            }
        }
    }
    if query_index < query_len {
        return None;
    }

    let mut last_end = first_end;
    for (text_index, text_char) in text.chars().enumerate() {
        if text_index > first_end && text_char.to_lowercase().next() == Some(last_query) {
            last_end = text_index;
        }
    }
    let max_end = first_end.max(last_end);
    let chars = text
        .chars()
        .enumerate()
        .take(max_end + 1)
        .collect::<Vec<_>>();
    let start1 = backward_unicode(&chars, query, first_end);
    let (window_start, window_end) = if last_end == first_end {
        (start1, first_end)
    } else {
        let start2 = backward_unicode(&chars, query, last_end);
        if last_end - start2 < first_end - start1 {
            (start2, last_end)
        } else {
            (start1, first_end)
        }
    };

    let mut positions = [0_u16; 32];
    let mut query_index = 0;
    for (text_index, text_char) in text.chars().enumerate() {
        if text_index < window_start {
            continue;
        }
        if text_index > window_end {
            break;
        }
        if text_char.to_lowercase().next() == Some(query[query_index]) {
            positions[query_index] = text_index as u16;
            query_index += 1;
            if query_index == query_len {
                break;
            }
        }
    }
    Some((positions, query_len as u8))
}

fn backward_unicode(chars: &[(usize, char)], query: &[char], end: usize) -> usize {
    let mut query_index = query.len();
    for &(text_index, text_char) in chars.iter().rev() {
        if text_index > end {
            continue;
        }
        if text_char.to_lowercase().next() == Some(query[query_index - 1]) {
            query_index -= 1;
            if query_index == 0 {
                return text_index;
            }
        }
    }
    0
}

fn lowercase_query(query: &str) -> Vec<char> {
    query
        .chars()
        .flat_map(|character| character.to_lowercase())
        .collect()
}

fn is_word_boundary_char(character: char) -> bool {
    matches!(character, '/' | '-' | '_' | '.' | ' ' | '\t')
}

fn is_word_boundary_byte(byte: u8) -> bool {
    matches!(byte, b'/' | b'-' | b'_' | b'.' | b' ' | b'\t')
}

fn hash_str(value: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn current_unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn memchr_count(needle: u8, haystack: &[u8]) -> usize {
    haystack.iter().filter(|&&byte| byte == needle).count()
}

fn cache_path_for(path: &Path) -> PathBuf {
    let mut cache = path.to_path_buf();
    let mut name = cache.file_name().unwrap_or_default().to_os_string();
    name.push(".bin");
    cache.set_file_name(name);
    cache
}

fn lock_path_for(path: &Path) -> PathBuf {
    let mut lock = path.to_path_buf();
    let mut name = lock.file_name().unwrap_or_default().to_os_string();
    name.push(".lock");
    lock.set_file_name(name);
    lock
}

#[cfg(test)]
mod tests {
    use super::{History, parse_history};
    use std::io::Write;

    #[test]
    fn parses_compact_ish_and_legacy_history_records() {
        let history = parse_history(
            "1774022576000 echo compact\n1774022576001 0 echo temporary\n:ish-history:v1\t1774022576002\t7\techo ish\nlegacy line\n",
        );

        assert_eq!(
            history,
            ["echo compact", "echo temporary", "echo ish", "legacy line"]
        );
    }

    #[test]
    fn stores_entries_in_arena_and_returns_borrowed_prefix_matches() {
        let history = History::from_entries(vec![
            "git status".to_string(),
            "cargo test".to_string(),
            "git commit".to_string(),
        ]);

        assert_eq!(history.prefix_search("git", 0), Some("git commit"));
        assert_eq!(history.prefix_search("git", 1), Some("git status"));
        let mut candidates = Vec::new();
        let mut scratch = Vec::new();
        let mut matches = Vec::new();
        history.visible_entry_indices_into(&mut candidates);
        history.fuzzy_search_subset_into("gco", &candidates, &mut scratch, &mut matches, 1);
        assert_eq!(history.get(matches[0].entry_idx), "git commit");
    }

    #[test]
    fn binary_cache_load_syncs_text_tail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history");
        {
            let mut file = std::fs::File::create(&path).unwrap();
            writeln!(file, "1774022576000 echo cached").unwrap();
        }
        let mut history = History::load(Some(path.clone()));
        assert_eq!(history.get(0), "echo cached");
        history.compact();

        {
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            writeln!(file, "1774022576001 echo tail").unwrap();
        }
        let history = History::load(Some(path));

        assert_eq!(history.len(), 2);
        assert_eq!(history.get(0), "echo cached");
        assert_eq!(history.get(1), "echo tail");
    }
}
