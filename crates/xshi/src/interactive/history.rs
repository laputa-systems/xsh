use rustc_hash::{FxHashMap, FxHashSet};
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

mod store;
#[cfg(test)]
mod tests;

use store::{
    CacheRead, FileStamp, HistoryLock, Record, cache_path_for, dedup_last, file_stamp,
    format_record, hash_str, history_path_for_home, is_recordable, new_session_id, now_millis,
    overlay, quarantine_path_for, read_cache, read_reset_marker, read_text_records,
    remove_if_present, reset_marker_for, write_cache, write_reset_marker,
};

/// How long readers and appenders wait for a compaction before proceeding
/// without the lock; a shell must never hang on a stuck peer.
const SHARED_LOCK_WAIT: Duration = Duration::from_secs(1);
/// Compaction is only an optimisation, so it gives up quickly when busy.
const COMPACT_LOCK_WAIT: Duration = Duration::from_millis(250);
/// `history reset` and `history rebuild` are explicit requests and wait longer.
const EXPLICIT_LOCK_WAIT: Duration = Duration::from_secs(2);

pub struct History {
    /// All entry text packed into a single allocation.
    arena: String,
    /// (start, len) byte offsets into `arena` for each entry.
    offsets: Vec<(u32, u16)>,
    /// Epoch milliseconds when each entry was last used. Parallel to `offsets`.
    timestamps: Vec<u64>,
    /// Session ids parallel to `offsets`. Zero means unknown/legacy.
    session_ids: Vec<u64>,
    /// Working directory for each entry. None means unknown/legacy.
    cwds: Vec<Option<PathBuf>>,
    /// Maps command hash to its current in-memory position.
    index_by_hash: FxHashMap<u64, usize>,
    /// Set when two different commands hash alike, making the index incomplete;
    /// lookups then fall back to a scan.
    has_collisions: bool,
    /// The text log. Empty for a history that is never persisted.
    path: PathBuf,
    /// Byte offset into the text log up to which complete lines were consumed.
    /// Enables incremental sync: only bytes appended by other shells are read.
    file_pos: u64,
    /// Device and inode of the log at the last read; a change means the file
    /// was replaced and `file_pos` no longer refers to it.
    text_id: Option<(u64, u64)>,
    /// Stamp of the cache at the last read; a change means another shell
    /// compacted.
    cache_stamp: Option<FileStamp>,
    /// Per-entry flag: true if the entry was added by this shell session
    /// (`add()`).
    local: Vec<bool>,
    /// Entries with timestamps at or before this boundary are considered part
    /// of the session-visible history. Up-arrow sees those entries plus any
    /// entry added by this shell.
    session_cutoff: u64,
    /// Session id used for new entries written by this shell.
    session_id: u64,
    /// Set when the cache was unreadable. Prevents overwriting the (possibly
    /// recoverable) cache file until `history rebuild` sets it aside.
    cache_dirty: bool,
    /// Generation written by `history reset`; stale shells clear their
    /// in-memory entries before they can persist them again.
    reset_generation: u64,
    reset_stamp: Option<FileStamp>,
}

impl History {
    fn empty(path: PathBuf) -> Self {
        let (reset_generation, reset_stamp) = if path.as_os_str().is_empty() {
            (0, None)
        } else {
            read_reset_marker(&path)
        };
        Self {
            arena: String::new(),
            offsets: Vec::new(),
            timestamps: Vec::new(),
            session_ids: Vec::new(),
            cwds: Vec::new(),
            index_by_hash: FxHashMap::default(),
            has_collisions: false,
            path,
            file_pos: 0,
            text_id: None,
            cache_stamp: None,
            local: Vec::new(),
            session_cutoff: 0,
            session_id: new_session_id(),
            cache_dirty: false,
            reset_generation,
            reset_stamp,
        }
    }

    pub fn load() -> Self {
        Self::load_from_home(None)
    }

    pub fn load_from_home(home: Option<&std::ffi::OsStr>) -> Self {
        Self::load_from(history_path_for_home(home))
    }

    /// Loads the cache and the log tail under a shared lock. Loading never
    /// writes: the first compaction creates the cache, so starting a shell
    /// cannot race another shell's rewrite.
    pub fn load_from(path: PathBuf) -> Self {
        let mut hist = Self::empty(path);
        {
            let _lock = HistoryLock::shared(&hist.path, SHARED_LOCK_WAIT);
            hist.resync_from_disk(true);
        }
        hist.session_cutoff = now_millis();
        hist
    }

    /// Create from pre-existing entries (for testing/benchmarks). The result
    /// is never persisted.
    pub fn from_entries(entries: Vec<String>) -> Self {
        let mut hist = Self::empty(PathBuf::new());
        let ts = now_millis();
        let records = entries
            .into_iter()
            .map(|command| Record {
                command,
                timestamp: ts,
                session_id: 0,
                cwd: None,
            })
            .collect();
        hist.apply_records(records);
        hist.session_cutoff = ts;
        hist
    }

    fn is_detached(&self) -> bool {
        self.path.as_os_str().is_empty()
    }

