use super::store::{
    CACHE_MAGIC, HistoryLock, Record, decode_cache, encode_cache, lock_path_for, parse_line,
};
use super::*;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Another test thread forking a child briefly shares every open lock file
/// description with it, so even an uncontended lock may take a moment.
const LOCK_WAIT: Duration = Duration::from_secs(5);

/// A private directory holding one history, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "xshi-history-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn log(&self) -> PathBuf {
        self.0.join("history")
    }

    fn cache(&self) -> PathBuf {
        cache_path_for(&self.log())
    }

    fn load(&self) -> History {
        History::load_from(self.log())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Every entry, oldest first.
fn entries(hist: &History) -> Vec<String> {
    (0..hist.len())
        .map(|idx| hist.get(idx).to_owned())
        .collect()
}

/// Entries this session can recall with Up-arrow, newest first.
fn recallable(hist: &History) -> Vec<String> {
    (0..)
        .map_while(|skip| hist.session_get(skip))
        .map(str::to_owned)
        .collect()
}

fn line(timestamp: u64, session: u64, command: &str) -> String {
    format_record(timestamp, session, None, command)
}

fn append_raw(path: &Path, bytes: &[u8]) {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    file.write_all(bytes).unwrap();
}

/// A timestamp that lies after any session cutoff taken during a test, so that
/// a record carrying it is written by a "later" shell.
fn later(offset: u64) -> u64 {
    now_millis() + 60_000 + offset
}

/// Lets the millisecond clock move past a session cutoff or a prior write.
fn tick() {
    std::thread::sleep(Duration::from_millis(3));
}

/// The parallel vectors and the hash index agree with each other.
fn assert_consistent(hist: &History) {
    let count = hist.offsets.len();
    assert_eq!(hist.timestamps.len(), count);
    assert_eq!(hist.session_ids.len(), count);
    assert_eq!(hist.cwds.len(), count);
    assert_eq!(hist.local.len(), count);
    let mut seen = FxHashSet::default();
    for idx in 0..count {
        let text = hist.get(idx);
        assert!(seen.insert(text.to_owned()), "duplicate entry {text:?}");
        assert_eq!(hist.find_entry_index(hash_str(text), text), Some(idx));
    }
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
    let path = history_path_for_home(Some(raw.as_os_str()));
    assert_eq!(path, PathBuf::from(raw).join(".local/share/xshi/history"));
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
    h.fuzzy_search_into("cargo", &mut results, 200, "xshi");
    assert_eq!(h.get(results[0].entry_idx), "cargo build");
    assert!(results.iter().any(|m| h.get(m.entry_idx) == "cargo build"));
}

#[test]
fn cwd_weight_prefers_ancestor_entries() {
    let mut hist = History::from_entries(vec!["echo cargo".into(), "cargo build".into()]);
    hist.cwds[0] = Some(PathBuf::from("/work/project"));
    hist.cwds[1] = Some(PathBuf::from("/other"));

    let results = hist.fuzzy_search_in_dir("cargo", Path::new("/work/project/src"));
    assert_eq!(hist.get(results[0].entry_idx), "echo cargo");
    assert_eq!(results[0].score, 6);

    let results = hist.fuzzy_search_in_dir("cargo", Path::new("/work/projects"));
    assert_eq!(hist.get(results[0].entry_idx), "cargo build");
}

#[test]
fn parallel_vecs_sync_after_add() {
    let mut h = History::from_entries(vec!["aaa".into(), "bbb".into(), "ccc".into()]);
    assert_consistent(&h);

    // A repeated command replaces its earlier occurrence.
    h.add("bbb");
    assert_eq!(entries(&h), ["aaa", "ccc", "bbb"]);
    assert_consistent(&h);

    h.add("ddd");
    assert_eq!(entries(&h), ["aaa", "ccc", "bbb", "ddd"]);
    assert_consistent(&h);
}

#[test]
fn timestamps_are_set() {
    let mut h = History::from_entries(vec!["old".into()]);
    let before = now_millis();
    h.add("new_cmd");
    let after = now_millis();
    let ts = h.timestamp(h.len() - 1);
    assert!(ts >= before && ts <= after);
}

#[test]
fn from_entries_keeps_the_last_of_repeated_commands() {
    let h = History::from_entries(vec!["a".into(), "b".into(), "a".into()]);
    assert_eq!(entries(&h), ["b", "a"]);
    assert_consistent(&h);
}

#[test]
fn session_ids_are_unique_within_a_process() {
    let ids: FxHashSet<u64> = (0..1000).map(|_| new_session_id()).collect();
    assert_eq!(ids.len(), 1000);
    assert!(!ids.contains(&0));
}

#[test]
fn colliding_hashes_still_deduplicate() {
    let mut h = History::from_entries(vec!["one".into(), "two".into(), "three".into()]);
    // Simulate two commands sharing a hash: the index can only name one.
    h.index_by_hash.clear();
    h.index_by_hash.insert(hash_str("one"), 0);
    h.has_collisions = true;

    h.add("three");
    h.add("two");
    assert_eq!(entries(&h), ["one", "three", "two"]);
    assert_consistent(&h);
}

#[test]
fn removing_an_entry_keeps_the_index_consistent() {
    let mut h = History::from_entries((0..50).map(|n| format!("cmd {n}")).collect());
    for n in [0, 49, 25, 25, 1, 48] {
        h.add(&format!("cmd {n}"));
        assert_consistent(&h);
    }
    assert_eq!(h.get(h.len() - 1), "cmd 48");
    assert_eq!(h.len(), 50);
}

// Record format

#[test]
fn record_round_trips_metadata_with_awkward_cwd() {
    let cwd = Path::new("/work/tab\there\\back\nline");
    let text = format_record(1234, 56, Some(cwd), "echo\tinside");
    assert_eq!(
        parse_line(&text, 9),
        Some(Record {
            command: "echo\tinside".into(),
            timestamp: 1234,
            session_id: 56,
            cwd: Some(cwd.to_path_buf()),
        })
    );
    let legacy = format_record(1234, 56, None, "echo plain");
    let parsed = parse_line(&legacy, 9).unwrap();
    assert_eq!(
        (parsed.timestamp, parsed.session_id, parsed.cwd),
        (1234, 56, None)
    );
}

#[test]
fn plain_lines_are_legacy_commands_with_unknown_metadata() {
    let parsed = parse_line("echo old style", 777).unwrap();
    assert_eq!(parsed.command, "echo old style");
    assert_eq!(
        (parsed.timestamp, parsed.session_id, parsed.cwd),
        (777, 0, None)
    );
}

#[test]
fn torn_and_foreign_records_are_not_taken_for_commands() {
    for torn in [
        ":ish-history:v2\t123\t4",
        ":ish-history:v2\t123\t4\t/cwd",
        ":ish-history:v2\tnot-a-time\t4\t/cwd\tls",
        ":ish-history:v1\t123",
        ":ish-history:v1\t123\tx\tls",
        ":ish-history:v9\t1\t2\tls",
        ":ish-history:v2\t123\t4\t/cwd\t",
    ] {
        assert_eq!(parse_line(torn, 1), None, "{torn:?}");
    }
}

#[test]
fn commands_that_cannot_be_stored_are_not_recorded() {
    let scratch = Scratch::new("unstorable");
    let mut h = scratch.load();
    h.add(&"x".repeat(MAX_ENTRY_LEN_FOR_TESTS + 1));
    // 65_535 bytes of two-byte characters ends mid-character at the limit.
    h.add(&"é".repeat(40_000));
    h.add("has\0nul");
    h.add("   ");
    h.add("fine");
    assert_eq!(entries(&h), ["fine"]);

    let log = fs::read_to_string(scratch.log()).unwrap();
    assert_eq!(log.lines().count(), 1, "{log}");
    assert!(parse_line(&"y".repeat(MAX_ENTRY_LEN_FOR_TESTS + 1), 1).is_none());
    assert!(parse_line("ok\0no", 1).is_none());
}

const MAX_ENTRY_LEN_FOR_TESTS: usize = u16::MAX as usize;

#[test]
fn commands_at_the_length_limit_survive_the_cache() {
    let scratch = Scratch::new("limit");
    let long = "l".repeat(MAX_ENTRY_LEN_FOR_TESTS);
    let mut h = scratch.load();
    h.add(&long);
    h.add("short");
    h.compact();
    assert_eq!(entries(&scratch.load()), [long.as_str(), "short"]);
}

#[test]
fn newlines_in_a_command_are_collapsed_into_one_record() {
    let scratch = Scratch::new("newline");
    let mut h = scratch.load();
    h.add("echo a\necho b");
    assert_eq!(entries(&h), ["echo a echo b"]);
    assert_eq!(
        fs::read_to_string(scratch.log()).unwrap().lines().count(),
        1
    );
}

// Cache codec

#[test]
fn cache_layout_is_the_ish_v5_format() {
    let bytes = [
        b"ISH\x05".as_slice(),
        &2u32.to_le_bytes(),
        &b"ls\0pwd\0".len().to_le_bytes()[..4],
        &b"/a\0\0".len().to_le_bytes()[..4],
        &(5u64.wrapping_sub(883_612_800_000)).to_le_bytes(),
        &(9u64.wrapping_sub(883_612_800_000)).to_le_bytes(),
        b"ls\0pwd\0",
        b"/a\0\0",
    ]
    .concat();
    let records = decode_cache(&bytes).expect("a hand-built v5 cache decodes");
    assert_eq!(records.len(), 2);
    assert_eq!(
        (
            records[0].command.as_str(),
            records[0].timestamp,
            records[0].cwd.as_deref()
        ),
        ("ls", 5, Some(Path::new("/a")))
    );
    assert_eq!(
        (
            records[1].command.as_str(),
            records[1].timestamp,
            records[1].cwd.as_deref()
        ),
        ("pwd", 9, None)
    );
    assert_eq!(encode_cache(&records), bytes);
}

#[test]
fn cache_with_any_structural_damage_is_rejected() {
    let good = encode_cache(&[
        Record {
            command: "alpha".into(),
            timestamp: 10,
            session_id: 0,
            cwd: Some("/w".into()),
        },
        Record {
            command: "beta".into(),
            timestamp: 20,
            session_id: 0,
            cwd: None,
        },
    ]);
    assert!(decode_cache(&good).is_some());
    assert!(decode_cache(&[]).is_none());
    for cut in [1, 3, 15, 16, 20, good.len() - 1] {
        assert!(decode_cache(&good[..cut]).is_none(), "truncated to {cut}");
    }

    let mut extra = good.clone();
    extra.push(0);
    assert!(decode_cache(&extra).is_none(), "trailing byte");

    let mut wrong_magic = good.clone();
    wrong_magic[3] = 4;
    assert!(
        decode_cache(&wrong_magic).is_none(),
        "older cache generation"
    );
    assert_eq!(&good[..4], CACHE_MAGIC);

    let mut too_many = good.clone();
    too_many[4] = 3;
    assert!(decode_cache(&too_many).is_none(), "entry count too high");

    let mut too_few = good.clone();
    too_few[4] = 1;
    assert!(decode_cache(&too_few).is_none(), "entry count too low");

    let mut huge_count = good.clone();
    huge_count[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(decode_cache(&huge_count).is_none(), "overflowing sizes");

    let mut bad_utf8 = good.clone();
    let alpha = good.windows(5).position(|w| w == b"alpha").unwrap();
    bad_utf8[alpha] = 0xff;
    assert!(decode_cache(&bad_utf8).is_none(), "invalid UTF-8");

    let mut empty_entry = good;
    let alpha = empty_entry.windows(5).position(|w| w == b"alpha").unwrap();
    empty_entry[alpha..alpha + 5].copy_from_slice(b"\0\0\0\0\0");
    assert!(decode_cache(&empty_entry).is_none(), "empty entries");
}

#[test]
fn cache_round_trips_metadata_and_pre_1998_timestamps() {
    let scratch = Scratch::new("codec");
    let mut text = String::new();
    text += &format_record(1_000, 4, Some(Path::new("/w/one")), "first");
    text.push('\n');
    text += &format_record(2_000_000_000_000, 5, None, "second");
    text.push('\n');
    fs::write(scratch.log(), text).unwrap();

    let mut h = scratch.load();
    h.compact();
    assert_eq!(fs::metadata(scratch.log()).unwrap().len(), 0);

    let reloaded = scratch.load();
    assert_eq!(entries(&reloaded), ["first", "second"]);
    assert_eq!(reloaded.timestamp(0), 1_000);
    assert_eq!(reloaded.timestamp(1), 2_000_000_000_000);
    assert_eq!(reloaded.cwds[0].as_deref(), Some(Path::new("/w/one")));
    assert_eq!(reloaded.cwds[1], None);
}

// Loading

#[test]
fn loading_reads_the_log_without_writing_anything() {
    let scratch = Scratch::new("load");
    let text = format!("{}\n{}\n", line(10, 1, "one"), line(20, 1, "two"));
    fs::write(scratch.log(), &text).unwrap();

    let h = scratch.load();
    assert_eq!(entries(&h), ["one", "two"]);
    assert_eq!(fs::read_to_string(scratch.log()).unwrap(), text);
    assert!(!scratch.cache().exists());
}

#[test]
fn loading_deduplicates_keeping_the_latest_use() {
    let scratch = Scratch::new("dedup");
    let text = format!(
        "{}\n{}\n{}\nlegacy line\n",
        line(111, 7, "echo one"),
        line(222, 8, "echo two"),
        line(333, 9, "echo one"),
    );
    fs::write(scratch.log(), text).unwrap();

    let h = scratch.load();
    assert_eq!(entries(&h), ["echo two", "echo one", "legacy line"]);
    assert_eq!((h.timestamp(0), h.session_ids[0]), (222, 8));
    assert_eq!((h.timestamp(1), h.session_ids[1]), (333, 9));
    assert_eq!(h.session_ids[2], 0);
    assert_consistent(&h);
}

#[test]
fn repeated_legacy_lines_keep_the_last_position() {
    let scratch = Scratch::new("legacy-repeat");
    fs::write(scratch.log(), "a\nb\na\n").unwrap();
    assert_eq!(entries(&scratch.load()), ["b", "a"]);
}

#[test]
fn legacy_history_has_unknown_cwd() {
    let scratch = Scratch::new("legacy");
    fs::write(scratch.log(), format!("{}\n", line(111, 7, "echo legacy"))).unwrap();
    let hist = scratch.load();
    assert_eq!(hist.get(0), "echo legacy");
    assert_eq!(hist.cwds[0], None);
}

#[test]
fn loading_recovers_unterminated_and_undecodable_lines_around_a_valid_history() {
    let scratch = Scratch::new("ragged");
    let mut bytes = Vec::new();
    bytes.extend_from_slice(line(10, 1, "before").as_bytes());
    bytes.push(b'\n');
    bytes.extend_from_slice(b"bad \xff\xfe bytes\n");
    bytes.extend_from_slice(b"\n\n");
    bytes.extend_from_slice(line(20, 1, "after").as_bytes());
    bytes.push(b'\n');
    fs::write(scratch.log(), &bytes).unwrap();
    assert_eq!(entries(&scratch.load()), ["before", "after"]);
}

#[test]
fn cache_and_log_are_merged_on_load() {
    let scratch = Scratch::new("merge");
    let mut h = scratch.load();
    h.add("compacted");
    h.compact();
    drop(h);
    append_raw(
        &scratch.log(),
        format!("{}\n", line(later(0), 55, "in the log")).as_bytes(),
    );
    assert_eq!(entries(&scratch.load()), ["compacted", "in the log"]);
}

#[test]
fn unreadable_cache_falls_back_to_the_log_and_is_left_alone() {
    let scratch = Scratch::new("corrupt");
    let mut original = scratch.load();
    original.add("kept in cache");
    original.compact();
    drop(original);
    let mut damaged = fs::read(scratch.cache()).unwrap();
    damaged.truncate(damaged.len() - 3);
    fs::write(scratch.cache(), &damaged).unwrap();
    append_raw(
        &scratch.log(),
        format!("{}\n", line(10, 1, "in the log")).as_bytes(),
    );

    let mut h = scratch.load();
    assert_eq!(entries(&h), ["in the log"]);
    assert!(h.cache_dirty);

    h.add("new");
    h.compact();
    h.sync();
    assert_eq!(
        fs::read(scratch.cache()).unwrap(),
        damaged,
        "cache is not overwritten"
    );
    let log = fs::read_to_string(scratch.log()).unwrap();
    assert!(
        log.contains("in the log") && log.contains("new"),
        "log is not truncated: {log}"
    );
    assert!(!quarantine_path_for(&scratch.log()).exists());
}

#[test]
fn rebuild_sets_the_unreadable_cache_aside_and_writes_a_new_one() {
    let scratch = Scratch::new("rebuild");
    fs::write(scratch.cache(), b"ISH\x05 this is not a cache").unwrap();
    append_raw(
        &scratch.log(),
        format!("{}\n", line(10, 1, "survivor")).as_bytes(),
    );

    let mut h = scratch.load();
    assert!(h.cache_dirty);
    h.rebuild();
    assert!(!h.cache_dirty);
    assert_eq!(
        fs::read(quarantine_path_for(&scratch.log())).unwrap(),
        b"ISH\x05 this is not a cache"
    );
    assert_eq!(fs::metadata(scratch.log()).unwrap().len(), 0);

    let after = scratch.load();
    assert!(!after.cache_dirty);
    assert_eq!(entries(&after), ["survivor"]);
}

#[test]
fn rebuild_with_a_healthy_cache_keeps_every_entry() {
    let scratch = Scratch::new("rebuild-ok");
    let mut h = scratch.load();
    h.add("one");
    h.compact();
    h.add("two");
    h.rebuild();
    assert!(!quarantine_path_for(&scratch.log()).exists());
    assert_eq!(entries(&scratch.load()), ["one", "two"]);
}

#[test]
fn a_stale_log_left_by_a_crashed_compaction_does_not_reorder_history() {
    let scratch = Scratch::new("crash");
    let mut h = scratch.load();
    for command in ["a", "b", "c"] {
        h.add(command);
        tick();
    }
    let stale_log = fs::read(scratch.log()).unwrap();
    h.compact();
    // The process died after renaming the cache but before truncating the log.
    fs::write(scratch.log(), stale_log).unwrap();

    assert_eq!(entries(&scratch.load()), ["a", "b", "c"]);
    let mut second = scratch.load();
    second.compact();
    assert_eq!(entries(&scratch.load()), ["a", "b", "c"]);
}

#[test]
fn a_log_line_torn_by_a_crash_does_not_swallow_the_next_record() {
    let scratch = Scratch::new("torn");
    append_raw(
        &scratch.log(),
        format!("{}\n", line(10, 1, "whole")).as_bytes(),
    );
    append_raw(&scratch.log(), b":ish-history:v2\t20\t2\t/tm");

    let mut h = scratch.load();
    assert_eq!(entries(&h), ["whole"]);
    h.add("next");

    let log = fs::read_to_string(scratch.log()).unwrap();
    let lines: Vec<_> = log.lines().collect();
    assert_eq!(lines.len(), 3, "{log}");
    assert!(lines[2].ends_with("\tnext"), "{log}");
    assert_eq!(entries(&scratch.load()), ["whole", "next"]);
}

// Incremental sync

#[test]
fn sync_picks_up_records_appended_by_another_shell() {
    let scratch = Scratch::new("sync");
    let mut me = scratch.load();
    tick();
    let mut peer = scratch.load();
    peer.add("from peer");

    me.sync();
    assert_eq!(entries(&me), ["from peer"]);
    assert_eq!(
        recallable(&me),
        Vec::<String>::new(),
        "peer entries stay out of Up-arrow"
    );
    assert_eq!(me.prefix_search("from", 0), Some("from peer"));
}

#[test]
fn appending_does_not_skip_records_other_shells_wrote_in_between() {
    let scratch = Scratch::new("skip");
    let mut me = scratch.load();
    me.add("mine one");
    me.sync();
    tick();
    let mut peer = scratch.load();
    peer.add("peer wrote while I was typing");
    tick();
    me.add("mine two");

    me.sync();
    assert!(
        entries(&me).contains(&"peer wrote while I was typing".to_owned()),
        "{:?}",
        entries(&me)
    );
    assert_eq!(recallable(&me), ["mine two", "mine one"]);
}

#[test]
fn sync_does_not_reread_or_duplicate_its_own_records() {
    let scratch = Scratch::new("own");
    let mut me = scratch.load();
    me.add("first");
    me.add("second");
    me.sync();
    me.sync();
    assert_eq!(entries(&me), ["first", "second"]);
    assert_consistent(&me);
}

#[test]
fn sync_waits_for_a_record_that_is_still_being_written() {
    let scratch = Scratch::new("partial");
    let mut me = scratch.load();
    let record = line(later(0), 4242, "half written");
    let (head, tail) = record.split_at(record.len() - 5);

    append_raw(&scratch.log(), head.as_bytes());
    me.sync();
    assert!(me.is_empty(), "an unterminated record is not consumed");

    append_raw(&scratch.log(), format!("{tail}\n").as_bytes());
    me.sync();
    assert_eq!(entries(&me), ["half written"]);
}

#[test]
fn sync_skips_undecodable_lines_and_keeps_making_progress() {
    let scratch = Scratch::new("badbytes");
    let mut me = scratch.load();
    append_raw(&scratch.log(), b"garbage \xc3\x28 bytes\n");
    me.sync();
    append_raw(
        &scratch.log(),
        format!("{}\n", line(later(0), 4242, "after garbage")).as_bytes(),
    );
    me.sync();
    assert_eq!(entries(&me), ["after garbage"]);
}

#[test]
fn a_peers_newer_use_moves_a_hidden_entry_but_not_a_recallable_one() {
    let scratch = Scratch::new("dup");
    fs::write(
        scratch.log(),
        format!("{}\n{}\n", line(10, 1, "recallable"), line(20, 1, "other")),
    )
    .unwrap();
    let mut me = scratch.load();
    tick();

    append_raw(
        &scratch.log(),
        format!(
            "{}\n{}\n{}\n",
            line(later(0), 4242, "hidden"),
            line(later(1), 4242, "recallable"),
            line(later(2), 4242, "hidden"),
        )
        .as_bytes(),
    );
    me.sync();
    assert_eq!(entries(&me), ["recallable", "other", "hidden"]);
    assert_eq!(recallable(&me), ["other", "recallable"]);
    assert_consistent(&me);
}

#[test]
fn a_peers_older_record_never_displaces_a_newer_entry() {
    let scratch = Scratch::new("older");
    let mut me = scratch.load();
    tick();
    append_raw(
        &scratch.log(),
        format!(
            "{}\n{}\n",
            line(later(100), 4242, "x"),
            line(later(200), 4242, "y")
        )
        .as_bytes(),
    );
    me.sync();
    append_raw(
        &scratch.log(),
        format!("{}\n", line(later(50), 4243, "x")).as_bytes(),
    );
    me.sync();
    assert_eq!(entries(&me), ["x", "y"]);
}

#[test]
fn sync_survives_another_shell_compacting() {
    let scratch = Scratch::new("compacted");
    let mut me = scratch.load();
    tick();
    let mut peer = scratch.load();
    peer.add("peer one");
    me.sync();
    peer.add("peer two");
    peer.compact();
    // The log is now empty and the cache holds both.
    me.sync();
    assert_eq!(entries(&me), ["peer one", "peer two"]);

    // The log regrows past the position this shell last read.
    for n in 0..20 {
        peer.add(&format!("after compaction {n}"));
    }
    me.sync();
    assert_eq!(me.len(), 22);
    assert!(entries(&me).contains(&"after compaction 19".to_owned()));
    assert_consistent(&me);
}

#[test]
fn sync_survives_the_log_regrowing_before_it_notices_a_compaction() {
    let scratch = Scratch::new("regrow");
    let mut me = scratch.load();
    tick();
    let mut peer = scratch.load();
    peer.add("short");
    me.sync();
    let seen = me.file_pos;

    peer.compact();
    for n in 0..30 {
        peer.add(&format!("regrown entry number {n}"));
    }
    assert!(fs::metadata(scratch.log()).unwrap().len() > seen);

    me.sync();
    assert_eq!(me.len(), 31);
    assert!(
        entries(&me)
            .iter()
            .all(|entry| !entry.contains("ish-history"))
    );
    assert_consistent(&me);
}

#[test]
fn sync_survives_the_log_being_replaced() {
    let scratch = Scratch::new("replaced");
    fs::write(scratch.log(), format!("{}\n", line(10, 1, "old"))).unwrap();
    let mut me = scratch.load();
    tick();

    let replacement = scratch.0.join("history.new");
    let mut body = String::new();
    for n in 0..5 {
        body += &line(later(n), 4242, &format!("replacement {n}"));
        body.push('\n');
    }
    fs::write(&replacement, body).unwrap();
    fs::rename(&replacement, scratch.log()).unwrap();

    me.sync();
    assert_eq!(me.len(), 6);
    assert_eq!(entries(&me)[0], "old");
    assert_eq!(entries(&me)[5], "replacement 4");
}

#[test]
fn sync_survives_the_log_being_truncated_or_removed() {
    let scratch = Scratch::new("truncated");
    let mut me = scratch.load();
    me.add("kept in memory");
    fs::write(scratch.log(), b"").unwrap();
    me.sync();
    assert_eq!(entries(&me), ["kept in memory"]);

    tick();
    append_raw(
        &scratch.log(),
        format!("{}\n", line(later(0), 4242, "later")).as_bytes(),
    );
    me.sync();
    assert_eq!(entries(&me), ["kept in memory", "later"]);

    fs::remove_file(scratch.log()).unwrap();
    me.sync();
    assert_eq!(entries(&me), ["kept in memory", "later"]);
    append_raw(
        &scratch.log(),
        format!("{}\n", line(later(1), 4242, "recreated")).as_bytes(),
    );
    me.sync();
    assert_eq!(entries(&me), ["kept in memory", "later", "recreated"]);
}

// Compaction and locking

#[test]
fn compaction_keeps_entries_another_shell_compacted_first() {
    let scratch = Scratch::new("lostupdate");
    let mut a = scratch.load();
    let mut b = scratch.load();
    a.add("from a");
    a.compact();
    b.add("from b");
    // b never synced, so its memory lacks "from a", which now lives only in the cache.
    b.compact();

    assert_eq!(entries(&scratch.load()), ["from a", "from b"]);
}

#[test]
fn compaction_keeps_entries_other_shells_have_not_been_read_yet() {
    let scratch = Scratch::new("unsynced");
    let mut a = scratch.load();
    let mut b = scratch.load();
    a.add("from a");
    b.add("from b");
    a.compact();
    b.compact();

    let fresh = scratch.load();
    assert_eq!(fresh.len(), 2);
    assert!(entries(&fresh).contains(&"from a".to_owned()));
    assert!(entries(&fresh).contains(&"from b".to_owned()));
    assert_eq!(fs::metadata(scratch.log()).unwrap().len(), 0);
}

#[test]
fn compaction_orders_the_cache_by_use() {
    let scratch = Scratch::new("order");
    let mut h = scratch.load();
    for command in ["a", "b", "c"] {
        h.add(command);
        tick();
    }
    h.add("a");
    h.compact();
    assert_eq!(entries(&scratch.load()), ["b", "c", "a"]);
}

#[test]
fn compaction_updates_this_shells_view_of_what_it_missed() {
    let scratch = Scratch::new("fold");
    let mut a = scratch.load();
    tick();
    let mut b = scratch.load();
    b.add("from b");
    a.compact();
    assert_eq!(entries(&a), ["from b"]);
    a.sync();
    assert_eq!(entries(&a), ["from b"]);
}

#[test]
fn compaction_yields_when_another_shell_holds_the_lock() {
    let scratch = Scratch::new("busy");
    let mut h = scratch.load();
    h.add("only in the log");
    let held = HistoryLock::exclusive(&scratch.log(), LOCK_WAIT).expect("lock is free");

    h.compact();
    assert!(!scratch.cache().exists());
    assert!(
        fs::read_to_string(scratch.log())
            .unwrap()
            .contains("only in the log")
    );

    drop(held);
    h.compact();
    assert!(scratch.cache().exists());
    assert_eq!(entries(&scratch.load()), ["only in the log"]);
}

#[test]
fn appending_waits_for_a_running_compaction() {
    let scratch = Scratch::new("wait");
    let mut h = scratch.load();
    let released = AtomicBool::new(false);
    let held = HistoryLock::exclusive(&scratch.log(), LOCK_WAIT).expect("lock is free");
    std::thread::scope(|s| {
        s.spawn(|| {
            std::thread::sleep(Duration::from_millis(100));
            released.store(true, Ordering::SeqCst);
            drop(held);
        });
        h.add("appended after the rewrite");
        assert!(
            released.load(Ordering::SeqCst),
            "the append ran under the exclusive lock"
        );
    });
    assert!(
        fs::read_to_string(scratch.log())
            .unwrap()
            .contains("appended after the rewrite")
    );
}

#[test]
fn a_stuck_lock_holder_cannot_hang_a_shell() {
    let scratch = Scratch::new("stuck");
    let mut h = scratch.load();
    let _held = HistoryLock::exclusive(&scratch.log(), LOCK_WAIT).expect("lock is free");
    let started = std::time::Instant::now();
    // Readers and appenders give up after SHARED_LOCK_WAIT and carry on.
    h.add("still recorded");
    h.sync();
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(
        fs::read_to_string(scratch.log())
            .unwrap()
            .contains("still recorded")
    );
}

#[test]
fn many_shells_together_lose_no_entries_and_tear_no_records() {
    let scratch = Scratch::new("stress");
    let log = scratch.log();
    const SHELLS: usize = 6;
    const PER_SHELL: usize = 40;
    std::thread::scope(|s| {
        for shell in 0..SHELLS {
            let log = log.clone();
            s.spawn(move || {
                let mut h = History::load_from(log);
                for n in 0..PER_SHELL {
                    h.add(&format!(
                        "shell {shell} command {n} {}",
                        "padding".repeat(20)
                    ));
                    h.add("shared command");
                    if n % 4 == 0 {
                        h.sync();
                    }
                    if n % 13 == 12 {
                        h.compact();
                    }
                }
                h.compact();
            });
        }
    });

    let mut fresh = scratch.load();
    assert_eq!(fresh.len(), SHELLS * PER_SHELL + 1);
    for shell in 0..SHELLS {
        for n in 0..PER_SHELL {
            let command = format!("shell {shell} command {n} {}", "padding".repeat(20));
            assert!(
                fresh.prefix_search(&command, 0).is_some(),
                "lost {command:?}"
            );
        }
    }
    assert_consistent(&fresh);

    let log = fs::read(scratch.log()).unwrap_or_default();
    for chunk in log.split(|&b| b == b'\n').filter(|chunk| !chunk.is_empty()) {
        let text = std::str::from_utf8(chunk).expect("records are whole");
        assert!(
            text.starts_with(":ish-history:v2\t") && parse_line(text, 0).is_some(),
            "{text:?}"
        );
    }

    fresh.compact();
    assert_eq!(scratch.load().len(), SHELLS * PER_SHELL + 1);
}

// Reset

#[test]
fn reset_removes_the_history_and_new_entries_start_over() {
    let scratch = Scratch::new("reset");
    let mut h = scratch.load();
    h.add("old");
    h.compact();
    h.add("older still, in the log");
    fs::write(quarantine_path_for(&scratch.log()), b"junk").unwrap();

    h.reset().unwrap();
    assert!(h.is_empty());
    assert!(!scratch.log().exists() && !scratch.cache().exists());
    assert!(!quarantine_path_for(&scratch.log()).exists());
    assert!(scratch.load().is_empty());

    h.add("fresh");
    assert_eq!(entries(&scratch.load()), ["fresh"]);
}

#[test]
fn every_reset_is_noticed_even_within_one_clock_tick() {
    let scratch = Scratch::new("resets");
    let mut me = scratch.load();
    let mut first = scratch.load();
    let mut second = scratch.load();

    first.reset().unwrap();
    me.sync();
    me.add("after the first reset");
    second.reset().unwrap();
    me.sync();
    assert!(me.is_empty(), "{:?}", entries(&me));
}

#[test]
fn compaction_does_not_resurrect_history_another_shell_reset() {
    let scratch = Scratch::new("resurrect");
    let mut stale = scratch.load();
    stale.add("must stay deleted");
    let mut resetter = scratch.load();
    resetter.reset().unwrap();

    stale.compact();
    assert!(scratch.load().is_empty());
    assert!(!scratch.cache().exists());
}

#[test]
fn entries_added_after_a_peers_reset_survive() {
    let scratch = Scratch::new("after-reset");
    let mut stale = scratch.load();
    stale.add("deleted");
    let mut resetter = scratch.load();
    resetter.reset().unwrap();

    stale.add("kept");
    stale.compact();
    assert_eq!(entries(&scratch.load()), ["kept"]);
}

// Detached and large histories

#[test]
fn a_detached_history_never_touches_the_disk() {
    let mut h = History::from_entries(vec!["a".into()]);
    h.add("b");
    h.sync();
    h.compact();
    h.rebuild();
    h.reset().unwrap();
    assert!(h.is_empty());
    assert!(h.is_detached());
}

#[test]
fn a_large_history_round_trips_through_the_cache() {
    let scratch = Scratch::new("large");
    const COUNT: u64 = 60_000;
    let mut text = String::new();
    for n in 0..COUNT {
        text += &format_record(
            1_000 + n,
            7,
            Some(Path::new("/work/project")),
            &format!("command number {n}"),
        );
        text.push('\n');
    }
    fs::write(scratch.log(), text).unwrap();

    let mut h = scratch.load();
    assert_eq!(h.len(), COUNT as usize);
    h.compact();
    assert_eq!(fs::metadata(scratch.log()).unwrap().len(), 0);

    let reloaded = scratch.load();
    assert_eq!(reloaded.len(), COUNT as usize);
    assert_eq!(reloaded.get(0), "command number 0");
    assert_eq!(reloaded.get(COUNT as usize - 1), "command number 59999");
    assert_eq!(reloaded.timestamp(12_345), 1_000 + 12_345);
    assert_eq!(
        reloaded.cwds[59_999].as_deref(),
        Some(Path::new("/work/project"))
    );
    assert_eq!(
        reloaded.prefix_search("command number 4", 0),
        Some("command number 49999")
    );
    assert_consistent(&reloaded);
}

#[test]
fn a_log_full_of_repeats_loads_in_linear_time() {
    let scratch = Scratch::new("repeats");
    let mut text = String::new();
    for n in 0..200_000u64 {
        text += &line(1_000 + n, 7, &format!("repeated command {}", n % 100));
        text.push('\n');
    }
    fs::write(scratch.log(), text).unwrap();

    let started = std::time::Instant::now();
    let mut h = scratch.load();
    assert_eq!(h.len(), 100);
    assert_eq!(h.get(99), "repeated command 99");
    h.compact();
    assert_eq!(scratch.load().len(), 100);
    // Quadratic handling of the repeats takes minutes; the bound only guards that.
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn the_lock_file_is_a_sibling_and_never_truncated() {
    let scratch = Scratch::new("lockfile");
    let mut h = scratch.load();
    h.add("x");
    let lock = lock_path_for(&scratch.log());
    assert!(lock.exists());
    assert_eq!(lock.parent(), scratch.log().parent());
    h.compact();
    assert!(lock.exists());
}
