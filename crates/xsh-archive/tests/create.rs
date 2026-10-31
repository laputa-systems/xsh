mod support;

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use support::{budget, list, open};
use xsh_archive::{
    Archive, Budget, Codec, Collision, Compression, CreateOptions, EntryKind, ErrorKind, Format,
    Limits, Policy, Source, create, extract,
};

fn sandbox() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn tree(base: &Path) -> PathBuf {
    let src = base.join("src");
    fs::create_dir_all(src.join("dir/deeper")).unwrap();
    fs::write(src.join("dir/a.txt"), "alpha").unwrap();
    fs::write(src.join("dir/deeper/b.txt"), "bravo").unwrap();
    fs::write(src.join("run.sh"), "#!/bin/sh\n").unwrap();
    fs::set_permissions(src.join("run.sh"), fs::Permissions::from_mode(0o755)).unwrap();
    symlink("dir/a.txt", src.join("link")).unwrap();
    src
}

fn options(format: Format) -> CreateOptions {
    CreateOptions {
        format,
        compression: Compression::Auto,
        level: None,
        collision: Collision::Refuse,
    }
}

fn names(path: &Path, format: Format) -> Vec<(Vec<u8>, EntryKind)> {
    let bytes = fs::read(path).unwrap();
    let mut archive = open(bytes, format, Compression::Auto).unwrap();
    list(&mut archive)
        .unwrap()
        .entries
        .into_iter()
        .map(|entry| (entry.name, entry.kind))
        .collect()
}

fn entries(list: &[&str]) -> Vec<Vec<u8>> {
    list.iter().map(|name| name.as_bytes().to_vec()).collect()
}

fn have(tool: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(tool).is_file()))
}

#[test]
fn tar_create_lists_in_order_and_round_trips_through_extract() {
    let base = sandbox();
    let src = tree(base.path());
    let dest = base.path().join("out.tar");
    create(&dest, &src, &entries(&["."]), &options(Format::Tar), &budget()).unwrap();
    let listed = names(&dest, Format::Tar);
    let paths: Vec<_> = listed.iter().map(|(name, _)| String::from_utf8_lossy(name).into_owned()).collect();
    assert_eq!(paths, ["dir", "dir/a.txt", "dir/deeper", "dir/deeper/b.txt", "link", "run.sh"]);
    assert_eq!(listed[4].1, EntryKind::Symlink);

    let out = base.path().join("out");
    let mut archive = Archive::open(
        Source::from_bytes(fs::read(&dest).unwrap()),
        Format::Tar,
        Compression::Auto,
        budget(),
    )
    .unwrap();
    extract(&mut archive, &out, &Policy::default(), &budget()).unwrap();
    assert_eq!(fs::read_to_string(out.join("dir/deeper/b.txt")).unwrap(), "bravo");
    assert_eq!(fs::read_link(out.join("link")).unwrap(), Path::new("dir/a.txt"));
    assert_eq!(fs::metadata(out.join("run.sh")).unwrap().permissions().mode() & 0o777, 0o755);
}

#[test]
fn tar_output_is_reproducible_and_carries_no_times_or_owners() {
    let base = sandbox();
    let src = tree(base.path());
    let first = base.path().join("one.tar");
    let second = base.path().join("two.tar");
    create(&first, &src, &entries(&["."]), &options(Format::Tar), &budget()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    create(&second, &src, &entries(&["."]), &options(Format::Tar), &budget()).unwrap();
    assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());
    let mut archive = open(fs::read(&first).unwrap(), Format::Tar, Compression::Plain).unwrap();
    let listing = list(&mut archive).unwrap();
    assert!(listing.entries.iter().all(|entry| entry.modified == Some(0) && entry.uid == Some(0)));
}

#[test]
fn named_entries_keep_their_relative_names_and_directories_recurse() {
    let base = sandbox();
    let src = tree(base.path());
    let dest = base.path().join("named.tar");
    create(&dest, &src, &entries(&["dir/deeper", "run.sh"]), &options(Format::Tar), &budget()).unwrap();
    let listed: Vec<_> = names(&dest, Format::Tar).into_iter().map(|(n, _)| String::from_utf8(n).unwrap()).collect();
    assert_eq!(listed, ["dir/deeper", "dir/deeper/b.txt", "run.sh"]);
}

