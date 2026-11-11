use std::fs;
use std::io;
use std::os::fd::AsFd;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;

use xsh_root::transfer::{
    Action, Collision, Operands, Operation, PolicyOptions, PolicyRequest, Progress, Target, decide,
    preflight,
};

use super::support::{Fixture, observe_path, open_directory, publish, set_times};

fn exact(source: &str, destination: &str) -> Operands {
    Operands::Exact {
        source: source.into(),
        destination: destination.into(),
    }
}

fn plan(
    cwd: &Path,
    operands: &Operands,
    options: PolicyOptions,
) -> xsh_root::transfer::Preflight {
    let directory = open_directory(cwd).expect("open fixture cwd");
    preflight(
        directory.as_fd(),
        &PolicyRequest {
            operation: Operation::Copy,
            operands,
            options,
            home: None,
        },
    )
    .expect("preflight operands")
}

fn publish_plan(
    cwd: &Path,
    operand: &xsh_root::transfer::PlannedOperand,
    progress: &mut Progress,
) {
    let source_name = operand
        .source
        .file_name()
        .expect("planned source has a final name");
    let destination_parent = operand
        .destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    publish(
        &cwd.join(
            operand
                .source
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new(".")),
        ),
        source_name,
        &cwd.join(destination_parent),
        &operand.destination_name,
        operand.action,
        operand.source_observed,
        operand.destination_observed,
        progress,
    )
    .expect("publish planned operand");
}

#[test]
fn replace_transfers_same_size_same_mtime_drift_while_no_clobber_skips_it() {
    let fixture = Fixture::new();
    let cwd = fixture.path("work");
    fs::write(cwd.join("replace-source"), b"new bytes").expect("write source");
    fs::write(cwd.join("replace-target"), b"old bytes").expect("write target");
    fs::write(cwd.join("skip-source"), b"new bytes").expect("write source");
    fs::write(cwd.join("skip-target"), b"old bytes").expect("write target");
    for name in ["replace-source", "replace-target", "skip-source", "skip-target"] {
        set_times(&cwd.join(name), 1_700_000_500, 321_000_000);
    }
    for pair in [["replace-source", "replace-target"], ["skip-source", "skip-target"]] {
        let source = fs::metadata(cwd.join(pair[0])).expect("source metadata");
        let destination = fs::metadata(cwd.join(pair[1])).expect("target metadata");
        assert_eq!(source.len(), destination.len());
        assert_eq!(
            (source.mtime(), source.mtime_nsec()),
            (destination.mtime(), destination.mtime_nsec())
        );
    }

    let replace = plan(
        &cwd,
        &exact("replace-source", "replace-target"),
        PolicyOptions {
            collision: Collision::Replace,
            ..PolicyOptions::default()
        },
    );
    assert_eq!(replace.operands[0].action, Action::Replace);
    let mut progress = Progress::default();
    publish_plan(&cwd, &replace.operands[0], &mut progress);
    assert_eq!(
        fs::read(cwd.join("replace-target")).expect("read target"),
        b"new bytes"
    );

    let skip = plan(
        &cwd,
        &exact("skip-source", "skip-target"),
        PolicyOptions {
            collision: Collision::Skip,
            ..PolicyOptions::default()
        },
    );
    assert_eq!(skip.operands[0].action, Action::Skip);
    assert_eq!(
        fs::read(cwd.join("skip-target")).expect("read target"),
        b"old bytes"
    );
    assert_eq!(
        fs::read(cwd.join("skip-source")).expect("read source"),
        b"new bytes"
    );
}

#[test]
fn destination_created_after_preflight_survives_for_later_into_operand() {
    for late_symlink in [false, true] {
        let fixture = Fixture::new();
        let cwd = fixture.path("work");
        fs::create_dir(cwd.join("left")).expect("create left");
        fs::create_dir(cwd.join("right")).expect("create right");
        fs::create_dir(cwd.join("out")).expect("create out");
        fs::write(cwd.join("left/first"), b"first source").expect("write first");
        fs::write(cwd.join("right/second"), b"second source").expect("write second");
        fs::write(cwd.join("referent"), b"late destination").expect("write referent");
        let operands = Operands::Into {
            directory: "out".into(),
            first: "left/first".into(),
            rest: vec!["right/second".into()],
        };
        let plan = plan(&cwd, &operands, PolicyOptions::default());
        assert_eq!(plan.operands.len(), 2);
        assert!(plan.operands.iter().all(|operand| operand.action == Action::Transfer));

        let mut progress = Progress::default();
        publish_plan(&cwd, &plan.operands[0], &mut progress);
        if late_symlink {
            std::os::unix::fs::symlink("../referent", cwd.join("out/second"))
                .expect("inject late symlink");
        } else {
            fs::write(cwd.join("out/second"), b"late destination").expect("inject late file");
        }
        let error = publish_plan_error(&cwd, &plan.operands[1], &mut progress);
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);

        assert_eq!(
            fs::read(cwd.join("out/first")).expect("read first output"),
            b"first source"
        );
        assert_eq!(
            fs::read(cwd.join("out/second")).expect("read late output"),
            b"late destination"
        );
        assert_eq!(
            fs::read(cwd.join("referent")).expect("read referent"),
            b"late destination"
        );
        if late_symlink {
            assert_eq!(
                fs::read_link(cwd.join("out/second")).expect("read late link"),
                Path::new("../referent")
            );
        }
        assert_eq!(
            fs::read(cwd.join("right/second")).expect("read second source"),
            b"second source"
        );
        assert_eq!(
            fs::read_dir(cwd.join("out")).expect("read out").count(),
            2
        );
    }
}

