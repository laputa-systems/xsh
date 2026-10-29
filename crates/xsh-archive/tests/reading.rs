mod support;

use std::io::{Read, Write};
use support::*;
use xsh_archive::{
    Archive, Budget, Codec, Compression, Completion, Encode, Encoder, EntryKind, ErrorKind, Format,
    Limits, Source,
};

const NO_NAME: &[u8] = b"";

#[test]
fn tar_lists_entries_with_metadata_and_complete_structure() {
    let bytes = tar_with(&[(b"a.txt", b"alpha"), (b"dir/b.txt", b"bravo!")]);
    let mut archive = open(bytes, Format::Tar, Compression::Plain).unwrap();
    let listing = list(&mut archive).unwrap();
    let names: Vec<_> = listing.entries.iter().map(|entry| entry.name.clone()).collect();
    assert_eq!(names, vec![b"a.txt".to_vec(), b"dir/b.txt".to_vec()]);
    assert_eq!(listing.entries[1].size, Some(6));
    assert_eq!(listing.entries[0].modified, Some(1_700_000_000));
    assert_eq!(listing.entries[0].mode, Some(0o644));
    assert!(listing.completion.contains(Completion::STRUCTURE_COMPLETE));
    assert!(listing.completion.contains(Completion::OUTER_COMPLETE));
    assert!(!listing.completion.contains(Completion::PAYLOAD_CONSUMED));
}

#[test]
fn tar_bodies_are_delivered_and_unread_bodies_are_skipped() {
    let bytes = tar_with(&[(b"a", b"first body"), (b"b", b"second body")]);
    let mut archive = open(bytes, Format::Tar, Compression::Plain).unwrap();
    let mut first = archive.next_entry().unwrap().unwrap();
    let mut prefix = [0_u8; 3];
    first.read_exact(&mut prefix).unwrap();
    assert_eq!(&prefix, b"fir");
    drop(first);
    let mut second = archive.next_entry().unwrap().unwrap();
    let mut body = Vec::new();
    second.read_to_end(&mut body).unwrap();
    assert_eq!(body, b"second body");
    drop(second);
    assert!(archive.next_entry().unwrap().is_none());
    let completion = archive.finish(true).unwrap();
    assert!(completion.contains(Completion::PAYLOAD_CONSUMED));
}

#[test]
fn tar_keeps_non_utf8_names_and_link_targets_as_bytes() {
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_size(0);
    header.set_entry_type(tar::EntryType::Symlink);
    header.set_mode(0o777);
    builder.append_link_bytes(&mut header, b"link\xff", b"target\xfe").unwrap();
    let bytes = builder.into_inner().unwrap();
    let mut archive = open(bytes, Format::Tar, Compression::Plain).unwrap();
    let listing = list(&mut archive).unwrap();
    assert_eq!(listing.entries[0].name, b"link\xff");
    assert_eq!(listing.entries[0].kind, EntryKind::Symlink);
    assert_eq!(listing.entries[0].link.as_deref(), Some(&b"target\xfe"[..]));
}

trait AppendLinkBytes {
    fn append_link_bytes(&mut self, header: &mut tar::Header, name: &[u8], target: &[u8]) -> std::io::Result<()>;
}

impl AppendLinkBytes for tar::Builder<Vec<u8>> {
    fn append_link_bytes(&mut self, header: &mut tar::Header, name: &[u8], target: &[u8]) -> std::io::Result<()> {
        use std::os::unix::ffi::OsStrExt;
        self.append_link(
            header,
            std::ffi::OsStr::from_bytes(name),
            std::ffi::OsStr::from_bytes(target),
        )
    }
}

#[test]
fn tar_reads_gnu_long_names_and_pax_names() {
    let long = "d".repeat(180);
    let bytes = tar_with(&[(long.as_bytes(), b"x")]);
    let mut archive = open(bytes, Format::Tar, Compression::Plain).unwrap();
    assert_eq!(list(&mut archive).unwrap().entries[0].name, long.as_bytes());

    let mut builder = tar::Builder::new(Vec::new());
    builder
        .append_pax_extensions([("path", b"pax/path/name".as_slice()), ("mtime", b"1234.5".as_slice())])
        .unwrap();
    let mut header = tar::Header::new_ustar();
    header.set_size(2);
    header.set_mode(0o600);
    header.set_mtime(7);
    header.set_cksum();
    builder.append_data(&mut header, "ignored", &b"hi"[..]).unwrap();
    let mut archive = open(builder.into_inner().unwrap(), Format::Tar, Compression::Plain).unwrap();
    let entry = &list(&mut archive).unwrap().entries[0];
    assert_eq!(entry.name, b"pax/path/name");
    assert_eq!(entry.modified, Some(1234));
}