#[test]
fn long_names_and_long_links_survive() {
    let base = sandbox();
    let src = base.path().join("src");
    let long_dir = "d".repeat(120);
    fs::create_dir_all(src.join(&long_dir)).unwrap();
    let long_name = format!("{long_dir}/{}", "f".repeat(130));
    fs::write(src.join(&long_name), "long").unwrap();
    let long_target = "t".repeat(150);
    symlink(&long_target, src.join("longlink")).unwrap();

    for format in [Format::Tar, Format::Cpio] {
        let dest = base.path().join(format!("long.{}", format.name()));
        create(&dest, &src, &entries(&["."]), &options(format), &budget()).unwrap();
        let bytes = fs::read(&dest).unwrap();
        let mut archive = open(bytes, format, Compression::Plain).unwrap();
        let listing = list(&mut archive).unwrap();
        let by_name = |name: &[u8]| listing.entries.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("{format:?}: {}", String::from_utf8_lossy(name)));
        assert_eq!(by_name(long_name.as_bytes()).size, Some(4), "{format:?}");
        assert_eq!(by_name(b"longlink").link.as_deref(), Some(long_target.as_bytes()), "{format:?}");
    }
}

// APFS rejects file names and symlink targets that are not valid UTF-8.
#[cfg(not(target_os = "macos"))]
#[test]
fn non_utf8_names_and_link_targets_survive() {
    let base = sandbox();
    let src = base.path().join("src");
    fs::create_dir_all(&src).unwrap();
    let raw_name = {
        use std::os::unix::ffi::OsStrExt;
        std::ffi::OsStr::from_bytes(b"caf\xe9").to_os_string()
    };
    fs::write(src.join(&raw_name), "odd").unwrap();
    symlink({
        use std::os::unix::ffi::OsStrExt;
        std::ffi::OsStr::from_bytes(b"t\xffarget").to_os_string()
    }, src.join("oddlink")).unwrap();

    for format in [Format::Tar, Format::Cpio] {
        let dest = base.path().join(format!("odd.{}", format.name()));
        create(&dest, &src, &entries(&["."]), &options(format), &budget()).unwrap();
        let bytes = fs::read(&dest).unwrap();
        let mut archive = open(bytes, format, Compression::Plain).unwrap();
        let listing = list(&mut archive).unwrap();
        let by_name = |name: &[u8]| listing.entries.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("{format:?}: {}", String::from_utf8_lossy(name)));
        assert_eq!(by_name(b"oddlink").link.as_deref(), Some(&b"t\xffarget"[..]), "{format:?}");
        by_name(b"caf\xe9");
    }
}

#[test]
fn compression_follows_the_destination_suffix_or_the_request() {
    let base = sandbox();
    let src = tree(base.path());
    for (name, codec) in [("a.tar.gz", Codec::Gzip), ("a.tar.bz2", Codec::Bzip2), ("a.tar.xz", Codec::Xz), ("a.tar.zst", Codec::Zstd)] {
        let dest = base.path().join(name);
        create(&dest, &src, &entries(&["."]), &options(Format::Tar), &budget()).unwrap();
        let bytes = fs::read(&dest).unwrap();
        assert_eq!(Codec::ALL.into_iter().find(|c| c.name() == codec.name()), Some(codec));
        let mut archive = open(bytes, Format::Tar, Compression::Codec(codec)).unwrap();
        assert_eq!(list(&mut archive).unwrap().entries.len(), 6, "{name}");
    }
    let plain = base.path().join("no-suffix");
    create(&plain, &src, &entries(&["."]), &options(Format::Tar), &budget()).unwrap();
    assert_eq!(fs::read(&plain).unwrap().len() % 512, 0);
    let forced = base.path().join("forced");
    let mut request = options(Format::Tar);
    request.compression = Compression::Codec(Codec::Gzip);
    create(&forced, &src, &entries(&["."]), &request, &budget()).unwrap();
    assert_eq!(&fs::read(&forced).unwrap()[..2], &[0x1f, 0x8b]);
}

