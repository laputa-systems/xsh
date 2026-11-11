use super::*;
use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().expect("temporary history directory"),
        }
    }

    fn path(&self) -> PathBuf {
        self.dir.path().join("history")
    }

    fn log(&self) -> PathBuf {
        store::storage_path(&self.path())
    }

    fn occurrences(&self) -> usize {
        Store::open(&self.path())
            .unwrap()
            .snapshot(-1, 0)
            .unwrap()
            .occurrences
            .len()
    }

    fn load(&self) -> History {
        History::try_load_from(self.path()).unwrap()
    }
}

fn record(timestamp: u64, command: &str) -> String {
    format!(":ish-history:v2\t{timestamp}\t7\t/work/project\t{command}\n")
}

fn cache_fixture(commands: &[&str], timestamps: &[u64]) -> Vec<u8> {
    const EPOCH: u64 = 883_612_800_000;
    let arena: String = commands.iter().map(|command| format!("{command}\0")).collect();
    let cwd_arena: String = commands.iter().map(|_| "/cached\0").collect();
    let mut bytes = b"ISH\x05".to_vec();
    bytes.extend_from_slice(&(commands.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(arena.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(cwd_arena.len() as u32).to_le_bytes());
    for &timestamp in timestamps {
        bytes.extend_from_slice(&timestamp.wrapping_sub(EPOCH).to_le_bytes());
    }
    bytes.extend_from_slice(arena.as_bytes());
    bytes.extend_from_slice(cwd_arena.as_bytes());
    bytes
}

fn append_raw(path: &Path, bytes: &[u8]) {
    use std::io::Write as _;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    file.write_all(bytes).unwrap();
}

fn use_command(history: &mut History, command: &str, timestamp: u64, cwd: Option<&Path>) {
    let id = history.last_id + 1;
    history.apply_occurrence(Occurrence {
        id,
        command: command.into(),
        timestamp,
        session_id: 0,
        cwd: cwd.map(Path::to_path_buf),
    });
}

#[test]
fn migrates_cache_and_text_and_retires_originals() {
    let fixture = Fixture::new();
    let timestamp = 1_700_000_000_000;
    fs::write(
        legacy::cache_path(&fixture.path()),
        cache_fixture(&["cached", "repeated"], &[timestamp, timestamp]),
    )
    .unwrap();
    fs::write(
        fixture.path(),
        record(timestamp + 1000, "repeated") + &record(timestamp + 2000, "repeated"),
    )
    .unwrap();

    let history = fixture.load();
    assert_eq!(history.len(), 2);
    assert_eq!(fixture.occurrences(), 4);
    let repeated = history
        .find_entry_index(hash_str("repeated"), "repeated")
        .unwrap();
    assert_eq!(history.frequency(repeated), 3);
    assert_eq!(history.timestamp(repeated), timestamp + 2000);
    assert_eq!(
        history
            .directory_usage(repeated, Path::new("/work/project"))
            .unwrap()
            .count,
        2
    );
    assert!(!fixture.path().exists());
    assert!(!legacy::cache_path(&fixture.path()).exists());
}

#[test]
fn deleting_the_log_after_migration_does_not_resurrect_legacy_history() {
    let fixture = Fixture::new();
    fs::write(fixture.path(), record(1_700_000_000_000, "secret text")).unwrap();
    fs::write(
        legacy::cache_path(&fixture.path()),
        cache_fixture(&["secret cache"], &[1_700_000_000_000]),
    )
    .unwrap();
    assert_eq!(fixture.occurrences(), 2);
    fs::remove_file(fixture.log()).unwrap();
    assert_eq!(fixture.occurrences(), 0);
    assert!(!fixture.path().exists());
    assert!(!legacy::cache_path(&fixture.path()).exists());
}

#[test]
fn simultaneous_open_migrates_once_and_appends_without_lost_occurrences() {
    let fixture = Fixture::new();
    fs::write(fixture.path(), "seed\n").unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let threads: Vec<_> = (0..2)
        .map(|session| {
            let path = fixture.path();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                let mut history = History::try_load_from(path).unwrap();
                for i in 0..20 {
                    history.append_in_dir(&format!("session {session} {i}"), None).unwrap();
                }
            })
        })
        .collect();
    barrier.wait();
    for thread in threads {
        thread.join().unwrap();
    }
    let history = fixture.load();
    assert_eq!(history.len(), 41);
    assert_eq!(fixture.occurrences(), 41);
}

