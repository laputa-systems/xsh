mod support;

use std::fs;
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use support::specs::*;
use support::{budget, budget_with, tar_with};
use xsh_archive::{
    Archive, Budget, Codec, Collision, Compression, Encode, Encoder, ErrorKind, Format, Limits,
    Links, Modes, Policy, Source, Special, extract,
};

struct Sandbox {
    base: tempfile::TempDir,
}

impl Sandbox {
    fn new() -> Self {
        Self {
            base: tempfile::tempdir().unwrap(),
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.base.path().join(name)
    }

    fn outside(&self) -> PathBuf {
        let outside = self.path("outside");
        if !outside.exists() {
            fs::create_dir(&outside).unwrap();
            fs::write(outside.join("sentinel"), "outside").unwrap();
        }
        outside
    }

    fn assert_outside_untouched(&self) {
        let outside = self.outside();
        let names: Vec<_> = fs::read_dir(&outside).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names.len(), 1, "{names:?}");
        assert_eq!(fs::read_to_string(outside.join("sentinel")).unwrap(), "outside");
    }

    fn leftovers(&self, dir: &Path) -> Vec<String> {
        fs::read_dir(dir)
            .map(|entries| {
                entries
                    .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                    .filter(|name| name.starts_with(".xsh-archive"))
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn run(
    bytes: Vec<u8>,
    dest: &Path,
    policy: &Policy,
) -> Result<xsh_archive::Report, xsh_archive::Error> {
    run_with(bytes, dest, policy, &budget())
}

fn run_with(
    bytes: Vec<u8>,
    dest: &Path,
    policy: &Policy,
    budget: &Arc<Budget>,
) -> Result<xsh_archive::Report, xsh_archive::Error> {
    let mut archive = Archive::open(
        Source::from_bytes(bytes),
        Format::Tar,
        Compression::Plain,
        Arc::clone(budget),
    )?;
    extract(&mut archive, dest, policy, budget)
}

#[test]
fn extracts_into_a_new_destination_and_publishes_it_whole() {
    let sandbox = Sandbox::new();
    let dest = sandbox.path("fresh/nested/dest");
    let bytes = tar_of(&[
        dir(b"pkg"),
        file(b"pkg/a.txt", b"alpha"),
        with_mode(file(b"pkg/run.sh", b"#!/bin/sh\n"), 0o755),
        symlink(b"pkg/link", b"a.txt"),
        hardlink(b"pkg/hard", b"pkg/a.txt"),
    ]);
    let report = run(bytes, &dest, &Policy::default()).unwrap();
    assert_eq!(report.files, 2);
    assert_eq!(fs::read_to_string(dest.join("pkg/a.txt")).unwrap(), "alpha");
    assert_eq!(fs::metadata(dest.join("pkg/run.sh")).unwrap().permissions().mode() & 0o777, 0o755);
    assert_eq!(fs::read_link(dest.join("pkg/link")).unwrap(), Path::new("a.txt"));
    assert_eq!(fs::metadata(dest.join("pkg/a.txt")).unwrap().nlink(), 2);
    assert!(sandbox.leftovers(dest.parent().unwrap()).is_empty());
    assert!(sandbox.leftovers(&dest).is_empty());
}

#[test]
fn extracts_into_an_existing_directory_and_keeps_unrelated_files() {
    let sandbox = Sandbox::new();
    let dest = sandbox.path("dest");
    fs::create_dir_all(dest.join("pkg")).unwrap();
    fs::write(dest.join("keep.txt"), "keep").unwrap();
    fs::write(dest.join("pkg/old.txt"), "old").unwrap();
    fs::set_permissions(dest.join("pkg"), fs::Permissions::from_mode(0o750)).unwrap();
    let bytes = tar_of(&[dir(b"pkg"), file(b"pkg/new.txt", b"new"), file(b"top.txt", b"top")]);
    run(bytes, &dest, &Policy::default()).unwrap();
    assert_eq!(fs::read_to_string(dest.join("keep.txt")).unwrap(), "keep");
    assert_eq!(fs::read_to_string(dest.join("pkg/old.txt")).unwrap(), "old");
    assert_eq!(fs::read_to_string(dest.join("pkg/new.txt")).unwrap(), "new");
    assert_eq!(fs::metadata(dest.join("pkg")).unwrap().permissions().mode() & 0o777, 0o750);
    assert!(sandbox.leftovers(&dest).is_empty());
}

#[test]
fn unsafe_names_fail_without_touching_the_destination_or_outside() {
    let sandbox = Sandbox::new();
    let outside = sandbox.outside();
    for name in [
        &b"../escape"[..],
        b"a/../../escape",
        b"/etc/escape",
        b"ok/../../outside/pwn",
    ] {
        let dest = sandbox.path("dest");
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("keep"), "keep").unwrap();
        let bytes = tar_of(&[file(b"first", b"1"), file(name, b"x")]);
        let error = run(bytes, &dest, &Policy::default()).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Unsafe, "{:?}", String::from_utf8_lossy(name));
        let names: Vec<_> = fs::read_dir(&dest).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, vec![std::ffi::OsString::from("keep")], "{names:?}");
        fs::remove_dir_all(&dest).unwrap();
    }
    let _ = outside;
    sandbox.assert_outside_untouched();
}

#[test]
fn a_failed_extraction_into_a_new_destination_leaves_nothing_behind() {
    let sandbox = Sandbox::new();
    let dest = sandbox.path("a/b/c");
    let bytes = tar_of(&[file(b"fine", b"1"), file(b"../bad", b"x")]);
    assert_eq!(run(bytes, &dest, &Policy::default()).unwrap_err().kind(), ErrorKind::Unsafe);
    assert!(!sandbox.path("a").exists());
}

#[test]
fn a_member_cannot_be_written_through_a_link_the_archive_created() {
    let sandbox = Sandbox::new();
    let outside = sandbox.outside();
    let policy = Policy { links: Links::Verbatim, ..Policy::default() };
    let dest = sandbox.path("dest");
    let target = outside.as_os_str().as_bytes().to_vec();
    let bytes = tar_of(&[symlink(b"l", &target), file(b"l/pwn", b"x")]);
    let error = run(bytes, &dest, &policy).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Unsafe);
    assert!(error.message().contains("symbolic link"), "{error}");