    /// Replaces the view of the disk state with the cache plus the whole log.
    /// The caller holds a lock when it needs a consistent snapshot.
    fn resync_from_disk(&mut self, announce: bool) {
        if self.is_detached() {
            return;
        }
        let cache = cache_path_for(&self.path);
        let cache_stamp = file_stamp(&cache);
        let cached = match read_cache(&cache) {
            CacheRead::Missing => {
                self.cache_dirty = false;
                Vec::new()
            }
            CacheRead::Entries(records) => {
                self.cache_dirty = false;
                records
            }
            CacheRead::Unreadable(why) => {
                self.cache_dirty = true;
                if announce {
                    eprintln!(
                        "xshi: history cache {why} — loading text file only; \
                         `history rebuild` sets it aside and writes a new one"
                    );
                }
                Vec::new()
            }
        };
        self.cache_stamp = cache_stamp;
        let text = read_text_records(&self.path, 0, false, now_millis());
        self.file_pos = text.consumed;
        self.text_id = text.stamp.map(FileStamp::id);
        self.apply_records(overlay(cached, text.records));
    }

    /// Folds `records` (in chronological order) into the in-memory history.
    ///
    /// A record for a command that is already present supersedes it only when
    /// it is strictly newer, and never disturbs an entry this session can
    /// still recall with Up-arrow. Records written by this session are already
    /// in memory and are skipped.
    fn apply_records(&mut self, records: Vec<Record>) {
        let mut stale = Vec::new();
        let mut fresh = Vec::new();
        for record in dedup_last(records) {
            if record.session_id == self.session_id {
                continue;
            }
            if let Some(idx) = self.find_entry_index(hash_str(&record.command), &record.command) {
                if self.is_session_visible(idx) || record.timestamp <= self.timestamps[idx] {
                    continue;
                }
                stale.push(idx);
            }
            fresh.push(record);
        }
        self.remove_entries(stale);
        for record in fresh {
            self.push_entry(
                &record.command,
                record.timestamp,
                record.session_id,
                record.cwd,
                false,
            );
        }
    }

    /// Read new entries written by other shell instances. Costs three `stat`
    /// calls when nothing changed; reads only the new tail when the log grew;
    /// re-reads cache and log when another shell compacted or replaced them.
    /// Called at each prompt and before Ctrl+R history search.
    pub fn sync(&mut self) {
        if self.is_detached() {
            return;
        }
        self.sync_reset_marker();
        let cache_now = file_stamp(&cache_path_for(&self.path));
        let text_now = file_stamp(&self.path);
        let replaced = match text_now {
            Some(text) => {
                self.text_id.is_some_and(|id| id != text.id()) || text.size < self.file_pos
            }
            None => self.file_pos > 0 || self.text_id.is_some(),
        };
        if replaced || cache_now != self.cache_stamp {
            let _lock = HistoryLock::shared(&self.path, SHARED_LOCK_WAIT);
            self.resync_from_disk(false);
            return;
        }
        let Some(text) = text_now else {
            return;
        };
        if text.size == self.file_pos {
            return;
        }
        let _lock = HistoryLock::shared(&self.path, SHARED_LOCK_WAIT);
        let read = read_text_records(&self.path, self.file_pos, false, now_millis());
        self.file_pos = read.consumed;
        self.text_id = read.stamp.map(FileStamp::id);
        self.apply_records(read.records);
    }

    fn sync_reset_marker(&mut self) {
        if file_stamp(&reset_marker_for(&self.path)) == self.reset_stamp {
            return;
        }
        let (generation, stamp) = read_reset_marker(&self.path);
        self.reset_stamp = stamp;
        if generation == self.reset_generation {
            return;
        }
        self.clear_entries();
        self.reset_generation = generation;
        self.cache_dirty = false;
    }

    fn clear_entries(&mut self) {
        self.arena.clear();
        self.offsets.clear();
        self.timestamps.clear();
        self.session_ids.clear();
        self.cwds.clear();
        self.index_by_hash.clear();
        self.has_collisions = false;
        self.local.clear();
        self.file_pos = 0;
        self.text_id = None;
        self.cache_stamp = None;
        self.session_cutoff = now_millis();
    }

    /// Folds the log into the cache and truncates the log. Uses `flock` to
    /// serialize across concurrent shells and skips quietly when another shell
    /// holds the lock: every entry is already in the log, so nothing is lost.
    pub fn compact(&mut self) {
        if self.is_detached() {
            return;
        }
        let Some(lock) = HistoryLock::exclusive(&self.path, COMPACT_LOCK_WAIT) else {
            self.sync();
            return;
        };
        self.compact_locked();
        drop(lock);
    }

    /// Rewrites the cache from the log, setting aside a cache that cannot be
    /// read so that it can still be inspected.
    pub fn rebuild(&mut self) {
        if self.is_detached() {
            return;
        }
        let Some(lock) = HistoryLock::exclusive(&self.path, EXPLICIT_LOCK_WAIT) else {
            eprintln!("xshi: history is busy; try again");
            return;
        };
        let cache = cache_path_for(&self.path);
        if matches!(read_cache(&cache), CacheRead::Unreadable(_)) {
            let quarantine = quarantine_path_for(&self.path);
            match fs::rename(&cache, &quarantine) {
                Ok(()) => eprintln!(
                    "xshi: set aside the unreadable history cache as {}",
                    quarantine.display()
                ),
                Err(error) => {
                    eprintln!("xshi: cannot set aside the history cache: {error}");
                    return;
                }
            }
        }
        self.cache_dirty = false;
        if let Some(count) = self.compact_locked() {
            eprintln!("xshi: rebuilt history cache — {count} entries");
        }
        drop(lock);
    }