#[test]
fn tar_without_end_of_archive_blocks_is_truncated() {
    let mut bytes = tar_with(&[(b"a", b"alpha")]);
    bytes.truncate(1024);
    let mut archive = open(bytes, Format::Tar, Compression::Plain).unwrap();
    assert_eq!(kind_of(list(&mut archive)), ErrorKind::Truncated);
}

#[test]
fn tar_truncated_at_every_length_is_never_reported_complete() {
    let bytes = tar_with(&[(b"a", b"alpha"), (b"b", &[7_u8; 700])]);
    let trailer_start = bytes.len() - 1024;
    for length in 0..bytes.len() {
        let mut archive = open(bytes[..length].to_vec(), Format::Tar, Compression::Plain).unwrap();
        let result = list(&mut archive);
        if length < trailer_start + 512 {
            let error = result.err().unwrap_or_else(|| panic!("length {length} was accepted"));
            assert!(
                matches!(error.kind(), ErrorKind::Truncated | ErrorKind::Corrupt),
                "length {length}: {error}"
            );
        } else {
            assert!(result.is_ok(), "length {length}: {:?}", result.err());
        }
    }
}

#[test]
fn tar_header_checksum_and_trailing_data_are_corrupt() {
    let mut bytes = tar_with(&[(b"a", b"alpha")]);
    bytes[0] ^= 0x01;
    let mut archive = open(bytes, Format::Tar, Compression::Plain).unwrap();
    assert_eq!(kind_of(list(&mut archive)), ErrorKind::Corrupt);

    let mut bytes = tar_with(&[(b"a", b"alpha")]);
    bytes.extend_from_slice(b"appended payload");
    let mut archive = open(bytes, Format::Tar, Compression::Plain).unwrap();
    assert_eq!(kind_of(list(&mut archive)), ErrorKind::Corrupt);

    let mut bytes = tar_with(&[(b"a", b"alpha")]);
    bytes.resize(bytes.len() + 4096, 0);
    let mut archive = open(bytes, Format::Tar, Compression::Plain).unwrap();
    assert!(list(&mut archive).is_ok());
}

#[test]
fn tar_declared_size_beyond_the_entry_limit_stops_before_reading_it() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&tar_raw_header(b"huge", b'0', 0o77_777_777_777, b""));
    let limits = Limits { entry_output: 1 << 20, ..Limits::default() };
    let mut archive =
        Archive::open(Source::from_bytes(bytes), Format::Tar, Compression::Plain, budget_with(limits))
            .unwrap();
    assert_eq!(kind_of(list(&mut archive)), ErrorKind::Limit);
}

#[test]
fn tar_oversized_extension_header_is_a_limit_not_an_allocation() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&tar_raw_header(b"././@LongLink", b'L', u64::from(u32::MAX), b""));
    let mut archive = open(bytes, Format::Tar, Compression::Plain).unwrap();
    assert_eq!(kind_of(list(&mut archive)), ErrorKind::Limit);
}

#[test]
fn entry_count_and_metadata_memory_are_budgeted() {
    let many: Vec<(Vec<u8>, &[u8])> = (0..50).map(|n| (format!("file-{n}").into_bytes(), &b""[..])).collect();
    let borrowed: Vec<(&[u8], &[u8])> = many.iter().map(|(n, b)| (n.as_slice(), *b)).collect();
    let bytes = tar_with(&borrowed);
    let limits = Limits { entries: 10, ..Limits::default() };
    let mut archive =
        Archive::open(Source::from_bytes(bytes.clone()), Format::Tar, Compression::Plain, budget_with(limits))
            .unwrap();
    assert_eq!(kind_of(list(&mut archive)), ErrorKind::Limit);
    let limits = Limits { index_bytes: 600, ..Limits::default() };
    let mut archive =
        Archive::open(Source::from_bytes(bytes), Format::Tar, Compression::Plain, budget_with(limits))
            .unwrap();
    assert_eq!(kind_of(list(&mut archive)), ErrorKind::Limit);
}