#[test]
fn session_recall_stays_fixed_while_global_usage_synchronizes() {
    let fixture = Fixture::new();
    fs::write(fixture.path(), "first\nsecond\n").unwrap();
    let mut local = fixture.load();
    let mut other = fixture.load();
    let first_idx = local.find_entry_index(hash_str("first"), "first").unwrap();
    other
        .append_in_dir("first", Some(Path::new("/elsewhere")))
        .unwrap();
    other.append_in_dir("foreign", None).unwrap();
    local.sync();
    assert_eq!(local.session_get(0), Some("second"));
    assert_eq!(local.session_get(1), Some("first"));
    assert_eq!(local.session_get(2), None);
    assert_eq!(local.get(first_idx), "first");
    assert_eq!(local.frequency(first_idx), 2);
    assert_eq!(local.fuzzy_search("foreign").len(), 1);
    local.append_in_dir("first", None).unwrap();
    assert_eq!(local.session_get(0), Some("first"));
    assert_eq!(local.session_get(1), Some("second"));
    assert_eq!(local.session_prefix_search("foreign", 0), None);
    other.append_in_dir("unsynced", None).unwrap();
    local.append_in_dir("mine", None).unwrap();
    assert_eq!(local.fuzzy_search("unsynced").len(), 1);
    assert_eq!(local.session_get(0), Some("mine"));
    assert_eq!(fixture.occurrences(), 7);
}

#[test]
fn reset_invalidates_stale_sessions_before_their_next_append() {
    let fixture = Fixture::new();
    fs::write(fixture.path(), "legacy\n").unwrap();
    let mut resetting = fixture.load();
    let mut stale = fixture.load();
    resetting.append_in_dir("old", None).unwrap();
    resetting.reset().unwrap();
    stale.append_in_dir("after reset", None).unwrap();
    assert_eq!(stale.len(), 1);
    assert_eq!(stale.session_get(0), Some("after reset"));
    resetting.sync();
    assert_eq!(resetting.len(), 1);
    assert_eq!(resetting.session_get(0), None);
    assert_eq!(fixture.occurrences(), 1);
    let reopened = fixture.load();
    assert_eq!(reopened.get(0), "after reset");
}

#[test]
fn append_failure_does_not_update_memory_or_usage() {
    let fixture = Fixture::new();
    let mut history = fixture.load();
    history.append_in_dir("saved", None).unwrap();
    let path = fixture.log();
    let saved = fs::read(&path).unwrap();
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(history.append_in_dir("unsaved", None).is_err());
    assert_eq!(history.len(), 1);
    assert_eq!(history.session_get(0), Some("saved"));
    assert_eq!(history.frequency(0), 1);
    fs::remove_dir(&path).unwrap();
    fs::write(&path, saved).unwrap();
    assert_eq!(fixture.occurrences(), 1);
}

#[test]
fn commands_are_stored_verbatim_and_display_on_one_row() {
    let fixture = Fixture::new();
    let mut history = fixture.load();
    history.append_in_dir("echo 'a\nb'\t", None).unwrap();
    assert_eq!(history.get(0), "echo 'a\nb'\t");
    assert_eq!(history.display(0), "echo 'a\u{21b5}b'\u{fffd}");
    let reopened = fixture.load();
    assert_eq!(reopened.get(0), "echo 'a\nb'\t");
}

#[test]
fn cwd_bytes_and_full_unicode_commands_round_trip() {
    let fixture = Fixture::new();
    let mut history = fixture.load();
    let cwd = PathBuf::from(OsString::from_vec(b"/work/\xff\t\\".to_vec()));
    let command = "\u{e9}".repeat(40_000) + " target";
    history.append_in_dir(&command, Some(&cwd)).unwrap();
    drop(history);
    let loaded = fixture.load();
    assert_eq!(loaded.get(0), command);
    assert_eq!(
        loaded
            .directories(0)
            .next()
            .unwrap()
            .as_os_str()
            .as_bytes(),
        cwd.as_os_str().as_bytes()
    );
    let matches = loaded.fuzzy_search("target");
    assert_eq!(
        matches[0].match_positions[..6],
        [40_001, 40_002, 40_003, 40_004, 40_005, 40_006]
    );
}

#[test]
fn log_and_lock_permissions_are_private() {
    let fixture = Fixture::new();
    let mut history = fixture.load();
    history.append_in_dir("secret", None).unwrap();
    for path in [fixture.log(), store::storage_path(&fixture.path()).with_extension("log.lock")] {
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "{}", path.display());
    }
    let dir = fixture.dir.path().join("nested");
    let mut nested = History::try_load_from(dir.join("history")).unwrap();
    nested.append_in_dir("secret", None).unwrap();
    let mode = fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700);
}