    /// The compaction itself: a disk-to-disk merge of the cache and the whole
    /// log, independent of this shell's memory. Returns the number of entries
    /// written.
    fn compact_locked(&mut self) -> Option<usize> {
        self.sync_reset_marker();
        if self.cache_dirty {
            return None;
        }
        let cache = cache_path_for(&self.path);
        let cached = match read_cache(&cache) {
            CacheRead::Missing => Vec::new(),
            CacheRead::Entries(records) => records,
            CacheRead::Unreadable(_) => {
                self.cache_dirty = true;
                return None;
            }
        };
        let cached_count = cached.len();
        // No appender can be mid-write while the exclusive lock is held, so an
        // unterminated final line is a torn write and is recovered, not deferred.
        let text = read_text_records(&self.path, 0, true, now_millis());
        let merged = overlay(cached, text.records);
        if merged.is_empty() {
            return Some(0);
        }
        if merged.len() < cached_count / 2 && cached_count > 100 {
            eprintln!(
                "xshi: refusing to shrink history cache from {cached_count} to {} entries",
                merged.len()
            );
            return None;
        }
        if write_cache(&cache, &merged).is_err() {
            return None;
        }
        // The log's contents are now in the cache.
        match fs::OpenOptions::new().write(true).truncate(true).open(&self.path) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => return None,
        }
        self.file_pos = 0;
        self.text_id = file_stamp(&self.path).map(FileStamp::id);
        self.cache_stamp = file_stamp(&cache);
        let count = merged.len();
        self.apply_records(merged);
        Some(count)
    }

    /// Delete all history and invalidate the in-memory state of other shells.
    pub fn reset(&mut self) -> io::Result<()> {
        if self.is_detached() {
            self.clear_entries();
            return Ok(());
        }
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        // A reset is explicit, so it proceeds even if the lock stays busy.
        let _lock = HistoryLock::exclusive(&self.path, EXPLICIT_LOCK_WAIT);
        let generation = new_session_id();
        write_reset_marker(&self.path, generation)?;
        remove_if_present(&self.path)?;
        remove_if_present(&cache_path_for(&self.path))?;
        remove_if_present(&quarantine_path_for(&self.path))?;

        self.clear_entries();
        self.reset_generation = generation;
        self.reset_stamp = file_stamp(&reset_marker_for(&self.path));
        self.cache_dirty = false;
        Ok(())
    }

    /// Appends an entry to the arena and every parallel vector.
    fn push_entry(
        &mut self,
        command: &str,
        timestamp: u64,
        session_id: u64,
        cwd: Option<PathBuf>,
        local: bool,
    ) -> bool {
        if !is_recordable(command) || self.arena.len() + command.len() > u32::MAX as usize {
            return false;
        }
        let start = self.arena.len() as u32;
        self.arena.push_str(command);
        self.offsets.push((start, command.len() as u16));
        self.timestamps.push(timestamp);
        self.session_ids.push(session_id);
        self.cwds.push(cwd);
        self.local.push(local);
        self.index_insert(hash_str(command), self.offsets.len() - 1);
        true
    }

    /// Add entry. Deduplicates (removes prior occurrence).
    pub fn add(&mut self, line: &str) {
        let cwd = std::env::current_dir().ok();
        self.add_in_dir(line, cwd.as_deref());
    }

    /// Add an entry with the directory in which it was entered. Commands that
    /// cannot be stored (over 64 KiB, or containing NUL) are not recorded.
    pub fn add_in_dir(&mut self, line: &str, cwd: Option<&Path>) {
        self.sync_reset_marker();
        // Collapse newlines to spaces to prevent history file corruption.
        let line = line.trim().replace('\n', " ");
        let line = line.trim();
        if !is_recordable(line) {
            return;
        }

        if let Some(idx) = self.find_entry_index(hash_str(line), line) {
            self.remove_entry_at(idx);
        }
        let timestamp = now_millis();
        if self.push_entry(line, timestamp, self.session_id, cwd.map(Path::to_path_buf), true) {
            self.append_to_file(timestamp, line, cwd);
        }
    }

    pub fn len(&self) -> usize {
        self.offsets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }

    /// Get the timestamp (epoch milliseconds) for entry at index.
    pub fn timestamp(&self, idx: usize) -> u64 {
        self.timestamps[idx]
    }

    /// Prefix search: find entries that start with `prefix`, starting from
    /// the end and skipping `skip` matches. Returns the entry text.
    pub fn prefix_search(&self, prefix: &str, skip: usize) -> Option<&str> {
        self.offsets
            .iter()
            .rev()
            .filter_map(|&(start, len)| {
                let s = &self.arena[start as usize..start as usize + len as usize];
                s.starts_with(prefix).then_some(s)
            })
            .nth(skip)
    }

    /// Get the `skip`'th session-visible entry from the end (for up-arrow
    /// navigation). Session-visible entries are those present when the shell
    /// started plus those added by this shell.
    pub fn session_get(&self, skip: usize) -> Option<&str> {
        self.offsets
            .iter()
            .enumerate()
            .rev()
            .filter(|&(i, _)| self.is_session_visible(i))
            .nth(skip)
            .map(|(_, &(start, len))| &self.arena[start as usize..start as usize + len as usize])
    }

    /// Prefix search over session-visible entries only (for up-arrow with
    /// partial input).
    pub fn session_prefix_search(&self, prefix: &str, skip: usize) -> Option<&str> {
        self.offsets
            .iter()
            .enumerate()
            .rev()
            .filter(|&(i, _)| self.is_session_visible(i))
            .filter_map(|(_, &(start, len))| {
                let s = &self.arena[start as usize..start as usize + len as usize];
                s.starts_with(prefix).then_some(s)
            })
            .nth(skip)
    }

    /// History search used by Ctrl+R.
    ///
    /// Ranking is intentionally simple and recency-friendly:
    /// 1. prefix match
    /// 2. substring match at a word boundary
    /// 3. other substring match
    /// 4. subsequence fallback
    ///
    /// Within a tier, newer entries win.
    pub fn fuzzy_search(&self, query: &str) -> Vec<FuzzyMatch> {
        self.fuzzy_search_scored(query, "")
    }

    /// Like `fuzzy_search` but keeps the old signature used by callers/tests.
    pub fn fuzzy_search_scored(&self, query: &str, cwd: &str) -> Vec<FuzzyMatch> {
        let mut results = Vec::new();
        let cwd = (!cwd.is_empty()).then(|| Path::new(cwd));
        self.fill_search_results(query, &mut results, cwd);
        results
    }

    /// Search with a priority boost for entries recorded in an ancestor directory.
    pub fn fuzzy_search_in_dir(&self, query: &str, cwd: &Path) -> Vec<FuzzyMatch> {
        let mut results = Vec::new();
        self.fill_search_results(query, &mut results, Some(cwd));
        results
    }

    /// Like `fuzzy_search` but appends into a caller-owned Vec (zero-alloc reuse).
    /// Caps at `limit` results since the pager only shows a screenful.
    pub fn fuzzy_search_into(
        &self,
        query: &str,
        results: &mut Vec<FuzzyMatch>,
        limit: usize,
        cwd: &str,
    ) {
        let cwd = (!cwd.is_empty()).then(|| Path::new(cwd));
        self.fuzzy_search_into_with_cwd(query, results, limit, cwd);
    }

    /// Search with cwd weighting into a caller-owned result buffer.
    pub fn fuzzy_search_into_in_dir(
        &self,
        query: &str,
        results: &mut Vec<FuzzyMatch>,
        limit: usize,
        cwd: &Path,
    ) {
        self.fuzzy_search_into_with_cwd(query, results, limit, Some(cwd));
    }

    fn fuzzy_search_into_with_cwd(
        &self,
        query: &str,
        results: &mut Vec<FuzzyMatch>,
        limit: usize,
        cwd: Option<&Path>,
    ) {
        if query.is_ascii() && query.len() <= 32 {
            let mut query_lower = [0u8; 32];
            for (slot, byte) in query_lower.iter_mut().zip(query.bytes()) {
                *slot = byte.to_ascii_lowercase();
            }
            self.fill_search_results_limited_bytes(
                &query_lower[..query.len()],
                results,
                limit,
                cwd,
            );
        } else {
            self.fill_search_results_limited(query, results, limit, cwd);
        }
    }

    /// Fill `out` with session-visible entry indices in recency order.
    pub fn visible_entry_indices_into(&self, out: &mut Vec<usize>) {
        out.clear();
        out.extend(
            (0..self.offsets.len())
                .rev()
                .filter(|&idx| self.is_session_visible(idx)),
        );
    }

    /// Search within an existing candidate set, preserving all matches in
    /// `matched_indices` and the best `limit` ranked results in `results`.
    pub fn fuzzy_search_subset_into(
        &self,
        query: &str,
        candidates: &[usize],
        matched_indices: &mut Vec<usize>,
        results: &mut Vec<FuzzyMatch>,
        limit: usize,
    ) {
        self.fuzzy_search_subset_into_with_cwd(
            query,
            candidates,
            matched_indices,
            results,
            limit,
            None,
        );
    }

    /// Search a candidate set with a priority boost for entries recorded in an
    /// ancestor of `cwd`.
    pub fn fuzzy_search_subset_into_in_dir(
        &self,
        query: &str,
        candidates: &[usize],
        matched_indices: &mut Vec<usize>,
        results: &mut Vec<FuzzyMatch>,
        limit: usize,
        cwd: &Path,
    ) {
        self.fuzzy_search_subset_into_with_cwd(
            query,
            candidates,
            matched_indices,
            results,
            limit,
            Some(cwd),
        );
    }

    fn fuzzy_search_subset_into_with_cwd(
        &self,
        query: &str,
        candidates: &[usize],
        matched_indices: &mut Vec<usize>,
        results: &mut Vec<FuzzyMatch>,
        limit: usize,
        cwd: Option<&Path>,
    ) {
        matched_indices.clear();
        results.clear();
        if limit == 0 {
            return;
        }

        if query.is_empty() {
            matched_indices.extend_from_slice(candidates);
            results.extend(candidates.iter().map(|&idx| FuzzyMatch {
                entry_idx: idx,
                match_positions: [0; 32],
                match_count: 0,
                score: self.cwd_weight(idx, cwd),
            }));
            if cwd.is_some() {
                results.sort_unstable_by(compare_fuzzy_match);
            }
            results.truncate(limit);
            return;
        }

        let query_lower = lowercase_query(query);
        for &idx in candidates {
            let entry = self.entry_text(idx);
            let Some(mut m) = classify_match(&query_lower, entry, idx) else {
                continue;
            };
            m.score += self.cwd_weight(idx, cwd);
            matched_indices.push(idx);
            let insert_at = results
                .binary_search_by(|existing| compare_fuzzy_match(existing, &m))
                .unwrap_or_else(|pos| pos);
            if insert_at >= limit {
                continue;
            }
            results.insert(insert_at, m);
            if results.len() > limit {
                results.pop();
            }
            if results.len() == limit && self.can_stop_search(results, cwd) {
                break;
            }
        }
    }

    /// Get entry text by index.
    pub fn get(&self, idx: usize) -> &str {
        let (start, len) = self.offsets[idx];
        &self.arena[start as usize..start as usize + len as usize]
    }

    /// Appends one record to the log in a single write, under the shared lock
    /// that keeps compaction from truncating the log underneath it.
    fn append_to_file(&self, timestamp: u64, line: &str, cwd: Option<&Path>) {
        if self.is_detached() {
            return;
        }
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _lock = HistoryLock::shared(&self.path, SHARED_LOCK_WAIT);
        let Ok(file) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&self.path)
        else {
            return;
        };
        let mut bytes = String::new();
        // A writer that died mid-record leaves an unterminated line; start a
        // fresh one so this record is not glued onto the torn one.
        if let Ok(meta) = file.metadata()
            && meta.len() > 0
        {
            let mut last = [0u8; 1];
            if file.read_exact_at(&mut last, meta.len() - 1).is_ok() && last[0] != b'\n' {
                bytes.push('\n');
            }
        }
        bytes.push_str(&format_record(timestamp, self.session_id, cwd, line));
        bytes.push('\n');
        let _ = (&file).write_all(bytes.as_bytes());
    }

    fn fill_search_results(&self, query: &str, results: &mut Vec<FuzzyMatch>, cwd: Option<&Path>) {
        results.clear();

        if query.is_empty() {
            results.extend(
                (0..self.offsets.len())
                    .rev()
                    .filter(|&idx| self.is_session_visible(idx))
                    .map(|idx| FuzzyMatch {
                        entry_idx: idx,
                        match_positions: [0; 32],
                        match_count: 0,
                        score: self.cwd_weight(idx, cwd),
                    }),
            );
            if cwd.is_some() {
                results.sort_unstable_by(compare_fuzzy_match);
            }
            return;
        }

        let query_lower = lowercase_query(query);
        for (idx, &(start, len)) in self.offsets.iter().enumerate().rev() {
            if !self.is_session_visible(idx) {
                continue;
            }
            let entry = &self.arena[start as usize..start as usize + len as usize];
            if let Some(mut m) = classify_match(&query_lower, entry, idx) {
                m.score += self.cwd_weight(idx, cwd);
                results.push(m);
            }
        }

        results.sort_unstable_by(compare_fuzzy_match);
    }

    fn fill_search_results_limited(
        &self,
        query: &str,
        results: &mut Vec<FuzzyMatch>,
        limit: usize,
        cwd: Option<&Path>,
    ) {
        let query_lower = lowercase_query(query);
        self.fill_search_results_limited_chars(&query_lower, results, limit, cwd);
    }

    fn fill_search_results_limited_bytes(
        &self,
        query_lower: &[u8],
        results: &mut Vec<FuzzyMatch>,
        limit: usize,
        cwd: Option<&Path>,
    ) {
        results.clear();
        if limit == 0 {
            return;
        }

        if query_lower.is_empty() {
            results.extend(
                (0..self.offsets.len())
                    .rev()
                    .filter(|&idx| self.is_session_visible(idx))
                    .take(limit)
                    .map(|idx| FuzzyMatch {
                        entry_idx: idx,
                        match_positions: [0; 32],
                        match_count: 0,
                        score: self.cwd_weight(idx, cwd),
                    }),
            );
            if cwd.is_some() {
                results.sort_unstable_by(compare_fuzzy_match);
                results.truncate(limit);
            }
            return;
        }

        for (idx, &(start, len)) in self.offsets.iter().enumerate().rev() {
            if !self.is_session_visible(idx) {
                continue;
            }
            let entry = &self.arena[start as usize..start as usize + len as usize];
            let Some(mut m) = classify_match_ascii(query_lower, entry, idx) else {
                continue;
            };
            m.score += self.cwd_weight(idx, cwd);

            let insert_at = results
                .binary_search_by(|existing| compare_fuzzy_match(existing, &m))
                .unwrap_or_else(|pos| pos);
            if insert_at >= limit {
                continue;
            }
            results.insert(insert_at, m);
            if results.len() > limit {
                results.pop();
            }
            if results.len() == limit && self.can_stop_search(results, cwd) {
                break;
            }
        }
    }

    fn fill_search_results_limited_chars(
        &self,
        query_lower: &[char],
        results: &mut Vec<FuzzyMatch>,
        limit: usize,
        cwd: Option<&Path>,
    ) {
        results.clear();
        if limit == 0 {
            return;
        }

        if query_lower.is_empty() {
            results.extend(
                (0..self.offsets.len())
                    .rev()
                    .filter(|&idx| self.is_session_visible(idx))
                    .take(limit)
                    .map(|idx| FuzzyMatch {
                        entry_idx: idx,
                        match_positions: [0; 32],
                        match_count: 0,
                        score: self.cwd_weight(idx, cwd),
                    }),
            );
            if cwd.is_some() {
                results.sort_unstable_by(compare_fuzzy_match);
                results.truncate(limit);
            }
            return;
        }

        for (idx, &(start, len)) in self.offsets.iter().enumerate().rev() {
            if !self.is_session_visible(idx) {
                continue;
            }
            let entry = &self.arena[start as usize..start as usize + len as usize];
            let Some(mut m) = classify_match(query_lower, entry, idx) else {
                continue;
            };
            m.score += self.cwd_weight(idx, cwd);

            let insert_at = results
                .binary_search_by(|existing| compare_fuzzy_match(existing, &m))
                .unwrap_or_else(|pos| pos);
            if insert_at >= limit {
                continue;
            }
            results.insert(insert_at, m);
            if results.len() > limit {
                results.pop();
            }
            if results.len() == limit && self.can_stop_search(results, cwd) {
                break;
            }
        }
    }

    fn cwd_weight(&self, idx: usize, cwd: Option<&Path>) -> i16 {
        const CWD_WEIGHT: i16 = 4;
        if cwd.is_some_and(|cwd| {
            self.cwds[idx]
                .as_deref()
                .is_some_and(|entry_cwd| cwd.starts_with(entry_cwd))
        }) {
            CWD_WEIGHT
        } else {
            0
        }
    }

    fn can_stop_search(&self, results: &[FuzzyMatch], cwd: Option<&Path>) -> bool {
        let best_possible_score = if cwd.is_some() { 7 } else { 3 };
        results
            .last()
            .is_some_and(|m| m.score >= best_possible_score)
    }

    fn is_session_visible(&self, idx: usize) -> bool {
        self.local[idx] || self.timestamps[idx] <= self.session_cutoff
    }

    fn entry_text(&self, idx: usize) -> &str {
        let (start, len) = self.offsets[idx];
        &self.arena[start as usize..start as usize + len as usize]
    }

    fn find_entry_index(&self, hash: u64, text: &str) -> Option<usize> {
        let hit = self
            .index_by_hash
            .get(&hash)
            .copied()
            .filter(|&idx| self.entry_text(idx) == text);
        if hit.is_some() || !self.has_collisions {
            return hit;
        }
        (0..self.offsets.len()).find(|&idx| self.entry_text(idx) == text)
    }

    fn index_insert(&mut self, hash: u64, idx: usize) {
        if let Some(previous) = self.index_by_hash.insert(hash, idx)
            && self.entry_text(previous) != self.entry_text(idx)
        {
            self.has_collisions = true;
        }
    }

    fn remove_entry_at(&mut self, idx: usize) {
        self.offsets.remove(idx);
        self.timestamps.remove(idx);
        self.session_ids.remove(idx);
        self.cwds.remove(idx);
        self.local.remove(idx);
        if self.has_collisions {
            self.rebuild_index();
        } else {
            self.index_by_hash.retain(|_, position| {
                if *position == idx {
                    return false;
                }
                if *position > idx {
                    *position -= 1;
                }
                true
            });
        }
    }

    /// Removes several entries in one pass.
    fn remove_entries(&mut self, mut indices: Vec<usize>) {
        if indices.is_empty() {
            return;
        }
        indices.sort_unstable();
        indices.dedup();
        let mut drop_entry = vec![false; self.offsets.len()];
        for idx in indices {
            drop_entry[idx] = true;
        }
        fn retain_kept<T>(items: &mut Vec<T>, drop_entry: &[bool]) {
            let mut position = 0;
            items.retain(|_| {
                position += 1;
                !drop_entry[position - 1]
            });
        }
        retain_kept(&mut self.offsets, &drop_entry);
        retain_kept(&mut self.timestamps, &drop_entry);
        retain_kept(&mut self.session_ids, &drop_entry);
        retain_kept(&mut self.cwds, &drop_entry);
        retain_kept(&mut self.local, &drop_entry);
        self.rebuild_index();
    }

    fn rebuild_index(&mut self) {
        self.index_by_hash.clear();
        self.has_collisions = false;
        for idx in 0..self.offsets.len() {
            self.index_insert(hash_str(self.entry_text(idx)), idx);
        }
    }
}