struct ZeroTar {
    header: Vec<u8>,
    body: u64,
    emitted: u64,
    trailer: Vec<u8>,
    position: usize,
}

impl ZeroTar {
    fn new(body: u64) -> Self {
        let padding = (512 - body % 512) % 512;
        let mut trailer = vec![0_u8; padding as usize];
        trailer.extend_from_slice(&[0_u8; 1024]);
        Self {
            header: tar_raw_header(b"zeros", b'0', body, b"").to_vec(),
            body,
            emitted: 0,
            trailer,
            position: 0,
        }
    }
}

impl Read for ZeroTar {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.position < self.header.len() {
            let count = buffer.len().min(self.header.len() - self.position);
            buffer[..count].copy_from_slice(&self.header[self.position..self.position + count]);
            self.position += count;
            return Ok(count);
        }
        if self.emitted < self.body {
            let count = (buffer.len() as u64).min(self.body - self.emitted) as usize;
            buffer[..count].fill(0);
            self.emitted += count as u64;
            return Ok(count);
        }
        let offset = self.position - self.header.len();
        let count = buffer.len().min(self.trailer.len().saturating_sub(offset));
        buffer[..count].copy_from_slice(&self.trailer[offset..offset + count]);
        self.position += count;
        Ok(count)
    }
}

#[test]
fn large_bodies_stream_without_buffering() {
    let body = 768 << 20;
    let mut archive = Archive::open(
        Source::from_reader(ZeroTar::new(body)),
        Format::Tar,
        Compression::Plain,
        budget(),
    )
    .unwrap();
    let mut entry = archive.next_entry().unwrap().unwrap();
    let mut total = 0_u64;
    let mut chunk = vec![0_u8; 1 << 16];
    loop {
        let count = entry.read(&mut chunk).unwrap();
        if count == 0 {
            break;
        }
        total += count as u64;
    }
    drop(entry);
    assert_eq!(total, body);
    assert!(archive.next_entry().unwrap().is_none());
    assert!(archive.finish(true).unwrap().contains(Completion::STRUCTURE_COMPLETE));
}

#[test]
fn expansion_beyond_the_total_budget_stops_within_one_buffer() {
    let plain = ZeroTar::new(64 << 20);
    let mut compressed = Vec::new();
    {
        let encode = Encode::new(Codec::Gzip, Some(1)).unwrap();
        let mut encoder = Encoder::new(Some(&encode), &mut compressed).unwrap();
        let mut plain = plain;
        std::io::copy(&mut plain, &mut encoder).unwrap();
        encoder.finish().unwrap();
    }
    assert!(compressed.len() < 1 << 20);
    let limit = 4 << 20;
    let budget = budget_with(Limits { total_output: limit, ..Limits::default() });
    let mut archive = Archive::open(
        Source::from_bytes(compressed),
        Format::Tar,
        Compression::Auto,
        std::sync::Arc::clone(&budget),
    )
    .unwrap();
    assert_eq!(kind_of(drain(&mut archive)), ErrorKind::Limit);
    assert!(budget.total_output() <= limit + (256 << 10), "read {}", budget.total_output());
}

fn drain(archive: &mut Archive) -> Result<(), xsh_archive::Error> {
    while let Some(mut entry) = archive.next_entry()? {
        std::io::copy(&mut entry, &mut std::io::sink())
            .map_err(|error| xsh_archive::Error::from_read(&error))?;
    }
    archive.finish(true).map(|_| ())
}

#[test]
fn cancellation_is_observed_between_buffers() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let flag = std::sync::Arc::new(AtomicBool::new(false));
    let seen = std::sync::Arc::clone(&flag);
    let budget = Budget::new(Limits::default(), move || seen.load(Ordering::Relaxed));
    let mut archive = Archive::open(
        Source::from_reader(ZeroTar::new(1 << 30)),
        Format::Tar,
        Compression::Plain,
        budget,
    )
    .unwrap();
    let mut entry = archive.next_entry().unwrap().unwrap();
    let mut chunk = vec![0_u8; 1 << 16];
    entry.read_exact(&mut chunk).unwrap();
    flag.store(true, Ordering::Relaxed);
    let error = entry.read(&mut chunk).unwrap_err();
    assert_eq!(xsh_archive::Error::from_read(&error).kind(), ErrorKind::Canceled);
}