    let bytes = tar_of(&[symlink(b"l", b"."), file(b"l/inner", b"x")]);
    assert_eq!(run(bytes, &dest, &policy).unwrap_err().kind(), ErrorKind::Unsafe);
    let bytes = tar_of(&[symlink(b"l", b"../outside"), dir(b"l")]);
    assert_eq!(run(bytes, &dest, &policy).unwrap_err().kind(), ErrorKind::Collision);
    sandbox.assert_outside_untouched();
}

#[test]
fn link_policy_decides_which_symbolic_links_are_created() {
    let sandbox = Sandbox::new();
    let inside = tar_of(&[dir(b"d"), symlink(b"d/up", b".."), symlink(b"d/self", b"../d/./x")]);
    let escaping = tar_of(&[dir(b"d"), symlink(b"d/out", b"../..")]);
    let absolute = tar_of(&[symlink(b"abs", b"/etc/passwd")]);

    let contained = Policy::default();
    run(inside.clone(), &sandbox.path("one"), &contained).unwrap();
    assert_eq!(run(escaping.clone(), &sandbox.path("two"), &contained).unwrap_err().kind(), ErrorKind::Unsafe);
    assert_eq!(run(absolute.clone(), &sandbox.path("three"), &contained).unwrap_err().kind(), ErrorKind::Unsafe);

    let verbatim = Policy { links: Links::Verbatim, ..Policy::default() };
    run(escaping, &sandbox.path("four"), &verbatim).unwrap();
    run(absolute.clone(), &sandbox.path("five"), &verbatim).unwrap();
    assert_eq!(fs::read_link(sandbox.path("five/abs")).unwrap(), Path::new("/etc/passwd"));

    let refuse = Policy { links: Links::Refuse, ..Policy::default() };
    assert_eq!(run(inside, &sandbox.path("six"), &refuse).unwrap_err().kind(), ErrorKind::Unsafe);
    let hard = tar_of(&[file(b"a", b"1"), hardlink(b"b", b"a")]);
    assert_eq!(run(hard, &sandbox.path("seven"), &refuse).unwrap_err().kind(), ErrorKind::Unsafe);
}