#[test]
fn unsupported_or_corrupt_log_headers_are_preserved() {
    let fixture = Fixture::new();
    fs::write(fixture.log(), b"not a history log").unwrap();
    assert!(History::try_load_from(fixture.path()).is_err());
    assert_eq!(fs::read(fixture.log()).unwrap(), b"not a history log");
}

#[test]
fn incomplete_final_frames_recover_the_valid_prefix() {
    let fixture = Fixture::new();
    let mut history = fixture.load();
    history.append_in_dir("/bin/echo durable_before_partial_append", None).unwrap();
    drop(history);
    append_raw(&fixture.log(), &[1, 2, 3]);
    let recovered = fixture.load();
    assert_eq!(recovered.len(), 1);
    assert_eq!(
        recovered.prefix_search("/bin/echo durable_before_partial_append", 0),
        Some("/bin/echo durable_before_partial_append")
    );
    let mut after = recovered;
    after.append_in_dir("/bin/echo durable_after_partial_append", None).unwrap();
    let reopened = fixture.load();
    assert_eq!(reopened.len(), 2);
}

#[test]
fn read_only_render_matches_actual_time_order_without_mutating_the_store() {
    let fixture = Fixture::new();
    let mut store = Store::open(&fixture.path()).unwrap();
    for (last_id, timestamp, command) in [
        (0, 1_700_000_000_000, "older"),
        (1, 1_700_000_001_000, "newer"),
    ] {
        store
            .append(
                0,
                last_id,
                Occurrence {
                    id: 0,
                    command: command.into(),
                    timestamp,
                    session_id: 7,
                    cwd: None,
                },
            )
            .unwrap();
    }
    drop(store);
    let log = fixture.log();
    let before = fs::read(&log).unwrap();
    let rendered = render_history_log(&fixture.log()).unwrap();
    assert_eq!(rendered, "older\nnewer\n");
    assert_eq!(fs::read(&log).unwrap(), before);
}

#[test]
fn single_directory_usage_shares_command_totals() {
    let dir = PathBuf::from("/work/a");
    let history = History::from_entries_in_dirs([
        ("cargo build".to_string(), Some(dir.clone())),
        ("cargo build".to_string(), Some(dir.clone())),
    ]);
    assert_eq!(history.frequency(0), 2);
    assert_eq!(history.directory_usage(0, &dir).unwrap().count, 2);
    assert_eq!(
        history.directory_usage(0, &dir).unwrap().timestamp,
        history.timestamp(0)
    );
}

#[test]
fn occurrence_without_directory_detaches_shared_totals() {
    let dir = PathBuf::from("/work/a");
    let history = History::from_entries_in_dirs([
        ("cargo build".to_string(), Some(dir.clone())),
        ("cargo build".to_string(), None),
    ]);
    assert_eq!(history.frequency(0), 2);
    assert_eq!(history.directory_usage(0, &dir).unwrap().count, 1);
}

#[test]
fn moving_between_directories_keeps_each_usage() {
    let a = PathBuf::from("/work/a");
    let b = PathBuf::from("/work/b");
    let history = History::from_entries_in_dirs([
        ("cargo build".to_string(), Some(a.clone())),
        ("cargo build".to_string(), Some(b.clone())),
        ("cargo build".to_string(), Some(a.clone())),
    ]);
    assert_eq!(history.frequency(0), 3);
    assert_eq!(history.directory_usage(0, &a).unwrap().count, 2);
    assert_eq!(history.directory_usage(0, &b).unwrap().count, 1);
}

#[test]
fn session_recall_skips_replaced_occurrences() {
    let history = History::from_entries(vec![
        "echo a".to_string(),
        "echo b".to_string(),
        "echo a".to_string(),
        "echo c".to_string(),
    ]);
    assert_eq!(history.session_get(0), Some("echo c"));
    assert_eq!(history.session_get(1), Some("echo a"));
    assert_eq!(history.session_get(2), Some("echo b"));
    assert_eq!(history.session_get(3), None);
}

#[test]
fn session_compaction_keeps_recall_order() {
    let mut entries: Vec<String> = (0..200).map(|i| format!("echo {i}")).collect();
    entries.extend((0..300).map(|_| "echo repeated".to_string()));
    let history = History::from_entries(entries);
    assert_eq!(history.session_get(0), Some("echo repeated"));
    assert_eq!(history.session_get(1), Some("echo 199"));
    assert_eq!(history.session_get(2), Some("echo 198"));
    assert!(history.session.len() <= 2 * history.session_live.max(1) + 64);
}