#[test]
fn tar_and_cpio_read_from_a_one_byte_stream() {
    let tar = tar_with(&[(b"a", b"alpha"), (b"b", b"bravo")]);
    let mut archive = open_stream(tar, Format::Tar, Compression::Auto).unwrap();
    let bodies = read_all(&mut archive).unwrap();
    assert_eq!(bodies[1].1, b"bravo");

    let cpio = cpio_with(&[(b"a", b"alpha"), (b"b", b"bravo")]);
    let mut archive = open_stream(cpio, Format::Cpio, Compression::Auto).unwrap();
    let bodies = read_all(&mut archive).unwrap();
    assert_eq!(bodies[0].0.name, b"a");
    assert_eq!(bodies[0].1, b"alpha");
}

#[test]
fn zip_rejects_a_stream_before_reading_it_and_accepts_memory() {
    let zip = zip_with(&[("a.txt", b"alpha", 1_700_000_000)]);
    let error = open_stream(zip.clone(), Format::Zip, Compression::Auto).err().unwrap();
    assert_eq!(error.kind(), ErrorKind::NeedsSeek);
    assert!(error.message().contains("seekable"));
    let mut archive = open(zip, Format::Zip, Compression::Auto).unwrap();
    assert_eq!(read_all(&mut archive).unwrap()[0].1, b"alpha");
}

#[test]
fn zip_with_an_outer_compression_is_unsupported() {
    let zip = zip_with(&[("a.txt", b"alpha", 0)]);
    let error = open(zip, Format::Zip, Compression::Codec(Codec::Gzip)).err().unwrap();
    assert_eq!(error.kind(), ErrorKind::Unsupported);
}

#[test]
fn zip_reports_modified_time_mode_and_verifies_checksums() {
    let zip = zip_with(&[("a.txt", b"alpha", 1_700_000_000), ("b.txt", b"bravo", 1_600_000_000)]);
    let mut archive = open(zip.clone(), Format::Zip, Compression::Plain).unwrap();
    let listing = list(&mut archive).unwrap();
    assert_eq!(listing.entries[0].modified, Some(1_700_000_000));
    assert_eq!(listing.entries[1].modified, Some(1_600_000_000));
    assert_eq!(listing.entries[0].mode, Some(0o640));
    assert!(!listing.completion.contains(Completion::CHECKSUM_VERIFIED));

    let mut archive = open(zip, Format::Zip, Compression::Plain).unwrap();
    read_all(&mut archive).unwrap();
    assert!(archive.finish(true).unwrap().contains(Completion::CHECKSUM_VERIFIED));
}

#[test]
fn zip_detects_a_corrupted_body_when_it_is_read() {
    let body = b"alpha beta gamma delta epsilon zeta eta theta".repeat(4);
    let zip = {
        let mut output = Vec::new();
        let mut archive = rawzip::ZipArchiveWriter::new(&mut output);
        let (mut file, config) = archive
            .new_file("stored.txt")
            .compression_method(rawzip::CompressionMethod::STORE)
            .start()
            .unwrap();
        let mut writer = config.wrap(&mut file);
        writer.write_all(&body).unwrap();
        let (_, description) = writer.finish().unwrap();
        file.finish(description).unwrap();
        archive.finish().unwrap();
        output
    };
    let mut broken = zip.clone();
    let at = broken.windows(5).position(|window| window == b"alpha").unwrap();
    broken[at] = b'A';
    let mut archive = open(broken, Format::Zip, Compression::Plain).unwrap();
    assert_eq!(kind_of(read_all(&mut archive)), ErrorKind::Corrupt);
    let mut archive = open(zip, Format::Zip, Compression::Plain).unwrap();
    assert!(read_all(&mut archive).is_ok());
}