#[test]
fn invalid_requests_fail_before_anything_is_written() {
    let base = sandbox();
    let src = tree(base.path());
    let dest = base.path().join("never.tar.gz");
    let mut bad_level = options(Format::Tar);
    bad_level.level = Some(10);
    assert_eq!(create(&dest, &src, &entries(&["."]), &bad_level, &budget()).unwrap_err().kind(), ErrorKind::Options);
    let plain = base.path().join("never.tar");
    let mut level_without_codec = options(Format::Tar);
    level_without_codec.level = Some(3);
    assert_eq!(create(&plain, &src, &entries(&["."]), &level_without_codec, &budget()).unwrap_err().kind(), ErrorKind::Options);
    assert_eq!(create(&plain, &src, &[], &options(Format::Tar), &budget()).unwrap_err().kind(), ErrorKind::Options);
    assert_eq!(create(&plain, &src, &entries(&["../x"]), &options(Format::Tar), &budget()).unwrap_err().kind(), ErrorKind::Unsafe);
    assert_eq!(create(&plain, &src, &entries(&["."]), &options(Format::Zip), &budget()).unwrap_err().kind(), ErrorKind::Unsupported);
    assert!(!dest.exists() && !plain.exists());
    assert!(fs::read_dir(base.path()).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().starts_with(".xsh-archive")));
}

#[test]
fn an_existing_destination_survives_a_refused_or_failed_create() {
    let base = sandbox();
    let src = tree(base.path());
    let dest = base.path().join("out.tar");
    fs::write(&dest, "precious").unwrap();
    let error = create(&dest, &src, &entries(&["."]), &options(Format::Tar), &budget()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Collision);
    assert_eq!(fs::read_to_string(&dest).unwrap(), "precious");

    let error = create(&dest, &src, &entries(&["missing"]), &CreateOptions { collision: Collision::Replace, ..options(Format::Tar) }, &budget()).unwrap_err();
    assert_eq!(error.io_kind(), Some(std::io::ErrorKind::NotFound));
    assert_eq!(fs::read_to_string(&dest).unwrap(), "precious");
    assert!(fs::read_dir(base.path()).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().starts_with(".xsh-archive")));

    create(&dest, &src, &entries(&["."]), &CreateOptions { collision: Collision::Replace, ..options(Format::Tar) }, &budget()).unwrap();
    assert!(fs::read(&dest).unwrap().len() > 1024);
}

#[test]
fn the_archive_does_not_contain_itself_when_written_inside_its_source() {
    let base = sandbox();
    let src = tree(base.path());
    let dest = src.join("self.tar");
    create(&dest, &src, &entries(&["."]), &options(Format::Tar), &budget()).unwrap();
    let listed = names(&dest, Format::Tar);
    assert!(listed.iter().all(|(name, _)| name != b"self.tar"));
    assert_eq!(listed.len(), 6);
}

#[test]
fn symlinks_are_archived_as_links_and_sockets_are_refused() {
    let base = sandbox();
    let src = base.path().join("src");
    fs::create_dir(&src).unwrap();
    fs::create_dir(base.path().join("elsewhere")).unwrap();
    fs::write(base.path().join("elsewhere/secret"), "secret").unwrap();
    symlink("../elsewhere", src.join("out")).unwrap();
    let dest = base.path().join("links.tar");
    create(&dest, &src, &entries(&["."]), &options(Format::Tar), &budget()).unwrap();
    assert_eq!(names(&dest, Format::Tar), vec![(b"out".to_vec(), EntryKind::Symlink)]);

    let _listener = std::os::unix::net::UnixListener::bind(src.join("sock")).unwrap();
    let blocked = base.path().join("blocked.tar");
    let error = create(&blocked, &src, &entries(&["."]), &options(Format::Tar), &budget()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Unsupported);
    assert!(!blocked.exists());
}

