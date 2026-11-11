use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom};
use std::os::fd::AsFd;
use std::os::unix::fs::FileExt;

use xsh_root::transfer::{Cancellation, CopyMethod, Progress, copy_data};

use super::support::Fixture;

#[test]
fn copy_rejects_short_sources_and_cancelled_work() {
    let fixture = Fixture::new();
    let source_path = fixture.path("source");
    let destination_path = fixture.path("destination");
    fs::write(&source_path, b"short").expect("write source");
    let source = File::open(&source_path).expect("open source");
    let destination = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&destination_path)
        .expect("create destination");
    let mut scratch = [0u8; 4];
    let error = copy_data(
        source.as_fd(),
        destination.as_fd(),
        6,
        &mut scratch,
        &Cancellation::default(),
    )
    .expect_err("a short source must be refused");
    assert_eq!(error.kind(), std::io::ErrorKind::UnexpectedEof);

    let cancellation = Cancellation::default();
    cancellation.cancel();
    let error = copy_data(
        source.as_fd(),
        destination.as_fd(),
        5,
        &mut scratch,
        &cancellation,
    )
    .expect_err("cancelled work must stop");
    assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
}

#[test]
fn copy_reports_logical_streamed_and_hole_bytes() {
    let fixture = Fixture::new();
    let source_path = fixture.path("sparse-source");
    let destination_path = fixture.path("sparse-destination");
    let length = 1024 * 1024_u64;
    let source = File::create(&source_path).expect("create source");
    source.set_len(length).expect("set source length");
    source
        .write_at(b"nonzero data", 64 * 1024 + 3)
        .expect("write source extent");
    drop(source);

    let source = File::open(&source_path).expect("open source");
    let destination = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&destination_path)
        .expect("create destination");
    let mut scratch = vec![0; 64 * 1024];
    let stats = copy_data(
        source.as_fd(),
        destination.as_fd(),
        length,
        &mut scratch,
        &Cancellation::default(),
    )
    .expect("copy sparse source");
    assert_eq!(stats.logical_bytes, length);
    assert_eq!(stats.streamed_bytes + stats.hole_bytes, length);
    if stats.method != CopyMethod::Clone {
        assert!(stats.hole_bytes > 0);
    }
    assert_eq!(
        fs::read(&destination_path).expect("read destination"),
        fs::read(&source_path).expect("read source")
    );

    let mut progress = Progress::default();
    progress.record(stats);
    progress.record(stats);
    assert_eq!(progress.logical_bytes, length * 2);
    assert_eq!(
        progress.streamed_bytes + progress.hole_bytes,
        progress.logical_bytes
    );
    if stats.method == CopyMethod::Clone {
        assert_eq!(progress.cloned_files, 2);
    }
}

#[test]
fn copy_restores_the_source_offset() {
    let fixture = Fixture::new();
    let source_path = fixture.path("source");
    let destination_path = fixture.path("destination");
    let expected = (0..512 * 1024)
        .map(|index| ((index * 31 + 7) % 251) as u8)
        .collect::<Vec<_>>();
    fs::write(&source_path, &expected).expect("write source");
    let mut source = File::open(&source_path).expect("open source");
    let destination = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&destination_path)
        .expect("create destination");
    let initial_offset = 19;
    source
        .seek(SeekFrom::Start(initial_offset))
        .expect("set source offset");
    let mut scratch = vec![0; 31 * 1024];
    let stats = copy_data(
        source.as_fd(),
        destination.as_fd(),
        expected.len() as u64,
        &mut scratch,
        &Cancellation::default(),
    )
    .expect("copy exact source length");
    assert_eq!(
        fs::read(&destination_path).expect("read destination"),
        expected
    );
    assert_eq!(
        source.stream_position().expect("read source offset"),
        initial_offset
    );
    assert_eq!(stats.logical_bytes, expected.len() as u64);
    assert_eq!(stats.streamed_bytes + stats.hole_bytes, stats.logical_bytes);
}