fn compare_fuzzy_match(a: &FuzzyMatch, b: &FuzzyMatch) -> std::cmp::Ordering {
    b.score.cmp(&a.score).then(b.entry_idx.cmp(&a.entry_idx))
}

/// Lowercase a query into a fixed stack buffer, returning the used slice.
fn lowercase_query(query: &str) -> Vec<char> {
    query.chars().flat_map(|c| c.to_lowercase()).collect()
}

#[derive(Debug)]
pub struct FuzzyMatch {
    pub entry_idx: usize,
    /// Matched character indices (as u16 — entries are always <64K chars).
    pub match_positions: [u16; 32],
    pub match_count: u8,
    /// Match tier. Higher = stronger literal match.
    /// 3 = prefix, 2 = boundary substring, 1 = substring, 0 = subsequence fallback.
    pub score: i16,
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

    let (positions, count) = subsequence_match(query, text)?;
    Some(FuzzyMatch {
        entry_idx,
        match_positions: positions,
        match_count: count,
        score: 0,
    })
}

fn classify_match_ascii(query: &[u8], text: &str, entry_idx: usize) -> Option<FuzzyMatch> {
    if starts_with_icase_ascii(query, text.as_bytes()) {
        return Some(contiguous_match(entry_idx, 3, 0, query.len()));
    }

    if let Some(start) = find_substring_icase_ascii_bytes(query, text.as_bytes(), true) {
        return Some(contiguous_match(entry_idx, 2, start, query.len()));
    }

    if let Some(start) = find_substring_icase_ascii_bytes(query, text.as_bytes(), false) {
        return Some(contiguous_match(entry_idx, 1, start, query.len()));
    }

    let (positions, count) = subsequence_match_ascii_bytes(query, text.as_bytes())?;
    Some(FuzzyMatch {
        entry_idx,
        match_positions: positions,
        match_count: count,
        score: 0,
    })
}

