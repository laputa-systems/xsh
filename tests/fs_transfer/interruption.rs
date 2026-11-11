use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{BufRead, Read, Write};
use std::os::fd::AsFd;
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use xsh_root::transfer::{
    Action, Boundary, Cancellation, Progress, PublishRequest, Session, observe_entry, publish_tree,
};

use super::support::{Fixture, open_directory};

const FILES: u32 = 64;
const BYTES_PER_FILE: usize = 512 * 1024;
const MARKER_NAME: &str = ".owned-copy-marker";
const MARKER_CONTENT: &[u8] = b"xsh-fs-private-stage-fixture-v1\0";

fn payload(index: u32) -> Vec<u8> {
    let mut bytes = vec![0; BYTES_PER_FILE];
    let seed = index.to_le_bytes();
    for (offset, byte) in bytes.iter_mut().enumerate() {
        *byte = seed[offset % seed.len()] ^ (offset as u8).wrapping_mul(31);
    }
    bytes
}

fn build_source(root: &Path) {
    let source = root.join("source");
    fs::create_dir(&source).expect("create source");
    fs::write(source.join(MARKER_NAME), MARKER_CONTENT).expect("write marker");
    for index in 0..FILES {
        fs::write(source.join(format!("payload-{index:03}")), payload(index))
            .expect("write payload");
    }
}

fn read_named(parent: &Path, name: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    File::open(parent.join(name))
        .expect("open fixture entry")
        .read_to_end(&mut bytes)
        .expect("read fixture entry");
    bytes
}

fn verify_complete_copy(root: &Path) {
    let source = root.join("source");
    let destination = root.join("destination");
    assert_eq!(read_named(&destination, MARKER_NAME), MARKER_CONTENT);
    for index in 0..FILES {
        let name = format!("payload-{index:03}");
        assert_eq!(
            read_named(&destination, &name),
            read_named(&source, &name),
            "{name}"
        );
    }
}

fn staged_copy(root: &Path) -> Option<(PathBuf, PathBuf, Vec<u8>)> {
    let prefix = ".fdu-quarantine-v2-";
    for entry in fs::read_dir(root).ok()?.flatten() {
        if !entry.file_name().to_string_lossy().starts_with(prefix) {
            continue;
        }
        let stage = entry.path().join("0.entry");
        let Ok(children) = fs::read_dir(&stage) else {
            continue;
        };
        for child in children.flatten() {
            let Ok(metadata) = fs::symlink_metadata(child.path()) else {
                continue;
            };
            if metadata.file_type().is_file() && metadata.len() > 0 {
                let Ok(bytes) = fs::read(child.path()) else {
                    continue;
                };
                if !bytes.is_empty() {
                    return Some((stage, child.path(), bytes));
                }
            }
        }
    }
    None
}

