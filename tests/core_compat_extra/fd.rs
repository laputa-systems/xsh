use super::{applet, Running};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

fn input_pipe(bytes: &[u8]) -> OwnedFd {
    let mut descriptors = [0; 2];
    assert_eq!(unsafe { libc::pipe(descriptors.as_mut_ptr()) }, 0);
    let reader = unsafe { <OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(descriptors[0]) };
    let writer = unsafe { <OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(descriptors[1]) };
    for descriptor in descriptors {
        assert_eq!(unsafe { libc::fcntl(descriptor, libc::F_SETFD, libc::FD_CLOEXEC) }, 0);
    }
    // Fixtures are smaller than the pipe capacity and have no live producer.
    let mut writer = File::from(writer);
    writer.write_all(bytes).unwrap();
    drop(writer);
    reader
}

// These cases need inherited open file descriptions: reopening a capture path
// changes either the shared cursor or whether dd can seek its output.
// origin: uutils test_dd::test_bytes_oseek_bytes_trunc_oflag
#[test]
fn test_bytes_oseek_bytes_trunc_oflag() {
    let directory = tempfile::tempdir().unwrap();
    let mut command = applet("dd", directory.path());
    command.args(["oseek=8", "oflag=seek_bytes", "bs=2", "count=0"])
        .stdin(input_pipe(b"abcdefghijklm"));
    let output = Running::spawn(&mut command).finish();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    // GNU rejects seeking an inherited pipe even for a zero-length transfer.
    // Only the measured transfer duration is variable.
    let prefix = "dd: 'standard output': cannot seek: Illegal seek\n0+0 records in\n0+0 records out\n0 bytes copied, ";
    let duration = stderr.strip_prefix(prefix).expect("seek error and zero-transfer records")
        .strip_suffix(" s, 0.0 kB/s\n").expect("zero-transfer rate");
    assert!(duration.parse::<f64>().unwrap() >= 0.0);
}

// origin: uutils test_dd::test_multiple_processes_reading_stdin
#[test]
fn test_multiple_processes_reading_stdin() {
    let directory = tempfile::tempdir().unwrap();
    let reader = input_pipe(b"abcdef\n");
    let mut first = applet("dd", directory.path());
    first.args(["bs=1", "skip=3", "count=0"])
        .stdin(reader.try_clone().unwrap()).stderr(Stdio::null());
    let output = Running::spawn(&mut first).finish();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    let mut second = applet("dd", directory.path());
    second.stdin(reader).stderr(Stdio::null());
    let output = Running::spawn(&mut second).finish();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"def\n");
    assert!(output.stderr.is_empty());
}

// origin: uutils test_dd::test_stdin_stdout_not_rewound_even_when_connected_to_seekable_file
#[test]
fn test_stdin_stdout_not_rewound_even_when_connected_to_seekable_file() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("in"), b"abcde").unwrap();
    let input = File::open(directory.path().join("in")).unwrap();
    let output = OpenOptions::new().create(true).write(true).truncate(true)
        .open(directory.path().join("out")).unwrap();
    let error = OpenOptions::new().create(true).write(true).truncate(true)
        .open(directory.path().join("err")).unwrap();
    let mut first = applet("dd", directory.path());
    first.args(["bs=1", "skip=1", "count=1"])
        .stdin(input.try_clone().unwrap()).stdout(output.try_clone().unwrap())
        .stderr(error.try_clone().unwrap());
    assert!(Running::spawn(&mut first).finish().status.success());
    let mut second = applet("dd", directory.path());
    second.args(["bs=1", "skip=1"]).stdin(input).stdout(output).stderr(error);
    assert!(Running::spawn(&mut second).finish().status.success());
    assert_eq!(std::fs::read(directory.path().join("out")).unwrap(), b"bde");
}

// Ignore SIGXFSZ only in the child so parallel tests keep their own signal
// dispositions. The inherited ignore turns the file cap into a write error.
#[cfg(not(target_vendor = "apple"))]
fn cap_file(command: &mut Command, bytes: libc::rlim_t) {
    unsafe {
        command.pre_exec(move || {
            if libc::signal(libc::SIGXFSZ, libc::SIG_IGN) == libc::SIG_ERR {
                return Err(std::io::Error::last_os_error());
            }
            let limit = libc::rlimit { rlim_cur: bytes, rlim_max: bytes };
            if libc::setrlimit(libc::RLIMIT_FSIZE, &limit) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

// origin: uutils test_dd::test_stats_are_reported_when_a_write_fails
#[test]
#[cfg(not(target_vendor = "apple"))]
fn test_stats_are_reported_when_a_write_fails() {
    let directory = tempfile::tempdir().unwrap();
    let mut command = applet("dd", directory.path());
    command.args(["if=/dev/zero", "of=capped.bin", "bs=512K", "count=3"]);
    cap_file(&mut command, 786432);
    let output = Running::spawn(&mut command).finish();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("1+1 records out"), "{stderr}");
    assert!(stderr.contains("786432 bytes"), "{stderr}");
    assert_eq!(std::fs::metadata(directory.path().join("capped.bin")).unwrap().len(), 786432);
}

// origin: uutils test_dd::test_block_stats_are_reported_when_a_write_fails
#[test]
#[cfg(not(target_vendor = "apple"))]
fn test_block_stats_are_reported_when_a_write_fails() {
    let directory = tempfile::tempdir().unwrap();
    let mut command = applet("dd", directory.path());
    command.args(["conv=block", "cbs=1M", "obs=64K", "of=capped.bin"])
        .stdin(input_pipe(b"x\n"));
    cap_file(&mut command, 204800);
    let output = Running::spawn(&mut command).finish();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("3+1 records out"), "{stderr}");
    assert!(stderr.contains("204800 bytes"), "{stderr}");
    assert_eq!(std::fs::metadata(directory.path().join("capped.bin")).unwrap().len(), 204800);
}

// origin: uutils test_dd::test_large_bs_does_not_fault_in_copy_buffer
#[test]
#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
fn test_large_bs_does_not_fault_in_copy_buffer() {
    let directory = tempfile::tempdir().unwrap();
    let peak_rss = |block_size| {
        let mut command = applet("dd", directory.path());
        command.args([block_size, "if=/dev/null", "of=/dev/null"]).stderr(Stdio::null());
        let (output, usage) = Running::spawn(&mut command).finish_with_usage();
        assert!(output.status.success(), "dd {block_size}: {:?}", output.status);
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
        usage.ru_maxrss
    };
    // Four GiB of virtual address space must not become resident for an empty
    // transfer. Verification runs in a container capped at one GiB of memory.
    let growth = peak_rss("bs=4G") - peak_rss("bs=4K");
    assert!(growth < 64 << 10, "a 4 GiB copy buffer raised peak RSS by {growth} KiB");
}

// origin: uutils test_od::test_read_bytes
#[test]
fn test_read_bytes() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("input"), b"abcdefghijklmnopqrstuvwxyz\n12345678").unwrap();
    let input = File::open(directory.path().join("input")).unwrap();
    let mut shadow = input.try_clone().unwrap();
    let mut command = applet("od", directory.path());
    command.args(["--endian=little", "--read-bytes=27"]).stdin(input);
    let output = Running::spawn(&mut command).finish();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(output.stdout, b"0000000 061141 062143 063145 064147 065151 066153 067155 070157\n0000020 071161 072163 073165 074167 075171 000012\n0000033\n");
    let mut remainder = Vec::new();
    assert_eq!(shadow.read_to_end(&mut remainder).unwrap(), 8);
    assert_eq!(remainder, b"12345678");
}