fn contiguous_match(entry_idx: usize, score: i16, start: usize, len: usize) -> FuzzyMatch {
    let mut positions = [0u16; 32];
    let count = len.min(positions.len()).min(u8::MAX as usize);
    for (offset, slot) in positions.iter_mut().take(count).enumerate() {
        *slot = (start + offset) as u16;
    }
    FuzzyMatch {
        entry_idx,
        match_positions: positions,
        match_count: count as u8,
        score,
    }
}

fn starts_with_icase(query: &[char], text: &str) -> bool {
    let mut chars = text.chars();
    for &q in query {
        let Some(tc) = chars.next() else {
            return false;
        };
        if tc.to_lowercase().next() != Some(q) {
            return false;
        }
    }
    true
}

fn starts_with_icase_ascii(query: &[u8], text: &[u8]) -> bool {
    text.len() >= query.len()
        && text[..query.len()]
            .iter()
            .zip(query)
            .all(|(&text_byte, &query_byte)| text_byte.to_ascii_lowercase() == query_byte)
}

fn find_substring_icase(query: &[char], text: &str, boundary_only: bool) -> Option<usize> {
    if query.is_empty() {
        return Some(0);
    }

    if text.is_ascii() && query.iter().all(|c| c.is_ascii()) {
        return find_substring_icase_ascii(query, text.as_bytes(), boundary_only);
    }

    let chars: Vec<char> = text.chars().collect();
    if query.len() > chars.len() {
        return None;
    }

    for start in 0..=chars.len() - query.len() {
        if boundary_only && start > 0 && !is_word_boundary_char(chars[start - 1]) {
            continue;
        }
        if chars[start..start + query.len()]
            .iter()
            .zip(query.iter())
            .all(|(&tc, &q)| tc.to_lowercase().next() == Some(q))
        {
            return Some(start);
        }
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
        for (offset, &q) in query.iter().enumerate() {
            if text[start + offset].to_ascii_lowercase() != q as u8 {
                continue 'start;
            }
        }
        return Some(start);
    }

    None
}