#[test]
fn hard_links_need_an_earlier_regular_file_of_the_same_extraction() {
    let sandbox = Sandbox::new();
    let policy = Policy::default();
    for (index, specs) in [
        tar_of(&[hardlink(b"b", b"missing")]),
        tar_of(&[symlink(b"s", b"x"), hardlink(b"b", b"s")]),
        tar_of(&[hardlink(b"b", b"/etc/passwd")]),
        tar_of(&[hardlink(b"b", b"../outside/sentinel")]),
        tar_of(&[dir(b"d"), hardlink(b"b", b"d")]),
    ]
    .into_iter()
    .enumerate()
    {
        let error = run(specs, &sandbox.path(&format!("dest-{index}")), &policy).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Unsafe, "case {index}: {error}");
    }
    sandbox.outside();
    sandbox.assert_outside_untouched();
}

#[test]
fn repeated_names_and_kind_clashes_follow_the_collision_policy() {
    let sandbox = Sandbox::new();
    let twice = tar_of(&[file(b"f", b"first"), file(b"f", b"second")]);
    let error = run(twice.clone(), &sandbox.path("refuse"), &Policy::default()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Collision);
    let replace = Policy { collision: Collision::Replace, ..Policy::default() };
    run(twice, &sandbox.path("replace"), &replace).unwrap();
    assert_eq!(fs::read_to_string(sandbox.path("replace/f")).unwrap(), "second");

    let file_then_dir = tar_of(&[file(b"x", b"1"), dir(b"x")]);
    assert_eq!(
        run(file_then_dir, &sandbox.path("clash-a"), &Policy::default()).unwrap_err().kind(),
        ErrorKind::Collision
    );
    let dir_then_file = tar_of(&[dir(b"x"), file(b"x", b"1")]);
    assert_eq!(
        run(dir_then_file, &sandbox.path("clash-b"), &replace).unwrap_err().kind(),
        ErrorKind::Collision
    );
    let through_file = tar_of(&[file(b"x", b"1"), file(b"x/y", b"2")]);
    assert_eq!(
        run(through_file, &sandbox.path("clash-c"), &replace).unwrap_err().kind(),
        ErrorKind::Collision
    );
}

#[test]
fn destination_collisions_publish_nothing_unless_replace_is_requested() {
    let sandbox = Sandbox::new();
    let dest = sandbox.path("dest");
    fs::create_dir_all(dest.join("sub")).unwrap();
    fs::write(dest.join("sub/existing"), "mine").unwrap();
    let bytes = tar_of(&[file(b"new", b"n"), file(b"sub/other", b"o"), file(b"sub/existing", b"theirs")]);
    let error = run(bytes.clone(), &dest, &Policy::default()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Collision);
    assert!(error.entry().is_some_and(|entry| entry.ends_with(b"existing")));
    assert!(!dest.join("new").exists());
    assert!(!dest.join("sub/other").exists());
    assert_eq!(fs::read_to_string(dest.join("sub/existing")).unwrap(), "mine");
    assert!(sandbox.leftovers(&dest).is_empty());

    let replace = Policy { collision: Collision::Replace, ..Policy::default() };
    run(bytes, &dest, &replace).unwrap();
    assert_eq!(fs::read_to_string(dest.join("sub/existing")).unwrap(), "theirs");
    assert_eq!(fs::read_to_string(dest.join("new")).unwrap(), "n");

    fs::create_dir(dest.join("adir")).unwrap();
    fs::write(dest.join("adir/precious"), "p").unwrap();
    let file_over_dir = tar_of(&[file(b"adir", b"file")]);
    assert_eq!(run(file_over_dir, &dest, &replace).unwrap_err().kind(), ErrorKind::Collision);
    assert!(dest.join("adir/precious").exists());
}