fn start_child(root: &Path) -> Child {
    Command::new(std::env::current_exe().expect("current test binary"))
        .args([
            "--exact",
            "interruption::child_copy_for_sigkill",
            "--ignored",
            "--nocapture",
        ])
        .env("XSH_FS_KILL_FIXTURE", root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn copy child")
}

#[test]
fn killing_an_inflight_copy_never_exposes_a_partial_final_file() {
    let fixture = Fixture::new();
    let root = fixture.root();
    build_source(root);
    let mut child = start_child(root);
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut readiness = None;
    while Instant::now() < deadline {
        if let Some(staged) = staged_copy(root) {
            readiness = Some(staged);
            break;
        }
        if let Some(status) = child.try_wait().expect("poll child") {
            assert!(
                status.success(),
                "copy child exited before staging readiness: {status}"
            );
            verify_complete_copy(root);
            panic!("copy completed before the parent observed an in-flight staged file");
        }
        std::thread::yield_now();
    }
    let Some((stage, staged_file, staged_bytes)) = readiness else {
        if child.try_wait().expect("poll child").is_none() {
            let _ = child.kill();
            let _ = child.wait();
        }
        panic!("no nonempty staged file appeared within 20 seconds");
    };
    assert!(
        child.try_wait().expect("poll child").is_none(),
        "copy exited after staging readiness and before SIGKILL"
    );
    let source_name = staged_file.file_name().expect("staged file name");
    let source_bytes = fs::read(root.join("source").join(source_name)).expect("read source file");
    assert!(
        source_bytes.starts_with(&staged_bytes),
        "staging bytes do not match the source prefix"
    );

    child.kill().expect("kill copy child");
    let status = child.wait().expect("reap copy child");
    assert_eq!(status.signal(), Some(libc::SIGKILL));

    let source = root.join("source");
    assert_eq!(read_named(&source, MARKER_NAME), MARKER_CONTENT);
    for index in 0..FILES {
        let name = format!("payload-{index:03}");
        assert_eq!(read_named(&source, &name), payload(index), "source {name} was not retained");
    }
    assert_eq!(read_named(&source, source_name.to_str().expect("staged name")), source_bytes);
    if root.join("destination").exists() {
        verify_complete_copy(root);
    }

    let metadata = fs::symlink_metadata(&stage).expect("staged metadata");
    assert!(metadata.file_type().is_dir());
    assert_eq!(metadata.uid(), rustix::process::geteuid().as_raw());
    assert_eq!(metadata.mode() & 0o777, 0o700);
    for entry in fs::read_dir(&stage).expect("read staged entries") {
        let entry = entry.expect("staged entry");
        let entry_metadata = fs::symlink_metadata(entry.path()).expect("staged entry metadata");
        assert!(
            entry_metadata.file_type().is_file(),
            "unexpected staged object: {:?}",
            entry.file_name()
        );
        assert_eq!(
            entry_metadata.uid(),
            rustix::process::geteuid().as_raw()
        );
        if entry.file_name() == MARKER_NAME {
            assert_eq!(fs::read(entry.path()).expect("read staged marker"), MARKER_CONTENT);
        }
    }
}

fn tree_contains(root: &Path, needle: &[u8]) -> bool {
    let Ok(entries) = fs::read_dir(root) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_dir() {
            if tree_contains(&path, needle) {
                return true;
            }
        } else if metadata.file_type().is_file()
            && fs::read(&path).is_ok_and(|bytes| bytes == needle)
        {
            return true;
        }
    }
    false
}

#[test]
fn killing_publication_at_each_journal_step_leaves_a_complete_destination() {
    let old = vec![b'o'; 64 * 1024];
    let new = (0..64 * 1024)
        .map(|index| ((index * 37 + 11) % 251) as u8)
        .collect::<Vec<_>>();
    let cases = [
        (Boundary::SessionStarted, false),
        (Boundary::StageRegistered, false),
        (Boundary::TransferComplete, false),
        (Boundary::BeforePublish, false),
        (Boundary::IntentPublished, false),
        (Boundary::Published, true),
        (Boundary::BeforeCapture, true),
        (Boundary::Captured, true),
        (Boundary::CaptureParentSynced, true),
        (Boundary::CaptureDirectorySynced, true),
        (Boundary::BeforeUnlink, true),
        (Boundary::Unlinked, true),
        (Boundary::RemovalDirectorySynced, true),
        (Boundary::BeforeParentSync, true),
        (Boundary::ParentSynced, true),
        (Boundary::IntentRemoved, true),
        (Boundary::IntentRemovalSynced, true),
        (Boundary::StageCleared, true),
        (Boundary::Finished, true),
    ];
    for (boundary, new_visible) in cases {
        let fixture = Fixture::new();
        let root = fixture.root();
        fs::write(root.join("source"), &new).expect("write source file");
        fs::write(root.join("destination"), &old).expect("write destination file");
        let mut child = Command::new(std::env::current_exe().expect("current test binary"))
            .args([
                "--exact",
                "interruption::child_replace_at_boundary",
                "--ignored",
                "--nocapture",
            ])
            .env("XSH_PUBLICATION_CRASH_ROOT", root)
            .env("XSH_PUBLICATION_CRASH_BOUNDARY", format!("{boundary:?}"))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn publication child");
        wait_ready(&mut child, boundary);
        child.kill().expect("kill publication child");
        assert!(!child.wait().expect("reap publication child").success());

        let root_file = File::open(root).expect("open publication root");
        let _ = Session::check_recovery(root_file.as_fd());

        let destination = fs::read(root.join("destination")).expect("read destination");
        if new_visible {
            assert_eq!(destination, new, "{boundary:?}");
        } else {
            assert_eq!(destination, old, "{boundary:?}");
        }
        assert_eq!(fs::read(root.join("source")).expect("read source"), new);

        let prior_removed = matches!(
            boundary,
            Boundary::Unlinked
                | Boundary::RemovalDirectorySynced
                | Boundary::BeforeParentSync
                | Boundary::ParentSynced
                | Boundary::IntentRemoved
                | Boundary::IntentRemovalSynced
                | Boundary::StageCleared
                | Boundary::Finished
        );
        if !prior_removed {
            assert!(
                destination == old.as_slice() || tree_contains(root, &old),
                "the prior destination was lost at {boundary:?}"
            );
        }
    }
}

