mod support;

use std::fs;
use std::io::Write;
use lzma_rust2::{XzOptions, XzWriter};
use support::{budget, budget_with};
use xsh_archive::{
    Codec, Collision, Compression, ErrorKind, Limits, compress_file, decompress_file,
    decompress_to_memory,
};

#[test]
fn every_codec_round_trips_through_files_with_the_suffix_or_a_named_codec() {
    let base = tempfile::tempdir().unwrap();
    let source = base.path().join("payload");
    let text = b"alpha beta gamma\n".repeat(2000);
    fs::write(&source, &text).unwrap();
    for (suffix, codec) in [
        ("gz", Codec::Gzip),
        ("bz2", Codec::Bzip2),
        ("xz", Codec::Xz),
        ("lzma", Codec::Lzma),
        ("zst", Codec::Zstd),
    ] {
        let packed = base.path().join(format!("payload.{suffix}"));
        compress_file(&source, &packed, Compression::Auto, None, Collision::Refuse, &budget()).unwrap();
        assert!(fs::metadata(&packed).unwrap().len() < text.len() as u64 / 4, "{suffix}");
        let restored = base.path().join(format!("restored-{suffix}"));
        decompress_file(&packed, &restored, Compression::Auto, Collision::Refuse, &budget()).unwrap();
        assert_eq!(fs::read(&restored).unwrap(), text, "{suffix}");
        assert_eq!(decompress_to_memory(&packed, Compression::Codec(codec), &budget()).unwrap(), text);

        let named = base.path().join(format!("named-{suffix}.out"));
        compress_file(&source, &named, Compression::Codec(codec), Some(1), Collision::Refuse, &budget()).unwrap();
        assert_eq!(decompress_to_memory(&named, Compression::Codec(codec), &budget()).unwrap(), text);
    }
}

#[test]
fn an_unnamed_format_is_an_option_error_and_writes_nothing() {
    let base = tempfile::tempdir().unwrap();
    let source = base.path().join("payload");
    fs::write(&source, "x").unwrap();
    let dest = base.path().join("payload.unknown");
    let error = compress_file(&source, &dest, Compression::Auto, None, Collision::Refuse, &budget()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Options);
    assert!(!dest.exists());
    let error = decompress_file(&source, &dest, Compression::Auto, Collision::Refuse, &budget()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Options);

    let bad_level = base.path().join("payload.gz");
    let error = compress_file(&source, &bad_level, Compression::Auto, Some(10), Collision::Refuse, &budget()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Options);
    assert!(!bad_level.exists());
}