#[test]
fn cpio_lists_kinds_links_and_requires_a_trailer() {
    let mut bytes = Vec::new();
    bytes.extend(cpio_entry(b"dir", 0o040_755, b"", 2, None));
    bytes.extend(cpio_entry(b"file", 0o100_644, b"data", 1, None));
    bytes.extend(cpio_entry(b"link", 0o120_777, b"file", 1, None));
    bytes.extend(cpio_entry(b"node", 0o020_600, b"", 1, None));
    let without_trailer = bytes.clone();
    bytes.extend(cpio_trailer());
    let mut archive = open(pad512(bytes), Format::Cpio, Compression::Plain).unwrap();
    let listing = list(&mut archive).unwrap();
    let kinds: Vec<_> = listing.entries.iter().map(|entry| entry.kind).collect();
    assert_eq!(
        kinds,
        vec![EntryKind::Directory, EntryKind::File, EntryKind::Symlink, EntryKind::CharDevice]
    );
    assert_eq!(listing.entries[2].link.as_deref(), Some(&b"file"[..]));
    assert_eq!(listing.entries[0].link_count, Some(2));
    assert!(listing.completion.contains(Completion::STRUCTURE_COMPLETE));

    let mut archive = open(without_trailer, Format::Cpio, Compression::Plain).unwrap();
    assert_eq!(kind_of(list(&mut archive)), ErrorKind::Truncated);
}

#[test]
fn cpio_truncated_at_every_length_is_never_reported_complete() {
    let bytes = cpio_with(&[(b"a", b"alpha"), (b"bb", b"bravo!!")]);
    let end = bytes.iter().rposition(|byte| *byte != 0).unwrap() + 1;
    for length in 0..end {
        let mut archive = open(bytes[..length].to_vec(), Format::Cpio, Compression::Plain).unwrap();
        let result = list(&mut archive);
        assert!(result.is_err(), "length {length} was accepted");
    }
}

#[test]
fn cpio_oversized_name_and_bad_numbers_are_rejected() {
    let mut header = cpio_entry(b"x", 0o100_644, b"", 1, None);
    header[94..102].copy_from_slice(b"00ffffff");
    let mut archive = open(header, Format::Cpio, Compression::Plain).unwrap();
    assert_eq!(kind_of(list(&mut archive)), ErrorKind::Limit);

    let mut header = cpio_entry(b"x", 0o100_644, b"", 1, None);
    header[14..22].copy_from_slice(b"zzzzzzzz");
    let mut archive = open(header, Format::Cpio, Compression::Plain).unwrap();
    assert_eq!(kind_of(list(&mut archive)), ErrorKind::Corrupt);

    let mut archive = open(b"070707".repeat(30), Format::Cpio, Compression::Plain).unwrap();
    assert_eq!(kind_of(list(&mut archive)), ErrorKind::Unsupported);
}

#[test]
fn cpio_crc_format_verifies_each_body() {
    let sum = |bytes: &[u8]| bytes.iter().map(|byte| u32::from(*byte)).sum::<u32>();
    let mut good = cpio_entry(b"f", 0o100_644, b"abc", 1, Some(sum(b"abc")));
    good.extend(cpio_trailer());
    let mut archive = open(pad512(good), Format::Cpio, Compression::Plain).unwrap();
    read_all(&mut archive).unwrap();
    assert!(archive.finish(true).unwrap().contains(Completion::CHECKSUM_VERIFIED));

    let mut bad = cpio_entry(b"f", 0o100_644, b"abc", 1, Some(sum(b"abd")));
    bad.extend(cpio_trailer());
    let mut archive = open(pad512(bad), Format::Cpio, Compression::Plain).unwrap();
    assert_eq!(kind_of(read_all(&mut archive)), ErrorKind::Corrupt);
}