fn find_substring_icase_ascii_bytes(
    query: &[u8],
    text: &[u8],
    boundary_only: bool,
) -> Option<usize> {
    if query.len() > text.len() {
        return None;
    }

    'start: for start in 0..=text.len() - query.len() {
        if boundary_only && start > 0 && !is_word_boundary_byte(text[start - 1]) {
            continue;
        }
        for (offset, &query_byte) in query.iter().enumerate() {
            if text[start + offset].to_ascii_lowercase() != query_byte {
                continue 'start;
            }
        }
        return Some(start);
    }

    None
}

/// Check if `query` chars appear in `text` in order (case-insensitive).
/// Uses a forward-then-backward scan to find the tightest match window,
/// then a final forward pass within that window for optimal positions.
/// Returns a fixed-size array of matched character indices and the count.
/// Zero heap allocations — uses stack arrays only.
pub fn subsequence_match(query: &[char], text: &str) -> Option<([u16; 32], u8)> {
    if query.is_empty() {
        return Some(([0; 32], 0));
    }

    // ASCII fast path: if both query and text are ASCII, operate on bytes directly.
    if text.is_ascii() && query.iter().all(|c| c.is_ascii()) {
        return subsequence_match_ascii(query, text);
    }

    subsequence_match_unicode(query, text)
}

