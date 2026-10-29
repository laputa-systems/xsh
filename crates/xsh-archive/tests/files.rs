mod support;

use std::fs;
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