#[test]
fn directories_share_one_path_across_commands() {
    let same = PathBuf::from("/work/shared");
    let history = History::from_entries_in_dirs([
        ("one".to_string(), Some(same.clone())),
        ("two".to_string(), Some(same.clone())),
        ("three".to_string(), Some(PathBuf::from("/work/other"))),
    ]);
    assert_eq!(history.directory_paths.len(), 2);
    assert_eq!(history.directories(0).collect::<Vec<_>>(), [same.as_path()]);
    assert_eq!(
        history.directories(2).collect::<Vec<_>>(),
        [Path::new("/work/other")]
    );
}

#[test]
fn subsequence() {
    let q: Vec<char> = "gco".chars().collect();
    let (positions, count) = subsequence_match(&q, "git checkout").unwrap();
    assert_eq!(count, 3);
    assert_eq!(&positions[..3], &[0, 4, 9]);
}

#[test]
fn subsequence_no_match() {
    let q: Vec<char> = "xyz".chars().collect();
    assert!(subsequence_match(&q, "hello").is_none());
}

#[test]
fn history_path_uses_non_utf8_home() {
    let raw = OsString::from_vec(vec![b'/', b't', b'm', b'p', b'/', 0xf0, 0x80, 0x80, b'h']);
    let path = history_path_for_home(Some(raw.as_os_str())).unwrap();
    assert_eq!(path, PathBuf::from(raw).join(".local/share/xshi/history"));
}

#[test]
fn history_path_requires_a_home() {
    assert!(history_path_for_home(None).is_err());
    assert!(history_path_for_home(Some(std::ffi::OsStr::new(""))).is_err());
}

#[test]
fn recency_breaks_ties_within_same_tier() {
    let entries: Vec<String> = (0..100).map(|i| format!("cargo test {i}")).collect();
    let h = History::from_entries(entries);
    let results = h.fuzzy_search("cargo");
    assert_eq!(results[0].entry_idx, 99);
}

#[test]
fn prefix_tier_beats_boundary_substring() {
    let h = History::from_entries(vec!["echo cargo".into(), "cargo build".into()]);
    let results = h.fuzzy_search("cargo");
    assert_eq!(h.get(results[0].entry_idx), "cargo build");
}

#[test]
fn boundary_substring_tier_beats_plain_substring() {
    let h = History::from_entries(vec!["foocargobar".into(), "echo cargo".into()]);
    let results = h.fuzzy_search("cargo");
    assert_eq!(h.get(results[0].entry_idx), "echo cargo");
}

#[test]
fn substring_tier_beats_subsequence_fallback() {
    let h = History::from_entries(vec![
        "git remote add origin https://github.com/joshuarli/smtp-server.git".into(),
        "ls target/debug/".into(),
    ]);
    let results = h.fuzzy_search("target");
    assert_eq!(h.get(results[0].entry_idx), "ls target/debug/");
}

#[test]
fn search_into_sorts_before_limit() {
    let mut entries = vec!["cargo build".to_string()];
    entries.extend((0..220).map(|i| format!("c-x-{i}-a-x-r-x-g-x-o")));
    let h = History::from_entries(entries);
    let mut results = Vec::new();
    let mut candidates = Vec::new();
    h.search_entry_indices_into(&mut candidates);
    h.fuzzy_search_subset_into_in_dir(
        "cargo",
        &candidates,
        &mut Vec::new(),
        &mut results,
        200,
        Path::new("xshi"),
    );
    assert_eq!(h.get(results[0].entry_idx), "cargo build");
    assert!(results.iter().any(|m| h.get(m.entry_idx) == "cargo build"));
}

#[test]
fn match_quality_precedes_exact_directory_and_frequency() {
    let mut history = History::from_entries(vec!["cargo build".into()]);
    for _ in 0..50 {
        use_command(
            &mut history,
            "echo cargo",
            now_millis(),
            Some(Path::new("/work")),
        );
    }
    let results = history.fuzzy_search_in_dir("cargo", Path::new("/work"));
    assert_eq!(history.get(results[0].entry_idx), "cargo build");
    assert_eq!(results[0].score, 3);
}

#[test]
fn directory_proximity_precedes_scoped_frequency() {
    let mut history = History::from_entries(Vec::new());
    let now = now_millis();
    use_command(
        &mut history,
        "cargo exact",
        now,
        Some(Path::new("/work/project/src")),
    );
    use_command(
        &mut history,
        "cargo ancestor",
        now,
        Some(Path::new("/work/project")),
    );
    use_command(&mut history, "cargo distant", now, Some(Path::new("/work")));
    for _ in 0..500 {
        use_command(
            &mut history,
            "cargo unrelated",
            now,
            Some(Path::new("/work/projects")),
        );
    }
    let results = history.fuzzy_search_in_dir("cargo", Path::new("/work/project/src"));
    assert_eq!(
        results
            .iter()
            .map(|entry| history.get(entry.entry_idx))
            .collect::<Vec<_>>(),
        [
            "cargo exact",
            "cargo ancestor",
            "cargo distant",
            "cargo unrelated"
        ]
    );
}