#[test]
fn special_permission_bits_are_stripped_unless_exact_modes_are_requested() {
    let sandbox = Sandbox::new();
    let bytes = tar_of(&[
        with_mode(file(b"suid", b"x"), 0o4755),
        with_mode(file(b"sticky", b"x"), 0o1777),
        with_mode(dir(b"sgid"), 0o2775),
    ]);
    run(bytes.clone(), &sandbox.path("plain"), &Policy::default()).unwrap();
    assert_eq!(fs::metadata(sandbox.path("plain/suid")).unwrap().mode() & 0o7777, 0o755);
    assert_eq!(fs::metadata(sandbox.path("plain/sticky")).unwrap().mode() & 0o7777, 0o777);
    assert_eq!(fs::metadata(sandbox.path("plain/sgid")).unwrap().mode() & 0o7777, 0o775);

    let exact = Policy { modes: Modes::Exact, ..Policy::default() };
    run(bytes, &sandbox.path("exact"), &exact).unwrap();
    assert_eq!(fs::metadata(sandbox.path("exact/suid")).unwrap().mode() & 0o7777, 0o4755);
    assert_eq!(fs::metadata(sandbox.path("exact/sticky")).unwrap().mode() & 0o7777, 0o1777);
    assert_eq!(fs::metadata(sandbox.path("exact/sgid")).unwrap().mode() & 0o7777, 0o2775);
}

#[test]
fn read_only_directories_do_not_block_their_children() {
    let sandbox = Sandbox::new();
    let bytes = tar_of(&[
        with_mode(dir(b"ro"), 0o555),
        file(b"ro/child", b"c"),
        with_mode(dir(b"ro/deeper"), 0o500),
        file(b"ro/deeper/leaf", b"l"),
    ]);
    let dest = sandbox.path("dest");
    run(bytes.clone(), &dest, &Policy::default()).unwrap();
    assert_eq!(fs::metadata(dest.join("ro")).unwrap().mode() & 0o777, 0o555);
    assert_eq!(fs::read_to_string(dest.join("ro/deeper/leaf")).unwrap(), "l");
    fs::set_permissions(dest.join("ro/deeper"), fs::Permissions::from_mode(0o700)).unwrap();
    fs::set_permissions(dest.join("ro"), fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn special_files_are_refused_by_default_and_created_on_request() {
    let sandbox = Sandbox::new();
    let bytes = tar_of(&[fifo(b"pipe")]);
    assert_eq!(
        run(bytes.clone(), &sandbox.path("refused"), &Policy::default()).unwrap_err().kind(),
        ErrorKind::Unsafe
    );
    let create = Policy { special: Special::Create, ..Policy::default() };
    run(bytes, &sandbox.path("created"), &create).unwrap();
    assert!(fs::symlink_metadata(sandbox.path("created/pipe")).unwrap().file_type().is_fifo());
}

#[test]
fn unsupported_entry_types_are_reported_not_skipped() {
    let sandbox = Sandbox::new();
    let sparse = tar_of(&[Spec { name: b"s", flag: b'S', body: b"", link: b"", mode: 0o644 }]);
    assert_eq!(
        run(sparse, &sandbox.path("dest"), &Policy::default()).unwrap_err().kind(),
        ErrorKind::Unsupported
    );
}

#[test]
fn strip_components_and_member_filters_apply_to_names_and_hard_link_targets() {
    let sandbox = Sandbox::new();
    let bytes = tar_of(&[
        dir(b"top"),
        dir(b"top/dir"),
        file(b"top/dir/a", b"a"),
        file(b"top/dir/b", b"b"),
        hardlink(b"top/dir/c", b"top/dir/a"),
        file(b"top/other", b"o"),
    ]);
    let stripped = Policy { strip_components: 1, ..Policy::default() };
    run(bytes.clone(), &sandbox.path("stripped"), &stripped).unwrap();
    assert!(sandbox.path("stripped/dir/a").exists());
    assert!(!sandbox.path("stripped/top").exists());
    assert_eq!(fs::metadata(sandbox.path("stripped/dir/a")).unwrap().nlink(), 2);

    let picked = Policy { members: vec![b"top/dir".to_vec()], strip_components: 2, ..Policy::default() };
    run(bytes.clone(), &sandbox.path("picked"), &picked).unwrap();
    let mut names: Vec<_> = fs::read_dir(sandbox.path("picked")).unwrap().map(|e| e.unwrap().file_name()).collect();
    names.sort();
    assert_eq!(names, vec!["a", "b", "c"]);

    let everything = Policy { strip_components: 3, ..Policy::default() };
    let report = run(bytes, &sandbox.path("nothing"), &everything).unwrap();
    assert_eq!(report.entries, 0);

    let outside_target = tar_of(&[
        dir(b"top"),
        file(b"top/other", b"o"),
        dir(b"top/dir"),
        hardlink(b"top/dir/c", b"top/other"),
    ]);
    let cut = Policy { strip_components: 2, ..Policy::default() };
    assert_eq!(
        run(outside_target, &sandbox.path("cut"), &cut).unwrap_err().kind(),
        ErrorKind::Unsafe
    );
}

#[test]
fn non_utf8_names_survive_extraction_byte_for_byte() {
    let sandbox = Sandbox::new();
    let bytes = tar_with(&[(b"caf\xe9/na\xefve", b"data")]);
    run(bytes, &sandbox.path("dest"), &Policy::default()).unwrap();
    let inner = fs::read_dir(sandbox.path("dest")).unwrap().next().unwrap().unwrap().path();
    assert_eq!(inner.file_name().unwrap().as_bytes(), b"caf\xe9");
    let leaf = fs::read_dir(&inner).unwrap().next().unwrap().unwrap().file_name();
    assert_eq!(leaf.as_bytes(), b"na\xefve");
}

#[test]
fn truncation_and_budget_failures_leave_the_destination_as_it_was() {
    let sandbox = Sandbox::new();
    let dest = sandbox.path("dest");
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("keep"), "keep").unwrap();
    let mut bytes = tar_of(&[file(b"a", b"alpha"), file(b"b", &vec![1_u8; 4096])]);
    bytes.truncate(512 + 512 + 512 + 100);
    let error = run(bytes, &dest, &Policy::default()).unwrap_err();
    assert!(matches!(error.kind(), ErrorKind::Truncated), "{error}");
    let names: Vec<_> = fs::read_dir(&dest).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names, vec![std::ffi::OsString::from("keep")]);

    let limited = budget_with(Limits { entries: 1, ..Limits::default() });
    let bytes = tar_of(&[file(b"a", b"1"), file(b"b", b"2")]);
    let error = run_with(bytes, &dest, &Policy::default(), &limited).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Limit);
    assert_eq!(fs::read_dir(&dest).unwrap().count(), 1);
}