fn publish_plan_error(
    cwd: &Path,
    operand: &xsh_root::transfer::PlannedOperand,
    progress: &mut Progress,
) -> io::Error {
    let source_name = operand
        .source
        .file_name()
        .expect("planned source has a final name");
    let source_parent = cwd.join(
        operand
            .source
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new(".")),
    );
    let destination_parent = cwd.join(
        operand
            .destination
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new(".")),
    );
    publish(
        &source_parent,
        source_name,
        &destination_parent,
        &operand.destination_name,
        operand.action,
        operand.source_observed,
        operand.destination_observed,
        progress,
    )
    .expect_err("the late destination must stop publication")
}

#[test]
fn concurrent_readers_observe_complete_old_or_new_replacement_bytes() {
    let fixture = Fixture::new();
    let cwd = fixture.path("work");
    let source = cwd.join("source");
    let destination = cwd.join("destination");
    let old = Arc::new(vec![b'o'; 16 * 1024 * 1024]);
    let new = Arc::new(
        (0..16 * 1024 * 1024)
            .map(|index| ((index * 29 + 17) % 251) as u8)
            .collect::<Vec<_>>(),
    );
    fs::write(&source, new.as_slice()).expect("write source");
    fs::write(&destination, old.as_slice()).expect("write destination");
    let plan = plan(
        &cwd,
        &exact("source", "destination"),
        PolicyOptions {
            collision: Collision::Replace,
            ..PolicyOptions::default()
        },
    );
    assert_eq!(plan.operands[0].action, Action::Replace);

    let done = Arc::new(AtomicBool::new(false));
    let invalid = Arc::new(AtomicBool::new(false));
    let snapshots = Arc::new(AtomicUsize::new(0));
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let (first_snapshot, first_snapshot_received) = mpsc::sync_channel(0);
    let reader_path = destination.clone();
    let reader_old = Arc::clone(&old);
    let reader_new = Arc::clone(&new);
    let reader_done = Arc::clone(&done);
    let reader_invalid = Arc::clone(&invalid);
    let reader_snapshots = Arc::clone(&snapshots);
    let reader_barrier = Arc::clone(&barrier);
    let reader = thread::spawn(move || {
        reader_barrier.wait();
        let mut first = true;
        while !reader_done.load(Ordering::Acquire) {
            match fs::read(&reader_path) {
                Ok(bytes)
                    if bytes.as_slice() == reader_old.as_slice()
                        || bytes.as_slice() == reader_new.as_slice() =>
                {
                    reader_snapshots.fetch_add(1, Ordering::Relaxed);
                }
                _ => reader_invalid.store(true, Ordering::Release),
            }
            if first {
                first_snapshot.send(()).expect("signal first snapshot");
                first = false;
            }
            thread::yield_now();
        }
    });
    barrier.wait();
    first_snapshot_received.recv().expect("reader started");

    let mut progress = Progress::default();
    publish_plan(&cwd, &plan.operands[0], &mut progress);
    done.store(true, Ordering::Release);
    reader.join().expect("reader finished");

    assert!(
        !invalid.load(Ordering::Acquire),
        "reader observed a missing or partial destination"
    );
    assert!(snapshots.load(Ordering::Relaxed) > 0);
    assert_eq!(
        fs::read(&destination).expect("read destination"),
        new.as_slice()
    );
}

#[test]
fn policy_decisions_match_the_collision_matrix_without_mutation() {
    let fixture = Fixture::new();
    let cwd = fixture.path("work");
    fs::write(cwd.join("source"), b"source").expect("write source");
    fs::write(cwd.join("target"), b"target").expect("write target");
    let source_observation = observe_path(&cwd.join("source"));
    let target_observation = observe_path(&cwd.join("target"));

    for (collision, expected) in [
        (Collision::Error, None),
        (Collision::Skip, Some(Action::Skip)),
        (Collision::Replace, Some(Action::Replace)),
    ] {
        let decision = decide(
            source_observation,
            Target::Existing(target_observation),
            &PolicyOptions {
                collision,
                merge: false,
                dry_run: false,
            },
        );
        match expected {
            Some(expected) => assert_eq!(decision.expect("decision"), expected),
            None => assert_eq!(
                decision.expect_err("collision must be refused").kind(),
                io::ErrorKind::AlreadyExists
            ),
        }
    }
    assert_eq!(
        decide(
            source_observation,
            Target::Absent,
            &PolicyOptions::default()
        )
        .expect("absent destination transfers"),
        Action::Transfer
    );
    assert_eq!(
        decide(
            source_observation,
            Target::SameEntry,
            &PolicyOptions::default()
        )
        .expect("same entry is a no-op"),
        Action::SameEntry
    );
}
