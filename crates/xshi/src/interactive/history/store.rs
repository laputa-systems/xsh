//! On-disk representation of the shared history: the append-only text log, the
//! binary cache that compaction folds it into, and the advisory lock and reset
//! marker that keep concurrent shells consistent.
//!
//! Every recorded command is appended to the text log before anything else, so
//! the log plus the cache always hold the whole history. Compaction is a pure
//! disk-to-disk merge under an exclusive lock and never trusts a shell's memory.

use rustc_hash::FxHashMap;
use std::ffi::OsStr;
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub(super) const CACHE_MAGIC: &[u8; 4] = b"ISH\x05";
/// magic(4) + entry_count(4) + arena_size(4) + cwd_arena_size(4)
pub(super) const CACHE_HEADER_SIZE: usize = 16;
const LOG_RECORD_STEM: &str = ":ish-history:v";
const LOG_RECORD_PREFIX: &str = ":ish-history:v1\t";
const LOG_RECORD_PREFIX_V2: &str = ":ish-history:v2\t";

/// Entry lengths are stored as `u16` in memory, so longer commands are not
/// recorded and longer records are ignored when read back.
pub(super) const MAX_ENTRY_LEN: usize = u16::MAX as usize;

/// 1998-01-01T00:00:00 UTC as Unix epoch milliseconds.
const TS_EPOCH_MILLIS: u64 = 883_612_800_000;