fn wait_ready(child: &mut Child, boundary: Boundary) {
    let stdout = child.stdout.take().expect("child stdout");
    let mut output = std::io::BufReader::new(stdout);
    let mut line = String::new();
    loop {
        assert_ne!(
            output.read_line(&mut line).expect("read child stdout"),
            0,
            "child ended before {boundary:?}"
        );
        if line.trim() == "ready" {
            return;
        }
        line.clear();
    }
}

#[test]
#[ignore = "subprocess fixture for process interruption"]
fn child_replace_at_boundary() {
    let Some(root) = std::env::var_os("XSH_PUBLICATION_CRASH_ROOT") else {
        return;
    };
    let boundary = std::env::var("XSH_PUBLICATION_CRASH_BOUNDARY").expect("crash boundary");
    let root = PathBuf::from(root);
    let parent = open_directory(&root).expect("open publication root");
    let expected_source = observe_entry(parent.as_fd(), OsStr::new("source")).expect("observe source");
    let expected_destination =
        observe_entry(parent.as_fd(), OsStr::new("destination")).expect("observe destination");
    let cancellation = Cancellation::default();
    let mut progress = Progress::default();
    publish_tree(
        PublishRequest {
            source_parent: parent.as_fd(),
            source_name: OsStr::new("source"),
            destination_parent: parent.as_fd(),
            destination_name: OsStr::new("destination"),
            expected_source,
            expected_destination: Some(expected_destination),
            action: Action::Replace,
            durable: false,
            cancellation: &cancellation,
            progress: &mut progress,
        },
        |point| {
            if format!("{point:?}") == boundary {
                println!("ready");
                std::io::stdout().flush().expect("flush readiness");
                loop {
                    std::thread::park();
                }
            }
            Ok(())
        },
    )
    .expect("publication completed unexpectedly");
    panic!("crash boundary was not reached");
}

#[test]
#[ignore = "subprocess fixture for process interruption"]
fn child_copy_for_sigkill() {
    let Some(root) = std::env::var_os("XSH_FS_KILL_FIXTURE") else {
        return;
    };
    let root = PathBuf::from(root);
    let source = open_directory(&root).expect("open fixture root");
    let destination = open_directory(&root).expect("open fixture root");
    let expected = observe_entry(source.as_fd(), OsStr::new("source")).expect("observe source");
    let cancellation = Cancellation::default();
    let mut progress = Progress::default();
    publish_tree(
        PublishRequest {
            source_parent: source.as_fd(),
            source_name: OsStr::new("source"),
            destination_parent: destination.as_fd(),
            destination_name: OsStr::new("destination"),
            expected_source: expected,
            expected_destination: None,
            action: Action::Transfer,
            durable: false,
            cancellation: &cancellation,
            progress: &mut progress,
        },
        |_| Ok(()),
    )
    .expect("publish child copy");
}