#[test]
fn cpio_create_round_trips_kinds_and_special_files() {
    let base = sandbox();
    let src = tree(base.path());
    assert!(Command::new("mkfifo").arg(src.join("pipe")).status().unwrap().success());
    let dest = base.path().join("out.cpio");
    create(&dest, &src, &entries(&["."]), &options(Format::Cpio), &budget()).unwrap();
    let listed = names(&dest, Format::Cpio);
    let kind = |name: &str| listed.iter().find(|(n, _)| n == name.as_bytes()).unwrap().1;
    assert_eq!(kind("dir"), EntryKind::Directory);
    assert_eq!(kind("link"), EntryKind::Symlink);
    assert_eq!(kind("pipe"), EntryKind::Fifo);
    assert_eq!(kind("run.sh"), EntryKind::File);
    let mut archive = open(fs::read(&dest).unwrap(), Format::Cpio, Compression::Plain).unwrap();
    let bodies = support::read_all(&mut archive).unwrap();
    let body = bodies.iter().find(|(m, _)| m.name == b"dir/a.txt").unwrap();
    assert_eq!(body.1, b"alpha");
}

#[test]
fn cancellation_leaves_no_temporary_file() {
    let base = sandbox();
    let src = tree(base.path());
    let calls = Arc::new(AtomicU64::new(0));
    let seen = Arc::clone(&calls);
    let budget = Budget::new(Limits::default(), move || seen.fetch_add(1, Ordering::Relaxed) >= 3);
    let dest = base.path().join("c.tar");
    let error = create(&dest, &src, &entries(&["."]), &options(Format::Tar), &budget).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Canceled);
    assert!(!dest.exists());
    assert!(fs::read_dir(base.path()).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().starts_with(".xsh-archive")));
}

#[test]
fn the_output_budget_limits_what_is_read_from_the_source() {
    let base = sandbox();
    let src = base.path().join("src");
    fs::create_dir(&src).unwrap();
    fs::write(src.join("big"), vec![1_u8; 1 << 20]).unwrap();
    let budget = Budget::uncancelable(Limits { total_output: 1 << 16, ..Limits::default() });
    let dest = base.path().join("big.tar");
    let error = create(&dest, &src, &entries(&["."]), &options(Format::Tar), &budget).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Limit);
    assert!(!dest.exists());
}

#[test]
fn archives_read_back_by_python_tarfile_and_busybox_tar() {
    let base = sandbox();
    let src = tree(base.path());
    let dest = base.path().join("interop.tar.gz");
    create(&dest, &src, &entries(&["."]), &options(Format::Tar), &budget()).unwrap();
    if have("python3") {
        let script = "import sys,tarfile\nt=tarfile.open(sys.argv[1])\nfor m in t.getmembers():\n    data = t.extractfile(m).read() if m.isfile() else b''\n    print(m.name, m.type.decode(), len(data), oct(m.mode))\n";
        let output = Command::new("python3").args(["-I", "-c", script]).arg(&dest).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("dir/a.txt 0 5 0o644"), "{text}");
        assert!(text.contains("link 2 0"), "{text}");
        assert!(text.contains("run.sh 0 10 0o755"), "{text}");
    } else {
        eprintln!("skipped python tarfile oracle: python3 is not installed");
    }
    if have("tar") {
        let output = Command::new("tar").arg("-tzf").arg(&dest).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("dir/deeper/b.txt"), "{text}");
    } else {
        eprintln!("skipped tar oracle: tar is not installed");
    }
}