#[test]
fn recent_usage_beats_stale_popularity_but_frequency_matters_when_recent() {
    let mut history = History::from_entries(Vec::new());
    let now = now_millis();
    for _ in 0..1024 {
        use_command(&mut history, "cargo stale", now - 90 * 24 * 3_600_000, None);
    }
    for _ in 0..8 {
        use_command(&mut history, "cargo frequent", now - 1000, None);
    }
    use_command(&mut history, "cargo fresh", now, None);
    let matches = history.fuzzy_search("cargo");
    assert_eq!(
        matches
            .iter()
            .map(|entry| history.get(entry.entry_idx))
            .collect::<Vec<_>>(),
        ["cargo frequent", "cargo fresh", "cargo stale"]
    );
    let empty = history.fuzzy_search("");
    assert_eq!(history.get(empty[0].entry_idx), "cargo fresh");
}

#[test]
fn subset_preserves_every_match_and_matches_full_top_k_on_refinement() {
    let mut history = History::from_entries(Vec::new());
    for _ in 0..16 {
        use_command(
            &mut history,
            "cargo older special",
            now_millis() - 1000,
            None,
        );
    }
    for i in 0..350 {
        use_command(
            &mut history,
            &format!("cargo newest {i}"),
            now_millis(),
            None,
        );
    }
    let mut candidates = Vec::new();
    history.search_entry_indices_into(&mut candidates);
    let mut matches = Vec::new();
    let mut results = Vec::new();
    let cwd = Path::new("/");
    history.fuzzy_search_subset_into_in_dir(
        "cargo",
        &candidates,
        &mut matches,
        &mut results,
        200,
        cwd,
    );
    assert_eq!(matches.len(), 351);
    let full = history.fuzzy_search("cargo");
    assert_eq!(
        results
            .iter()
            .map(|entry| entry.entry_idx)
            .collect::<Vec<_>>(),
        full[..200]
            .iter()
            .map(|entry| entry.entry_idx)
            .collect::<Vec<_>>()
    );
    assert_eq!(history.get(results[0].entry_idx), "cargo older special");
    let mut refined = Vec::new();
    history.fuzzy_search_subset_into_in_dir(
        "cargo older",
        &matches,
        &mut refined,
        &mut results,
        200,
        cwd,
    );
    assert_eq!(refined.len(), 1);
    assert_eq!(history.get(results[0].entry_idx), "cargo older special");
}

#[test]
fn unicode_scalar_lowercasing_and_long_subsequence_queries_keep_positions() {
    let history = History::from_entries(vec!["\u{130}stanbul".into(), "\u{e9} cafe".into()]);
    assert_eq!(
        history.get(history.fuzzy_search("\u{130}")[0].entry_idx),
        "\u{130}stanbul"
    );
    assert_eq!(
        history.get(history.fuzzy_search("i")[0].entry_idx),
        "\u{130}stanbul"
    );
    assert_eq!(history.fuzzy_search("ca")[0].match_positions[..2], [2, 3]);
    let query = "a".repeat(40);
    let text = "a-".repeat(40);
    let matches = History::from_entries(vec![text]).fuzzy_search(&query);
    assert_eq!(matches[0].match_count, 32);
    assert_eq!(matches[0].match_positions[31], 62);
}

#[test]
fn repeated_commands_keep_one_text_entry_at_their_first_position() {
    let history = History::from_entries(vec![
        "ls".into(),
        "cd /tmp".into(),
        "ls".into(),
        "cd /tmp".into(),
    ]);
    assert_eq!(history.len(), 2);
    assert_eq!(history.get(0), "ls");
    assert_eq!(history.get(1), "cd /tmp");
    assert_eq!(history.session_get(0), Some("cd /tmp"));
    assert_eq!(history.session_get(1), Some("ls"));
}

#[test]
fn command_listing_orders_by_last_use() {
    let history = History::from_entries(vec!["first".into(), "second".into(), "first".into()]);
    let mut indices = Vec::new();
    history.command_indices_into(&mut indices);
    assert_eq!(
        indices
            .iter()
            .map(|&idx| history.get(idx))
            .collect::<Vec<_>>(),
        ["second", "first"]
    );
}