#[test]
fn a_compression_bomb_stops_at_the_budget_and_cleans_up() {
    let sandbox = Sandbox::new();
    let dest = sandbox.path("dest");
    let zeros = vec![0_u8; 32 << 20];
    let tar = tar_of(&[file(b"zeros", &zeros)]);
    let mut packed = Vec::new();
    let encode = Encode::new(Codec::Gzip, Some(1)).unwrap();
    let mut encoder = Encoder::new(Some(&encode), &mut packed).unwrap();
    std::io::Write::write_all(&mut encoder, &tar).unwrap();
    encoder.finish().unwrap();
    let limits = Limits { total_output: 4 << 20, ..Limits::default() };
    let budget = budget_with(limits);
    let mut archive = Archive::open(
        Source::from_bytes(packed),
        Format::Tar,
        Compression::Auto,
        Arc::clone(&budget),
    )
    .unwrap();
    let error = extract(&mut archive, &dest, &Policy::default(), &budget).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Limit);
    assert!(!dest.exists());
    assert!(budget.total_output() <= (4 << 20) + (256 << 10));
}

#[test]
fn cancellation_mid_body_removes_the_stage_and_the_new_destination() {
    let sandbox = Sandbox::new();
    let counter = Arc::new(AtomicU64::new(0));
    let seen = Arc::clone(&counter);
    let budget = Budget::new(Limits::default(), move || seen.fetch_add(1, Ordering::Relaxed) > 40);
    let big = vec![7_u8; 8 << 20];
    let bytes = tar_of(&[file(b"first", b"1"), file(b"big", &big)]);
    let dest = sandbox.path("dest");
    let error = run_with(bytes.clone(), &dest, &Policy::default(), &budget).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Canceled);
    assert!(!dest.exists());

    fs::create_dir_all(&dest).unwrap();
    let counter = Arc::new(AtomicU64::new(0));
    let seen = Arc::clone(&counter);
    let budget = Budget::new(Limits::default(), move || seen.fetch_add(1, Ordering::Relaxed) > 40);
    let error = run_with(bytes, &dest, &Policy::default(), &budget).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Canceled);
    assert_eq!(fs::read_dir(&dest).unwrap().count(), 0);
}