/// ASCII fast path — operates on bytes directly, no char decoding.
fn subsequence_match_ascii(query: &[char], text: &str) -> Option<([u16; 32], u8)> {
    let bytes = text.as_bytes();
    let qlen = query.len();
    let last_qchar = query[qlen - 1] as u8;

    // 1) Forward pass: find the first complete match to confirm it exists.
    let mut qi = 0;
    let mut first_end = 0usize; // index of the first endpoint (last query char match)
    for (ti, &b) in bytes.iter().enumerate() {
        if b.to_ascii_lowercase() == query[qi] as u8 {
            qi += 1;
            if qi == qlen {
                first_end = ti;
                break;
            }
        }
    }
    if qi < qlen {
        return None;
    }

    // 2) Find the last occurrence of the last query char beyond the first endpoint.
    let mut last_end = first_end;
    for (ti, &b) in bytes.iter().enumerate().skip(first_end + 1) {
        if b.to_ascii_lowercase() == last_qchar {
            last_end = ti;
        }
    }

    // 3) Backward pass from both endpoints; pick the tighter window.
    let (window_start, window_end) = if last_end == first_end {
        (backward_ascii(bytes, query, first_end), first_end)
    } else {
        let start1 = backward_ascii(bytes, query, first_end);
        let start2 = backward_ascii(bytes, query, last_end);
        let span1 = first_end - start1;
        let span2 = last_end - start2;
        if span2 < span1 {
            (start2, last_end)
        } else {
            (start1, first_end)
        }
    };

    // 4) Forward pass within the tight window to record optimal positions.
    let mut positions = [0u16; 32];
    let mut qi2 = 0;
    for (ti, &b) in bytes
        .iter()
        .enumerate()
        .take(window_end + 1)
        .skip(window_start)
    {
        if b.to_ascii_lowercase() == query[qi2] as u8 {
            positions[qi2] = ti as u16;
            qi2 += 1;
            if qi2 == qlen {
                break;
            }
        }
    }

    Some((positions, qlen as u8))
}