#[test]
fn archives_written_by_external_tools_are_read_and_extracted() {
    let base = sandbox();
    let src = tree(base.path());
    if have("tar") {
        let dest = base.path().join("by-tar.tar");
        let status = Command::new("tar").arg("-cf").arg(&dest).arg("-C").arg(&src).arg(".").status().unwrap();
        assert!(status.success());
        let out = base.path().join("by-tar-out");
        let mut archive = Archive::open(Source::from_bytes(fs::read(&dest).unwrap()), Format::Tar, Compression::Auto, budget()).unwrap();
        extract(&mut archive, &out, &Policy::default(), &budget()).unwrap();
        assert_eq!(fs::read_to_string(out.join("dir/a.txt")).unwrap(), "alpha");
        assert_eq!(fs::read_link(out.join("link")).unwrap(), Path::new("dir/a.txt"));
    } else {
        eprintln!("skipped tar writer: tar is not installed");
    }
    if have("python3") {
        for (format, label) in [("USTAR_FORMAT", "ustar"), ("GNU_FORMAT", "gnu"), ("PAX_FORMAT", "pax")] {
            let dest = base.path().join(format!("by-python-{label}.tar"));
            let script = format!(
                "import sys,tarfile\nt=tarfile.open(sys.argv[1],'w',format=tarfile.{format})\nt.add(sys.argv[2],arcname='root')\nt.close()\n"
            );
            let status = Command::new("python3").args(["-I", "-c", &script]).arg(&dest).arg(&src).status().unwrap();
            assert!(status.success());
            let out = base.path().join(format!("by-python-{label}-out"));
            let mut archive = Archive::open(Source::from_bytes(fs::read(&dest).unwrap()), Format::Tar, Compression::Auto, budget()).unwrap();
            extract(&mut archive, &out, &Policy::default(), &budget()).unwrap();
            assert_eq!(fs::read_to_string(out.join("root/dir/deeper/b.txt")).unwrap(), "bravo", "{label}");
        }
        let zipped = base.path().join("by-python.zip");
        let script = "import sys,zipfile\nz=zipfile.ZipFile(sys.argv[1],'w',zipfile.ZIP_DEFLATED)\nz.writestr('a/b.txt','zipped')\nz.writestr('empty-dir/','')\nz.close()\n";
        assert!(Command::new("python3").args(["-I", "-c", script]).arg(&zipped).status().unwrap().success());
        let mut archive = open(fs::read(&zipped).unwrap(), Format::Zip, Compression::Auto).unwrap();
        let bodies = support::read_all(&mut archive).unwrap();
        assert_eq!(bodies[0].1, b"zipped");
        assert_eq!(bodies[1].0.kind, EntryKind::Directory);
    } else {
        eprintln!("skipped python writers: python3 is not installed");
    }
    if have("zip") {
        let zipped = base.path().join("by-zip.zip");
        let status = Command::new("zip").current_dir(&src).args(["-qr"]).arg(&zipped).arg(".").status().unwrap();
        assert!(status.success());
        let mut archive = open(fs::read(&zipped).unwrap(), Format::Zip, Compression::Auto).unwrap();
        let listing = list(&mut archive).unwrap();
        assert!(listing.entries.iter().any(|e| e.name == b"dir/a.txt"));
        assert!(listing.entries.iter().all(|e| e.modified.is_some()));
        let out = base.path().join("by-zip-out");
        let mut archive = open(fs::read(&zipped).unwrap(), Format::Zip, Compression::Auto).unwrap();
        extract(&mut archive, &out, &Policy { links: xsh_archive::Links::Contained, ..Policy::default() }, &budget()).unwrap();
        assert_eq!(fs::read_to_string(out.join("dir/deeper/b.txt")).unwrap(), "bravo");
    } else {
        eprintln!("skipped zip writer: zip is not installed");
    }
    if have("cpio") {
        let dest = base.path().join("by-cpio.cpio");
        let status = Command::new("sh")
            .current_dir(&src)
            .arg("-c")
            .arg("find . -mindepth 1 | sort | cpio -o -H newc --quiet > \"$1\"")
            .arg("cpio")
            .arg(&dest)
            .status()
            .unwrap();
        assert!(status.success());
        let mut archive = open(fs::read(&dest).unwrap(), Format::Cpio, Compression::Auto).unwrap();
        let listing = list(&mut archive).unwrap();
        assert!(listing.entries.iter().any(|e| e.name.ends_with(b"b.txt")));
    } else {
        eprintln!("skipped cpio writer: cpio is not installed");
    }
}

#[test]
fn archives_written_by_the_crate_pass_external_readers() {
    let base = sandbox();
    let src = tree(base.path());
    let dest = base.path().join("ours.cpio");
    create(&dest, &src, &entries(&["."]), &options(Format::Cpio), &budget()).unwrap();
    if have("cpio") {
        let output = Command::new("sh")
            .arg("-c")
            .arg("cpio -it --quiet < \"$1\"")
            .arg("cpio")
            .arg(&dest)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("dir/deeper/b.txt"), "{text}");
    } else {
        eprintln!("skipped cpio reader: cpio is not installed");
    }
}