#[test]
fn large_bodies_extract_without_being_held_in_memory() {
    struct GeneratedTar {
        header: Vec<u8>,
        body: u64,
        emitted: u64,
        trailer: Vec<u8>,
        step: usize,
    }
    impl Read for GeneratedTar {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            if self.step < self.header.len() {
                let count = buffer.len().min(self.header.len() - self.step);
                buffer[..count].copy_from_slice(&self.header[self.step..self.step + count]);
                self.step += count;
                return Ok(count);
            }
            if self.emitted < self.body {
                let count = (buffer.len() as u64).min(self.body - self.emitted) as usize;
                buffer[..count].fill(0x5a);
                self.emitted += count as u64;
                return Ok(count);
            }
            let offset = self.step - self.header.len();
            let count = buffer.len().min(self.trailer.len().saturating_sub(offset));
            buffer[..count].copy_from_slice(&self.trailer[offset..offset + count]);
            self.step += count;
            Ok(count)
        }
    }
    let sandbox = Sandbox::new();
    let size = 300_u64 << 20;
    let mut header = support::tar_raw_header(b"big", b'0', size, b"").to_vec();
    header.truncate(512);
    let mut trailer = vec![0_u8; ((512 - size % 512) % 512) as usize];
    trailer.extend_from_slice(&[0_u8; 1024]);
    let source = GeneratedTar { header, body: size, emitted: 0, trailer, step: 0 };
    let budget = budget();
    let mut archive = Archive::open(
        Source::from_reader(source),
        Format::Tar,
        Compression::Plain,
        Arc::clone(&budget),
    )
    .unwrap();
    let dest = sandbox.path("dest");
    let report = extract(&mut archive, &dest, &Policy::default(), &budget).unwrap();
    assert_eq!(report.bytes, size);
    assert_eq!(fs::metadata(dest.join("big")).unwrap().len(), size);
}

#[test]
fn a_destination_that_is_a_file_is_a_collision() {
    let sandbox = Sandbox::new();
    let dest = sandbox.path("file");
    fs::write(&dest, "x").unwrap();
    let error = run(tar_of(&[file(b"a", b"1")]), &dest, &Policy::default()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Collision);
    assert_eq!(fs::read_to_string(&dest).unwrap(), "x");
}

#[test]
fn extraction_from_a_pipe_like_stream_matches_extraction_from_memory() {
    let sandbox = Sandbox::new();
    let bytes = tar_of(&[dir(b"d"), file(b"d/f", b"streamed")]);
    let mut archive = support::open_stream(bytes, Format::Tar, Compression::Auto).unwrap();
    let budget = budget();
    extract(&mut archive, &sandbox.path("dest"), &Policy::default(), &budget).unwrap();
    assert_eq!(fs::read_to_string(sandbox.path("dest/d/f")).unwrap(), "streamed");
}

#[test]
fn applied_ownership_is_an_error_when_the_kernel_refuses_it() {
    let sandbox = Sandbox::new();
    let me = fs::metadata(sandbox.path(".")).unwrap().uid();
    let apply = Policy { ownership: xsh_archive::Ownership::Apply, ..Policy::default() };
    let bytes = tar_of(&[file(b"owned", b"x")]);
    let outcome = run(bytes, &sandbox.path("dest"), &apply);
    if me == 0 {
        outcome.unwrap();
        assert_eq!(fs::metadata(sandbox.path("dest/owned")).unwrap().uid(), 0);
    } else {
        let error = outcome.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Io, "{error}");
        assert!(!sandbox.path("dest").exists());
    }
}