pub(super) fn hash_str(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

pub(super) fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// A session id that is unique per process even when two histories are opened
/// within the same millisecond; zero is reserved for records of unknown origin.
pub(super) fn new_session_id() -> u64 {
    static LAST: AtomicU64 = AtomicU64::new(0);
    let candidate =
        now_millis().wrapping_shl(16) ^ u64::from(rustix::process::getpid().as_raw_pid() as u32);
    let mut last = LAST.load(Ordering::Relaxed);
    loop {
        let next = candidate.max(last.wrapping_add(1)).max(1);
        match LAST.compare_exchange_weak(last, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return next,
            Err(seen) => last = seen,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Record {
    pub(super) command: String,
    pub(super) timestamp: u64,
    /// Zero means unknown/legacy.
    pub(super) session_id: u64,
    pub(super) cwd: Option<PathBuf>,
}

/// Whether `command` can be stored in memory and in the NUL-delimited cache.
pub(super) fn is_recordable(command: &str) -> bool {
    !command.is_empty() && command.len() <= MAX_ENTRY_LEN && !command.contains('\0')
}

/// Parses one log line. Plain lines are legacy commands stamped with
/// `fallback_ts`. A line that carries the record prefix but does not parse is a
/// torn or foreign record and is dropped instead of being taken for a command.
pub(super) fn parse_line(line: &str, fallback_ts: u64) -> Option<Record> {
    let record = if let Some(rest) = line.strip_prefix(LOG_RECORD_PREFIX_V2) {
        let mut parts = rest.splitn(4, '\t');
        let (ts, session_id, cwd, command) =
            (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
        Record {
            command: command.to_owned(),
            timestamp: ts.parse().ok()?,
            session_id: session_id.parse().ok()?,
            cwd: unescape_field(cwd)
                .map(PathBuf::from)
                .filter(|cwd| !cwd.as_os_str().is_empty()),
        }
    } else if let Some(rest) = line.strip_prefix(LOG_RECORD_PREFIX) {
        let mut parts = rest.splitn(3, '\t');
        let (ts, session_id, command) = (parts.next()?, parts.next()?, parts.next()?);
        Record {
            command: command.to_owned(),
            timestamp: ts.parse().ok()?,
            session_id: session_id.parse().ok()?,
            cwd: None,
        }
    } else if line.starts_with(LOG_RECORD_STEM) {
        return None;
    } else {
        Record {
            command: line.to_owned(),
            timestamp: fallback_ts,
            session_id: 0,
            cwd: None,
        }
    };
    is_recordable(&record.command).then_some(record)
}

/// One log line, without the trailing newline.
pub(super) fn format_record(
    timestamp: u64,
    session_id: u64,
    cwd: Option<&Path>,
    command: &str,
) -> String {
    match cwd {
        Some(cwd) => format!(
            "{LOG_RECORD_PREFIX_V2}{timestamp}\t{session_id}\t{}\t{command}",
            escape_field(&cwd.to_string_lossy())
        ),
        None => format!("{LOG_RECORD_PREFIX}{timestamp}\t{session_id}\t{command}"),
    }
}

fn escape_field(field: &str) -> String {
    let mut escaped = String::with_capacity(field.len());
    for c in field.chars() {
        match c {
            '\\' => escaped.push_str("\\\\"),
            '\t' => escaped.push_str("\\t"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            _ => escaped.push(c),
        }
    }
    escaped
}

fn unescape_field(field: &str) -> Option<String> {
    let mut unescaped = String::with_capacity(field.len());
    let mut chars = field.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            unescaped.push(c);
            continue;
        }
        match chars.next()? {
            '\\' => unescaped.push('\\'),
            't' => unescaped.push('\t'),
            'n' => unescaped.push('\n'),
            'r' => unescaped.push('\r'),
            _ => return None,
        }
    }
    Some(unescaped)
}

/// Collapses a chronological record stream from a single source to one record
/// per command. The last occurrence wins and keeps its position.
pub(super) fn dedup_last(records: Vec<Record>) -> Vec<Record> {
    let mut last: FxHashMap<&str, usize> =
        FxHashMap::with_capacity_and_hasher(records.len(), Default::default());
    for (index, record) in records.iter().enumerate() {
        last.insert(record.command.as_str(), index);
    }
    let keep: Vec<bool> = records
        .iter()
        .enumerate()
        .map(|(index, record)| last.get(record.command.as_str()) == Some(&index))
        .collect();
    drop(last);
    records
        .into_iter()
        .zip(keep)
        .filter_map(|(record, keep)| keep.then_some(record))
        .collect()
}

/// Lays the log over the cache. A log record replaces the cached entry for the
/// same command only when it is strictly newer, so replaying a log that a
/// crashed compaction failed to truncate cannot reorder entries. Survivors keep
/// the position of their winning record: cached entries first, then the log's.
pub(super) fn overlay(cached: Vec<Record>, log: Vec<Record>) -> Vec<Record> {
    let log = dedup_last(log);
    let cached_at: FxHashMap<&str, usize> = cached
        .iter()
        .enumerate()
        .map(|(index, record)| (record.command.as_str(), index))
        .collect();
    let mut cached_dead = vec![false; cached.len()];
    let log_keep: Vec<bool> = log
        .iter()
        .map(|record| match cached_at.get(record.command.as_str()) {
            Some(&index) if record.timestamp <= cached[index].timestamp => false,
            Some(&index) => {
                cached_dead[index] = true;
                true
            }
            None => true,
        })
        .collect();
    drop(cached_at);
    let survivors = cached
        .into_iter()
        .zip(cached_dead)
        .filter_map(|(record, dead)| (!dead).then_some(record));
    let fresh = log
        .into_iter()
        .zip(log_keep)
        .filter_map(|(record, keep)| keep.then_some(record));
    survivors.chain(fresh).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FileStamp {
    dev: u64,
    ino: u64,
    pub(super) size: u64,
    mtime: i64,
    mtime_nsec: i64,
}

impl FileStamp {
    fn from_metadata(meta: &fs::Metadata) -> Self {
        Self {
            dev: meta.dev(),
            ino: meta.ino(),
            size: meta.size(),
            mtime: meta.mtime(),
            mtime_nsec: meta.mtime_nsec(),
        }
    }

    /// Identity of the underlying file; changes when it is replaced by rename.
    pub(super) fn id(self) -> (u64, u64) {
        (self.dev, self.ino)
    }
}

pub(super) fn file_stamp(path: &Path) -> Option<FileStamp> {
    fs::metadata(path)
        .ok()
        .map(|meta| FileStamp::from_metadata(&meta))
}

fn sibling_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

pub(super) fn history_path_for_home(home: Option<&OsStr>) -> PathBuf {
    if let Some(home) = home {
        PathBuf::from(home).join(".local/share/xshi/history")
    } else {
        PathBuf::from("/tmp/xshi_history")
    }
}

pub(super) fn cache_path_for(path: &Path) -> PathBuf {
    sibling_with_suffix(path, ".bin")
}

/// Where an unreadable cache is set aside by `history rebuild`.
pub(super) fn quarantine_path_for(path: &Path) -> PathBuf {
    sibling_with_suffix(path, ".bin.corrupt")
}

pub(super) fn lock_path_for(path: &Path) -> PathBuf {
    sibling_with_suffix(path, ".lock")
}

pub(super) fn reset_marker_for(path: &Path) -> PathBuf {
    sibling_with_suffix(path, ".reset")
}

/// An advisory `flock` on the history lock file, released on drop. Readers and
/// appenders share it; compaction and reset hold it exclusively, so neither can
/// observe or interleave with a half-finished rewrite.
pub(super) struct HistoryLock {
    _file: fs::File,
}

impl HistoryLock {
    /// Returns `None` when the lock file cannot be opened or the wait expires;
    /// callers decide whether to proceed without it.
    pub(super) fn shared(history: &Path, wait: Duration) -> Option<Self> {
        Self::acquire(history, false, wait)
    }

    pub(super) fn exclusive(history: &Path, wait: Duration) -> Option<Self> {
        Self::acquire(history, true, wait)
    }

    fn acquire(history: &Path, exclusive: bool, wait: Duration) -> Option<Self> {
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path_for(history))
            .ok()?;
        let operation = if exclusive {
            rustix::fs::FlockOperation::NonBlockingLockExclusive
        } else {
            rustix::fs::FlockOperation::NonBlockingLockShared
        };
        let started = Instant::now();
        loop {
            match rustix::fs::flock(&file, operation) {
                Ok(()) => return Some(Self { _file: file }),
                Err(rustix::io::Errno::INTR) => {}
                Err(rustix::io::Errno::WOULDBLOCK) if started.elapsed() < wait => {
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(_) => return None,
            }
        }
    }
}

pub(super) struct TextRead {
    pub(super) records: Vec<Record>,
    /// Offset just past the last byte that was consumed.
    pub(super) consumed: u64,
    /// Identity of the file that was read, taken from the open handle.
    pub(super) stamp: Option<FileStamp>,
}

/// Reads the log from byte `from`. Unless `accept_partial` is set the trailing
/// unterminated line is left unconsumed: it may still be being written, and a
/// later read picks it up once it is complete. Lines that are not valid UTF-8
/// are skipped so they cannot stall progress.
pub(super) fn read_text_records(
    path: &Path,
    from: u64,
    accept_partial: bool,
    fallback_ts: u64,
) -> TextRead {
    let unread = |stamp| TextRead {
        records: Vec::new(),
        consumed: from,
        stamp,
    };
    let Ok(mut file) = fs::File::open(path) else {
        return TextRead {
            records: Vec::new(),
            consumed: 0,
            stamp: None,
        };
    };
    let stamp = file.metadata().ok().map(|meta| FileStamp::from_metadata(&meta));
    let mut data = Vec::new();
    if file.seek(SeekFrom::Start(from)).is_err() || file.read_to_end(&mut data).is_err() {
        return unread(stamp);
    }
    let complete = if accept_partial {
        data.len()
    } else {
        data.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1)
    };
    let mut records = Vec::new();
    for chunk in data[..complete].split(|&b| b == b'\n') {
        if let Ok(line) = std::str::from_utf8(chunk)
            && let Some(record) = parse_line(line, fallback_ts)
        {
            records.push(record);
        }
    }
    TextRead {
        records,
        consumed: from + complete as u64,
        stamp,
    }
}

pub(super) enum CacheRead {
    Missing,
    Entries(Vec<Record>),
    /// Present but not usable; the reason is suitable for a diagnostic.
    Unreadable(String),
}

pub(super) fn read_cache(cache: &Path) -> CacheRead {
    match fs::read(cache) {
        Ok(data) => match decode_cache(&data) {
            Some(records) => CacheRead::Entries(records),
            None => CacheRead::Unreadable(format!("corrupt ({} bytes)", data.len())),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => CacheRead::Missing,
        Err(error) => CacheRead::Unreadable(format!("unreadable: {error}")),
    }
}

/// Decodes the v5 cache:
/// `[magic(4)][entry_count(4)][arena_size(4)][cwd_arena_size(4)]`
/// `[timestamps: N×8][arena: \0-terminated][cwd arena: \0-terminated]`.
/// Any structural inconsistency makes the whole cache unusable.
pub(super) fn decode_cache(data: &[u8]) -> Option<Vec<Record>> {
    if data.len() < CACHE_HEADER_SIZE || &data[..4] != CACHE_MAGIC {
        return None;
    }
    let word = |at: usize| Some(u32::from_le_bytes(data[at..at + 4].try_into().ok()?) as usize);
    let (count, arena_size, cwd_size) = (word(4)?, word(8)?, word(12)?);
    let timestamps_end = CACHE_HEADER_SIZE.checked_add(count.checked_mul(8)?)?;
    let arena_end = timestamps_end.checked_add(arena_size)?;
    if data.len() != arena_end.checked_add(cwd_size)? {
        return None;
    }
    let arena = std::str::from_utf8(&data[timestamps_end..arena_end]).ok()?;
    let cwd_arena = std::str::from_utf8(&data[arena_end..]).ok()?;
    let mut commands = arena.split('\0');
    let mut cwds = cwd_arena.split('\0');
    let mut records = Vec::with_capacity(count);
    for index in 0..count {
        let command = commands.next()?;
        let cwd = cwds.next()?;
        if !is_recordable(command) {
            return None;
        }
        let at = CACHE_HEADER_SIZE + index * 8;
        let stored = u64::from_le_bytes(data[at..at + 8].try_into().ok()?);
        records.push(Record {
            command: command.to_owned(),
            timestamp: stored.wrapping_add(TS_EPOCH_MILLIS),
            session_id: 0,
            cwd: (!cwd.is_empty()).then(|| PathBuf::from(cwd)),
        });
    }
    // Each arena ends with the terminator of its last entry, leaving one empty
    // trailing piece and nothing after it.
    let well_terminated = |mut pieces: std::str::Split<'_, char>| {
        pieces.next() == Some("") && pieces.next().is_none()
    };
    (well_terminated(commands) && well_terminated(cwds)).then_some(records)
}

pub(super) fn encode_cache(records: &[Record]) -> Vec<u8> {
    let mut arena = Vec::new();
    let mut cwd_arena = Vec::new();
    for record in records {
        arena.extend_from_slice(record.command.as_bytes());
        arena.push(0);
        if let Some(cwd) = &record.cwd {
            cwd_arena.extend_from_slice(cwd.to_string_lossy().as_bytes());
        }
        cwd_arena.push(0);
    }
    let mut buf =
        Vec::with_capacity(CACHE_HEADER_SIZE + records.len() * 8 + arena.len() + cwd_arena.len());
    buf.extend_from_slice(CACHE_MAGIC);
    buf.extend_from_slice(&(records.len() as u32).to_le_bytes());
    buf.extend_from_slice(&(arena.len() as u32).to_le_bytes());
    buf.extend_from_slice(&(cwd_arena.len() as u32).to_le_bytes());
    for record in records {
        buf.extend_from_slice(&record.timestamp.wrapping_sub(TS_EPOCH_MILLIS).to_le_bytes());
    }
    buf.extend_from_slice(&arena);
    buf.extend_from_slice(&cwd_arena);
    buf
}

/// Writes the cache through a private temporary file and an atomic rename, so a
/// reader or a crash never sees a partial cache.
pub(super) fn write_cache(cache: &Path, records: &[Record]) -> io::Result<()> {
    if u32::try_from(records.len()).is_err() {
        return Err(io::Error::other("history is too large to cache"));
    }
    let tmp = sibling_with_suffix(cache, &format!(".tmp.{}", std::process::id()));
    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(&encode_cache(records))?;
        file.sync_all()?;
        fs::rename(&tmp, cache)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

pub(super) fn read_reset_marker(path: &Path) -> (u64, Option<FileStamp>) {
    let marker = reset_marker_for(path);
    let generation = fs::read_to_string(&marker)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0);
    (generation, file_stamp(&marker))
}

pub(super) fn write_reset_marker(path: &Path, generation: u64) -> io::Result<()> {
    let marker = reset_marker_for(path);
    let tmp = sibling_with_suffix(&marker, &format!(".tmp.{}", std::process::id()));
    fs::write(&tmp, generation.to_string())?;
    fs::rename(&tmp, &marker).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

pub(super) fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