#[test]
fn damaged_and_cut_inputs_fail_without_publishing_output() {
    let base = tempfile::tempdir().unwrap();
    let source = base.path().join("payload");
    fs::write(&source, vec![b'z'; 100_000]).unwrap();
    let packed = base.path().join("payload.xz");
    compress_file(&source, &packed, Compression::Auto, None, Collision::Refuse, &budget()).unwrap();
    let bytes = fs::read(&packed).unwrap();
    for cut in [1_usize, 4, 12] {
        let broken = base.path().join("broken.xz");
        fs::write(&broken, &bytes[..bytes.len() - cut]).unwrap();
        let dest = base.path().join("out");
        let error = decompress_file(&broken, &dest, Compression::Auto, Collision::Refuse, &budget()).unwrap_err();
        assert!(matches!(error.kind(), ErrorKind::Truncated | ErrorKind::Corrupt), "cut {cut}: {error}");
        assert!(!dest.exists());
    }
    let leftovers: Vec<_> = fs::read_dir(base.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".xsh-archive"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn collisions_and_expansion_budgets_apply_to_single_files() {
    let base = tempfile::tempdir().unwrap();
    let source = base.path().join("payload");
    fs::write(&source, vec![0_u8; 8 << 20]).unwrap();
    let packed = base.path().join("payload.gz");
    compress_file(&source, &packed, Compression::Auto, None, Collision::Refuse, &budget()).unwrap();
    let error = compress_file(&source, &packed, Compression::Auto, None, Collision::Refuse, &budget()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Collision);
    compress_file(&source, &packed, Compression::Auto, None, Collision::Replace, &budget()).unwrap();

    let limited = budget_with(Limits { total_output: 1 << 20, ..Limits::default() });
    let error = decompress_to_memory(&packed, Compression::Auto, &limited).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Limit);
    assert!(limited.total_output() <= (1 << 20) + (256 << 10));
}

#[test]
fn xz_workspace_limit_applies_to_later_members() {
    fn member(text: &[u8], dictionary: u32) -> Vec<u8> {
        let mut options = XzOptions::with_preset(0);
        options.lzma_options.dict_size = dictionary;
        let mut writer = XzWriter::new(Vec::new(), options).unwrap();
        writer.write_all(text).unwrap();
        writer.finish().unwrap()
    }

    let base = tempfile::tempdir().unwrap();
    let path = base.path().join("members.xz");
    let mut packed = member(b"first", 1 << 16);
    packed.extend(member(b"second", 1 << 20));
    fs::write(&path, packed).unwrap();
    let allowed = budget_with(Limits { workspace_bytes: 2 << 20, ..Limits::default() });
    assert_eq!(decompress_to_memory(&path, Compression::Auto, &allowed).unwrap(), b"firstsecond");
    let limited = budget_with(Limits { workspace_bytes: 1 << 17, ..Limits::default() });
    let error = decompress_to_memory(&path, Compression::Auto, &limited).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Limit);
}

#[test]
fn xz_workspace_limit_applies_to_later_blocks() {
    let base = tempfile::tempdir().unwrap();
    let path = base.path().join("blocks.xz");
    fs::write(&path, include_bytes!("fixtures/two-blocks.xz")).unwrap();
    let allowed = budget_with(Limits { workspace_bytes: 2 << 20, ..Limits::default() });
    assert_eq!(decompress_to_memory(&path, Compression::Auto, &allowed).unwrap(), b"firstsecond");
    let limited = budget_with(Limits { workspace_bytes: 1 << 17, ..Limits::default() });
    let error = decompress_to_memory(&path, Compression::Auto, &limited).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Limit);
}

#[test]
fn xz_filter_chains_and_trailing_data_are_checked() {
    let base = tempfile::tempdir().unwrap();
    let path = base.path().join("filtered.xz");
    let mut packed = include_bytes!("fixtures/filter-chain.xz").to_vec();
    fs::write(&path, &packed).unwrap();
    let limited = budget_with(Limits { workspace_bytes: 1 << 20, ..Limits::default() });
    assert_eq!(decompress_to_memory(&path, Compression::Auto, &limited).unwrap(), b"abcdefghijklmno");
    packed.extend_from_slice(b"junk");
    fs::write(&path, packed).unwrap();
    let error = decompress_to_memory(&path, Compression::Auto, &limited).unwrap_err();
    assert!(matches!(error.kind(), ErrorKind::Corrupt | ErrorKind::Truncated));
}

#[test]
fn xz_unknown_integrity_check_is_rejected() {
    let mut writer = XzWriter::new(Vec::new(), XzOptions::with_preset(0)).unwrap();
    writer.write_all(b"integrity").unwrap();
    let mut packed = writer.finish().unwrap();
    packed[7] = 5;
    let mut crc = flate2::Crc::new();
    crc.update(&packed[6..8]);
    packed[8..12].copy_from_slice(&crc.sum().to_le_bytes());
    let footer = packed.len() - 12;
    packed[footer + 9] = 5;
    crc.reset();
    crc.update(&packed[footer + 4..footer + 10]);
    packed[footer..footer + 4].copy_from_slice(&crc.sum().to_le_bytes());
    let base = tempfile::tempdir().unwrap();
    let path = base.path().join("unknown-check.xz");
    fs::write(&path, packed).unwrap();
    let error = decompress_to_memory(&path, Compression::Auto, &budget()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Io);
}