#[test]
fn compressed_archives_are_detected_by_magic_and_by_name() {
    let tar = tar_with(&[(b"a", b"alpha")]);
    for codec in Codec::ALL {
        let mut packed = Vec::new();
        let encode = Encode::new(codec, None).unwrap();
        let mut encoder = Encoder::new(Some(&encode), &mut packed).unwrap();
        encoder.write_all(&tar).unwrap();
        encoder.finish().unwrap();

        let mut archive = open(packed.clone(), Format::Tar, Compression::Codec(codec)).unwrap();
        assert_eq!(list(&mut archive).unwrap().entries.len(), 1, "{codec:?} named");
        if codec != Codec::Lzma {
            let mut archive = open(packed.clone(), Format::Tar, Compression::Auto).unwrap();
            assert_eq!(list(&mut archive).unwrap().entries.len(), 1, "{codec:?} magic");
        }
        let mut source = Source::from_bytes(packed);
        source.name = Some(format!("archive.tar.{}", suffix(codec)).into_bytes());
        let mut archive = Archive::open(source, Format::Tar, Compression::Auto, budget()).unwrap();
        let listing = list(&mut archive).unwrap();
        assert!(listing.completion.contains(Completion::OUTER_COMPLETE), "{codec:?} name");
    }
    let _ = NO_NAME;
}

fn suffix(codec: Codec) -> &'static str {
    match codec {
        Codec::Gzip => "gz",
        Codec::Bzip2 => "bz2",
        Codec::Xz => "xz",
        Codec::Lzma => "lzma",
        Codec::Zstd => "zst",
    }
}

#[test]
fn a_damaged_or_cut_compression_trailer_is_never_accepted() {
    let tar = tar_with(&[(b"a", &[b'x'; 5000])]);
    for codec in Codec::ALL {
        if codec == Codec::Lzma {
            continue;
        }
        let mut packed = Vec::new();
        let encode = Encode::new(codec, None).unwrap();
        let mut encoder = Encoder::new(Some(&encode), &mut packed).unwrap();
        encoder.write_all(&tar).unwrap();
        encoder.finish().unwrap();

        for cut in [1_usize, 2, 4, 8] {
            let short = packed[..packed.len() - cut].to_vec();
            let mut archive = open(short, Format::Tar, Compression::Codec(codec)).unwrap();
            let kind = kind_of(drain(&mut archive));
            assert!(
                matches!(kind, ErrorKind::Truncated | ErrorKind::Corrupt),
                "{codec:?} cut {cut}: {kind:?}"
            );
        }
        let mut flipped = packed.clone();
        let last = flipped.len() - 6;
        flipped[last] ^= 0xff;
        let mut archive = open(flipped, Format::Tar, Compression::Codec(codec)).unwrap();
        assert!(drain(&mut archive).is_err(), "{codec:?} flipped trailer byte");
    }
}

#[test]
fn encoder_levels_are_validated_per_algorithm() {
    for (codec, low, high) in [
        (Codec::Gzip, 0, 9),
        (Codec::Bzip2, 1, 9),
        (Codec::Xz, 0, 9),
        (Codec::Lzma, 0, 9),
        (Codec::Zstd, -131_072, 22),
    ] {
        assert!(Encode::new(codec, Some(low)).is_ok(), "{codec:?} low");
        assert!(Encode::new(codec, Some(high)).is_ok(), "{codec:?} high");
        assert_eq!(Encode::new(codec, Some(low - 1)).unwrap_err().kind(), ErrorKind::Options);
        assert_eq!(Encode::new(codec, Some(high + 1)).unwrap_err().kind(), ErrorKind::Options);
    }
}

struct FailingSink {
    fail_after: usize,
    written: usize,
}

impl Write for FailingSink {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        if self.written + buffer.len() > self.fail_after {
            return Err(std::io::Error::other("disk full"));
        }
        self.written += buffer.len();
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_sink_that_fails_before_or_at_finish_fails_the_encoder() {
    for codec in Codec::ALL {
        let encode = Encode::new(codec, None).unwrap();
        let sink = FailingSink { fail_after: 8, written: 0 };
        let outcome = Encoder::new(Some(&encode), sink).and_then(|mut encoder| {
            encoder
                .write_all(b"small payload")
                .map_err(|error| xsh_archive::Error::from_host(&error))?;
            encoder.finish().map(|_| ())
        });
        let error = outcome.expect_err(&format!("{codec:?} should report the full disk"));
        assert_eq!(error.kind(), ErrorKind::Io, "{codec:?}: {error}");
    }
    let sink = FailingSink { fail_after: 0, written: 0 };
    let mut plain = Encoder::new(None, std::io::BufWriter::new(sink)).unwrap();
    plain.write_all(b"buffered").unwrap();
    assert_eq!(plain.finish().err().unwrap().kind(), ErrorKind::Io);
}