fn subsequence_match_ascii_bytes(query: &[u8], text: &[u8]) -> Option<([u16; 32], u8)> {
    let qlen = query.len();
    let last_qchar = query[qlen - 1];

    let mut qi = 0;
    let mut first_end = 0usize;
    for (ti, &byte) in text.iter().enumerate() {
        if byte.to_ascii_lowercase() == query[qi] {
            qi += 1;
            if qi == qlen {
                first_end = ti;
                break;
            }
        }
    }
    if qi < qlen {
        return None;
    }

    let mut last_end = first_end;
    for (ti, &byte) in text.iter().enumerate().skip(first_end + 1) {
        if byte.to_ascii_lowercase() == last_qchar {
            last_end = ti;
        }
    }

    let (window_start, window_end) = if last_end == first_end {
        (backward_ascii_bytes(text, query, first_end), first_end)
    } else {
        let start1 = backward_ascii_bytes(text, query, first_end);
        let start2 = backward_ascii_bytes(text, query, last_end);
        let span1 = first_end - start1;
        let span2 = last_end - start2;
        if span2 < span1 {
            (start2, last_end)
        } else {
            (start1, first_end)
        }
    };

    let mut positions = [0u16; 32];
    let mut qi2 = 0;
    for (ti, &byte) in text
        .iter()
        .enumerate()
        .take(window_end + 1)
        .skip(window_start)
    {
        if byte.to_ascii_lowercase() == query[qi2] {
            positions[qi2] = ti as u16;
            qi2 += 1;
            if qi2 == qlen {
                break;
            }
        }
    }

    Some((positions, qlen as u8))
}

/// Backward scan from `end` (inclusive) to find the tightest window start.
fn backward_ascii(bytes: &[u8], query: &[char], end: usize) -> usize {
    let mut qi = query.len();
    for ti in (0..=end).rev() {
        if bytes[ti].to_ascii_lowercase() == query[qi - 1] as u8 {
            qi -= 1;
            if qi == 0 {
                return ti;
            }
        }
    }
    0 // unreachable if forward pass confirmed the match
}

fn backward_ascii_bytes(bytes: &[u8], query: &[u8], end: usize) -> usize {
    let mut qi = query.len();
    for ti in (0..=end).rev() {
        if bytes[ti].to_ascii_lowercase() == query[qi - 1] {
            qi -= 1;
            if qi == 0 {
                return ti;
            }
        }
    }
    0
}

/// Unicode path — operates on chars.
fn subsequence_match_unicode(query: &[char], text: &str) -> Option<([u16; 32], u8)> {
    let qlen = query.len();
    let last_qchar = query[qlen - 1];

    // 1) Forward pass to confirm match exists and find first endpoint.
    let mut qi = 0;
    let mut first_end = 0usize;
    for (ti, tc) in text.chars().enumerate() {
        if tc.to_lowercase().next() == Some(query[qi]) {
            qi += 1;
            if qi == qlen {
                first_end = ti;
                break;
            }
        }
    }
    if qi < qlen {
        return None;
    }

    // 2) Find last occurrence of the last query char.
    let mut last_end = first_end;
    for (ti, tc) in text.chars().enumerate() {
        if ti > first_end && tc.to_lowercase().next() == Some(last_qchar) {
            last_end = ti;
        }
    }

    // 3) Backward pass from both endpoints; pick tighter window.
    // Collect (char_idx, char) pairs up to max(first_end, last_end) for reverse scanning.
    let max_end = first_end.max(last_end);
    // Use a Vec here since this is the non-ASCII slow path (rare).
    let chars_vec: Vec<(usize, char)> = text.chars().enumerate().take(max_end + 1).collect();

    let start1 = backward_unicode(&chars_vec, query, first_end);
    let (window_start, window_end) = if last_end == first_end {
        (start1, first_end)
    } else {
        let start2 = backward_unicode(&chars_vec, query, last_end);
        let span1 = first_end - start1;
        let span2 = last_end - start2;
        if span2 < span1 {
            (start2, last_end)
        } else {
            (start1, first_end)
        }
    };

    // 4) Forward pass within the tight window to record optimal positions.
    let mut positions = [0u16; 32];
    let mut qi2 = 0;
    for (ti, tc) in text.chars().enumerate() {
        if ti < window_start {
            continue;
        }
        if ti > window_end {
            break;
        }
        if tc.to_lowercase().next() == Some(query[qi2]) {
            positions[qi2] = ti as u16;
            qi2 += 1;
            if qi2 == qlen {
                break;
            }
        }
    }

    Some((positions, qlen as u8))
}

/// Backward scan through collected chars to find tightest window start.
fn backward_unicode(chars: &[(usize, char)], query: &[char], end: usize) -> usize {
    let mut qi = query.len();
    for &(ci, ch) in chars.iter().rev() {
        if ci > end {
            continue;
        }
        if ch.to_lowercase().next() == Some(query[qi - 1]) {
            qi -= 1;
            if qi == 0 {
                return ci;
            }
        }
    }
    0
}

fn is_word_boundary_char(c: char) -> bool {
    matches!(c, '/' | '-' | '_' | '.' | ' ' | '\t')
}

fn is_word_boundary_byte(b: u8) -> bool {
    matches!(b, b'/' | b'-' | b'_' | b'.' | b' ' | b'\t')
}

/// Compatibility helper retained for benchmarks.
/// Returns the literal-match tier for a precomputed match window.
pub fn score_match(positions: &[u16; 32], count: u8, text: &str, _pwd_basename: &str) -> i16 {
    let n = count as usize;
    if n == 0 {
        return 0;
    }

    let start = positions[0] as usize;
    for i in 1..n {
        if positions[i] != positions[i - 1] + 1 {
            return 0;
        }
    }

    if start == 0 {
        3
    } else if text
        .chars()
        .nth(start.saturating_sub(1))
        .is_some_and(is_word_boundary_char)
    {
        2
    } else {
        1
    }
}
