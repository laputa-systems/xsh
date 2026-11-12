use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::fs::{self, File};
use std::io;
use std::os::fd::AsFd;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

use rustix::fs::IFlags;
use xsh_root::transfer::{
    Action, Cancellation, Collision, DeferredFlags, EngineOptions, Jobs, PolicyOptions, Progress,
    Progress as TransferProgress, Publication, PublishRequest, TreeTransfer, apply_deferred_flags,
    decide, mount_id, publish_tree, transfer_tree, worker_count,
};

use super::support::{Fixture, observe_path, open_directory, set_times};

#[test]
fn mount_identity_comes_from_statx_and_distinguishes_mounts() {
    let root = File::open("/").expect("open root filesystem");
    let root_mount = mount_id(root.as_fd()).expect("read root mount ID");
    assert_ne!(root_mount.mount_id, 0);
    let device = fs::metadata("/").expect("stat root").dev();
    assert_eq!(root_mount.device, device);
    let memory = match File::open("/dev/shm") {
        Ok(file) => file,
        Err(error) => {
            eprintln!("second mount fixture unavailable: {error}");
            return;
        }
    };
    let other = mount_id(memory.as_fd()).expect("read /dev/shm mount ID");
    if root_mount == other {
        eprintln!("no second mount identity available; /dev/shm shares {root_mount:?}");
    } else {
        eprintln!("mount IDs distinguish {root_mount:?} and {other:?}");
    }
}

#[test]
fn worker_count_refuses_an_explicit_request_above_the_budget() {
    let allowed = worker_count(&EngineOptions::default()).expect("default worker count");
    assert!((1..=32).contains(&allowed));
    assert_eq!(worker_count(&EngineOptions {
        jobs: Some(Jobs::new(1).expect("one worker")),
        ..EngineOptions::default()
    })
    .expect("one worker fits any budget"), 1);
    assert_eq!(
        Jobs::new(0).expect_err("zero workers must be refused").kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        Jobs::new(33).expect_err("too many workers must be refused").kind(),
        io::ErrorKind::InvalidInput
    );
    let explicit = Jobs::new(allowed).expect("admitted worker count");
    assert_eq!(
        worker_count(&EngineOptions {
            jobs: Some(explicit),
            ..EngineOptions::default()
        })
        .expect("an admitted explicit request is honored"),
        allowed
    );
}

#[test]
fn parallel_transfer_preserves_a_wide_tree_and_counts_every_entry() {
    let fixture = Fixture::new();
    let work = fixture.path("work");
    let source = work.join("source");
    fs::create_dir(&source).expect("create source");
    const FILES: u32 = 256;
    for index in 0..FILES {
        fs::write(source.join(format!("item-{index:04}")), payload(index)).expect("write leaf");
    }
    let mut progress = TransferProgress::default();
    let mut deferred = Vec::new();
    let options = EngineOptions {
        jobs: Some(Jobs::new(4).expect("four workers")),
        ..EngineOptions::default()
    };
    transfer_with(
        &work,
        "source",
        &work,
        "destination",
        options,
        &mut progress,
        &mut deferred,
    )
    .expect("parallel transfer");
    let destination = work.join("destination");
    for index in 0..FILES {
        assert_eq!(
            fs::read(destination.join(format!("item-{index:04}"))).expect("read leaf"),
            payload(index)
        );
    }
    assert_eq!(progress.failed_files, 0);
    assert_eq!(progress.discovered_files, u64::from(FILES) + 1);
    assert_eq!(progress.completed_files, u64::from(FILES) + 1);
    assert_eq!(progress.logical_bytes, u64::from(FILES) * 27);
    assert!(deferred.is_empty());
}

fn payload(index: u32) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(32);
    bytes.extend_from_slice(&index.to_le_bytes());
    bytes.extend_from_slice(b"\0xsh-transfer-content\xff\n");
    bytes
}

#[test]
fn cancellation_stops_workers_and_removes_the_partial_tree() {
    let fixture = Fixture::new();
    let work = fixture.path("work");
    let source = work.join("source");
    fs::create_dir(&source).expect("create source");
    for index in 0..64 {
        fs::write(source.join(format!("item-{index:03}")), vec![b'x'; 128 * 1024])
            .expect("write leaf");
    }
    let cancellation = Cancellation::default();
    cancellation.cancel();
    let source_parent = open_directory(&work).expect("open work");
    let mut progress = TransferProgress::default();
    let mut deferred = Vec::new();
    let error = transfer_tree(TreeTransfer {
        source_parent: source_parent.as_fd(),
        source_name: OsStr::new("source"),
        destination_parent: source_parent.as_fd(),
        destination_name: OsStr::new("destination"),
        engine: EngineOptions::default(),
        cancellation: &cancellation,
        progress: &mut progress,
        deferred_flags: &mut deferred,
    })
    .expect_err("a cancelled transfer must fail");
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert!(!work.join("destination").exists());
}

#[test]
fn cancellation_cleans_a_partial_replacement_destination() {
    let fixture = Fixture::new();
    let work = fixture.path("work");
    fs::write(work.join("source"), b"new bytes").expect("write source");
    fs::write(work.join("destination"), b"old bytes").expect("write destination");
    let plan = plan_exact(&work, "source", "destination", PolicyOptions {
        collision: Collision::Replace,
        ..PolicyOptions::default()
    });
    let cancellation = Cancellation::default();
    let source_parent = open_directory(&work).expect("open work");
    let mut progress = TransferProgress::default();
    publish_tree(
        PublishRequest {
            source_parent: source_parent.as_fd(),
            source_name: OsStr::new("source"),
            destination_parent: source_parent.as_fd(),
            destination_name: OsStr::new("destination"),
            expected_source: plan.operands[0].source_observed,
            expected_destination: plan.operands[0].destination_observed,
            action: Action::Replace,
            engine: EngineOptions::default(),
            cancellation: &cancellation,
            progress: &mut progress,
        },
        |_| Ok(()),
    )
    .expect("replacement");
    assert_eq!(
        fs::read(work.join("destination")).expect("read destination"),
        b"new bytes"
    );
    assert_eq!(
        fs::read(work.join("source")).expect("read source"),
        b"new bytes"
    );
}

fn plan_exact(
    cwd: &std::path::Path,
    source: &str,
    destination: &str,
    options: PolicyOptions,
) -> xsh_root::transfer::Preflight {
    let operands = xsh_root::transfer::Operands::Exact {
        source: source.into(),
        destination: destination.into(),
    };
    let cwd_fd = open_directory(cwd).expect("open cwd");
    xsh_root::transfer::preflight(
        cwd_fd.as_fd(),
        &xsh_root::transfer::PolicyRequest {
            operation: xsh_root::transfer::Operation::Copy,
            operands: &operands,
            options,
            home: None,
        },
    )
    .expect("preflight")
}

#[test]
fn dry_run_plans_skips_and_transfers_without_mutation() {
    let fixture = Fixture::new();
    let cwd = fixture.path("work");
    let source = cwd.join("source");
    let destination = cwd.join("destination");
    fs::create_dir_all(source.join("nested")).expect("create source");
    fs::create_dir(&destination).expect("create destination");
    fs::write(source.join("same"), b"same").expect("write same");
    fs::write(destination.join("same"), b"same").expect("write same");
    set_times(&source.join("same"), 1_700_000_000, 0);
    set_times(&destination.join("same"), 1_700_000_000, 0);
    fs::write(source.join("nested/new"), b"new").expect("write new");

    let before = snapshot(&cwd);
    let source_parent = open_directory(&cwd).expect("open cwd");
    let options = PolicyOptions {
        collision: Collision::Skip,
        merge: true,
        dry_run: true,
    };
    let mut plan = Vec::new();
    xsh_root::transfer::dry_run(
        source_parent.as_fd(),
        OsStr::new("source"),
        source_parent.as_fd(),
        OsStr::new("destination"),
        &options,
        &EngineOptions::default(),
        &Cancellation::default(),
        &mut plan,
    )
    .expect("plan dry run");
    assert_eq!(snapshot(&cwd), before, "a dry run must not mutate the namespace");
    let by_source = plan
        .iter()
        .map(|entry| (entry.source.clone(), entry.action))
        .collect::<Vec<_>>();
    assert_eq!(
        by_source,
        vec![
            (std::path::PathBuf::from("source"), Action::Merge),
            (std::path::PathBuf::from("source/nested"), Action::Transfer),
            (std::path::PathBuf::from("source/same"), Action::Skip),
            (std::path::PathBuf::from("source/nested/new"), Action::Transfer),
        ]
    );
}

fn snapshot(root: &std::path::Path) -> Vec<(std::path::PathBuf, u64, i64, i64)> {
    fn visit(
        root: &std::path::Path,
        path: &std::path::Path,
        entries: &mut Vec<(std::path::PathBuf, u64, i64, i64)>,
    ) {
        let metadata = fs::symlink_metadata(path).expect("stat snapshot entry");
        entries.push((
            path.strip_prefix(root).expect("strip root").to_path_buf(),
            metadata.ino(),
            metadata.mtime(),
            metadata.mtime_nsec(),
        ));
        if metadata.is_dir() {
            for entry in fs::read_dir(path).expect("read snapshot directory") {
                visit(root, &entry.expect("read snapshot entry").path(), entries);
            }
        }
    }
    let mut entries = Vec::new();
    visit(root, root, &mut entries);
    entries.sort();
    entries
}

#[test]
fn cross_file_systems_refuses_a_nested_mount_without_opt_in() {
    let fixture = Fixture::new();
    let work = fixture.path("work");
    let source = work.join("source");
    fs::create_dir(&source).expect("create source");
    fs::write(source.join("ordinary"), b"ordinary").expect("write ordinary file");
    let nested = source.join("nested");
    fs::create_dir(&nested).expect("create nested directory");
    let child_mount = match mount_id(open_directory(&nested).expect("open nested").as_fd()) {
        Ok(mount) => mount,
        Err(error) => {
            eprintln!("nested mount fixture unavailable: {error}");
            return;
        }
    };
    if child_mount == mount_id(open_directory(&source).expect("open source").as_fd()).unwrap() {
        let status = std::process::Command::new("unshare")
            .args(["--user", "--map-root-user", "--mount", "true"])
            .status();
        match status {
            Ok(status) if status.success() => {
                eprintln!("nested mount fixture is unavailable; userns mount is supported but not configured");
            }
            _ => eprintln!("nested mount fixture is unavailable; userns mounts are not permitted here"),
        }
        return;
    }
    let mut progress = TransferProgress::default();
    let mut deferred = Vec::new();
    let error = transfer_with(
        &work,
        "source",
        &work,
        "destination",
        EngineOptions::default(),
        &mut progress,
        &mut deferred,
    )
    .expect_err("a nested mount must be refused by default");
    assert!(
        error
            .to_string()
            .contains("nested source mount requires --cross-file-systems"),
        "{error}"
    );
    assert!(!work.join("destination").exists());
}

#[test]
fn blocking_flags_are_deferred_until_after_publication() {
    let fixture = Fixture::new();
    let work = fixture.path("work");
    let source = work.join("source");
    fs::create_dir(&source).expect("create source");
    fs::write(source.join("file"), b"flag payload").expect("write file");
    let Some(flag) = settable_blocking_flag(&source.join("file")) else {
        eprintln!(
            "inode flags are unavailable on the fixture filesystem; a staged transfer cannot exercise deferred application here"
        );
        return;
    };
    set_flag(&source.join("file"), flag, true).expect("set source flag");

    let staging = work.join("staged");
    let mut progress = TransferProgress::default();
    let mut deferred = Vec::new();
    transfer_with(
        &work,
        "source",
        &work,
        "staged",
        EngineOptions::default(),
        &mut progress,
        &mut deferred,
    )
    .expect("transfer flagged source");

    assert_eq!(
        read_flags(&staging.join("file")) & flag,
        0,
        "a staged object must not carry a blocking flag before publication"
    );
    let entry = deferred
        .iter()
        .find(|entry| entry.relative == std::path::Path::new("file"))
        .expect("the blocking flag must be recorded as deferred");
    assert_eq!(entry.flags & flag, flag);
    let root_entry = deferred
        .iter()
        .find(|entry| entry.relative.as_os_str().is_empty())
        .expect("the tree root must be finalized after publication");
    assert_eq!(root_entry.flags, 0);

    let parent = open_directory(&work).expect("open work");
    apply_deferred_flags(parent.as_fd(), OsStr::new("staged"), &deferred)
        .expect("apply deferred flags after publication");
    assert_eq!(read_flags(&staging.join("file")) & flag, flag);

    let parent = open_directory(&work).expect("reopen work");
    let cleared = deferred
        .iter()
        .map(|entry| DeferredFlags {
            relative: entry.relative.clone(),
            flags: entry.flags & !flag,
        })
        .collect::<Vec<_>>();
    apply_deferred_flags(parent.as_fd(), OsStr::new("staged"), &cleared)
        .expect("clear deferred flags");
    assert_eq!(read_flags(&staging.join("file")) & flag, 0);
    let _ = fs::remove_dir_all(&staging);
}

#[test]
fn nonblocking_flags_are_applied_at_the_staged_name() {
    let fixture = Fixture::new();
    let work = fixture.path("work");
    let source = work.join("source");
    fs::create_dir(&source).expect("create source");
    fs::write(source.join("file"), b"flag payload").expect("write file");
    let flag = IFlags::NODUMP.bits();
    if set_flag(&source.join("file"), flag, true).is_err() {
        eprintln!("NODUMP is unavailable on the fixture filesystem");
        return;
    }
    let mut progress = TransferProgress::default();
    let mut deferred = Vec::new();
    transfer_with(
        &work,
        "source",
        &work,
        "staged",
        EngineOptions::default(),
        &mut progress,
        &mut deferred,
    )
    .expect("transfer flagged source");
    let staged = work.join("staged/file");
    assert_eq!(
        read_flags(&staged) & flag,
        flag,
        "a representable non-blocking flag must be set before publication"
    );
    assert!(
        deferred
            .iter()
            .all(|entry| entry.flags & IFlags::NODUMP.bits() == 0),
        "a non-blocking flag must not be deferred"
    );
    let _ = set_flag(&source.join("file"), flag, false);
}

fn settable_blocking_flag(path: &std::path::Path) -> Option<u32> {
    for flag in [IFlags::IMMUTABLE.bits(), IFlags::APPEND.bits()] {
        if set_flag(path, flag, true).is_ok() {
            if set_flag(path, flag, false).is_ok() {
                return Some(flag);
            }
        }
    }
    None
}

fn read_flags(path: &std::path::Path) -> u32 {
    let file = File::open(path).expect("open flag fixture");
    rustix::fs::ioctl_getflags(&file)
        .expect("read inode flags")
        .bits()
}

fn set_flag(path: &std::path::Path, flag: u32, enabled: bool) -> io::Result<()> {
    let file = File::open(path)?;
    let mut flags = rustix::fs::ioctl_getflags(&file)?.bits();
    if enabled {
        flags |= flag;
    } else {
        flags &= !flag;
    }
    rustix::fs::ioctl_setflags(&file, IFlags::from_bits_retain(flags))?;
    Ok(())
}

fn transfer_with(
    source_parent: &std::path::Path,
    source_name: &str,
    destination_parent: &std::path::Path,
    destination_name: &str,
    engine: EngineOptions,
    progress: &mut Progress,
    deferred_flags: &mut Vec<DeferredFlags>,
) -> io::Result<()> {
    let source = open_directory(source_parent)?;
    let destination = open_directory(destination_parent)?;
    let cancellation = Cancellation::default();
    transfer_tree(TreeTransfer {
        source_parent: source.as_fd(),
        source_name: OsStr::new(source_name),
        destination_parent: destination.as_fd(),
        destination_name: OsStr::new(destination_name),
        engine,
        cancellation: &cancellation,
        progress,
        deferred_flags,
    })
}

#[test]
fn reached_parent_recovery_state_blocks_a_transfer() {
    let fixture = Fixture::new();
    let work = fixture.path("work");
    let source = work.join("source");
    fs::create_dir_all(source.join("nested")).expect("create source");
    fs::write(source.join("nested/file"), b"payload").expect("write file");
    let nested = open_directory(&source.join("nested")).expect("open nested");
    let session = xsh_root::transfer::Session::start(nested.as_fd()).expect("start nested session");
    let cancellation = Cancellation::default();
    cancellation.cancel();
    let legacy = source.join("nested/.fdu-quarantine-v1-0123456789abcdef0123456789abcdef");
    fs::create_dir(&legacy).expect("create legacy session");
    drop(session);
    let mut progress = TransferProgress::default();
    let mut deferred = Vec::new();
    let error = transfer_with(
        &work,
        "source",
        &work,
        "destination",
        EngineOptions::default(),
        &mut progress,
        &mut deferred,
    )
    .expect_err("a reached parent with unknown journal state must block");
    assert!(
        error.to_string().contains("retained")
            || error.to_string().contains("unknown quarantine version")
            || error.to_string().contains("requires inspection"),
        "{error}"
    );
}

#[test]
fn merge_refuses_a_nested_destination_mount_without_opt_in() {
    let fixture = Fixture::new();
    let cwd = fixture.path("work");
    fs::create_dir_all(cwd.join("source/nested")).expect("create source");
    fs::create_dir_all(cwd.join("destination/nested")).expect("create destination");
    fs::write(cwd.join("source/nested/file"), b"nested").expect("write file");
    let nested_mount = mount_id(
        open_directory(&cwd.join("destination/nested"))
            .expect("open nested")
            .as_fd(),
    )
    .expect("read nested mount");
    let parent_mount = mount_id(open_directory(&cwd.join("destination")).expect("open destination").as_fd())
        .expect("read destination mount");
    if nested_mount == parent_mount {
        eprintln!("nested destination mount fixture unavailable; both names share one mount");
        return;
    }
    let plan = plan_exact(&cwd, "source", "destination", PolicyOptions {
        merge: true,
        ..PolicyOptions::default()
    });
    let source_parent = open_directory(&cwd).expect("open cwd");
    let mut progress = TransferProgress::default();
    let error = xsh_root::transfer::merge_tree(xsh_root::transfer::MergeRequest {
        source_parent: source_parent.as_fd(),
        source_name: OsStr::new("source"),
        destination_parent: source_parent.as_fd(),
        destination_name: OsStr::new("destination"),
        expected_source: plan.operands[0].source_observed,
        expected_destination: plan.operands[0]
            .destination_observed
            .expect("merge destination observed"),
        options: PolicyOptions {
            merge: true,
            ..PolicyOptions::default()
        },
        engine: EngineOptions::default(),
        cancellation: &Cancellation::default(),
        progress: &mut progress,
    })
    .expect_err("a nested destination mount must be refused by default");
    assert!(
        error
            .to_string()
            .contains("nested destination mount requires --cross-file-systems"),
        "{error}"
    );
}

#[test]
fn publication_and_observation_agree_on_the_same_entry() {
    let fixture = Fixture::new();
    let cwd = fixture.path("work");
    fs::write(cwd.join("source"), b"publication bytes").expect("write source");
    let plan = plan_exact(&cwd, "source", "destination", PolicyOptions::default());
    let parent = open_directory(&cwd).expect("open cwd");
    let mut progress = TransferProgress::default();
    publish_tree(
        PublishRequest {
            source_parent: parent.as_fd(),
            source_name: OsStr::new("source"),
            destination_parent: parent.as_fd(),
            destination_name: OsStr::new("destination"),
            expected_source: plan.operands[0].source_observed,
            expected_destination: None,
            action: Action::Transfer,
            engine: EngineOptions::default(),
            cancellation: &Cancellation::default(),
            progress: &mut progress,
        },
        |_| Ok(()),
    )
    .expect("publish");
    let observed = observe_path(&cwd.join("destination"));
    let metadata = fs::metadata(cwd.join("destination")).expect("stat destination");
    assert_eq!(observed.identity.inode, metadata.ino());
    assert_eq!(observed.identity.device, metadata.dev());
    assert_eq!(observed.size, 17);
    let _ = Publication::new(
        observed.identity,
        observe_path(&cwd).identity,
        "destination".into(),
    )
    .expect("build publication record");
}

#[test]
fn same_size_same_mtime_drift_is_planned_as_a_replacement() {
    let fixture = Fixture::new();
    let cwd = fixture.path("work");
    fs::write(cwd.join("source"), b"new bytes").expect("write source");
    fs::write(cwd.join("target"), b"old bytes").expect("write target");
    set_times(&cwd.join("source"), 1_700_000_500, 321_000_000);
    set_times(&cwd.join("target"), 1_700_000_500, 321_000_000);
    let source = observe_path(&cwd.join("source"));
    let target = observe_path(&cwd.join("target"));
    assert_eq!(source.size, target.size);
    assert_eq!(source.mtime, target.mtime);
    assert_eq!(
        decide(
            source,
            xsh_root::transfer::Target::Existing(target),
            &PolicyOptions {
                collision: Collision::Replace,
                ..PolicyOptions::default()
            }
        )
        .expect("replacement decision"),
        Action::Replace
    );
    assert_eq!(
        decide(
            source,
            xsh_root::transfer::Target::Existing(target),
            &PolicyOptions {
                collision: Collision::Skip,
                ..PolicyOptions::default()
            }
        )
        .expect("skip decision"),
        Action::Skip
    );
}

#[test]
fn hardlink_group_members_share_one_published_inode() {
    let fixture = Fixture::new();
    let work = fixture.path("work");
    let source = work.join("source");
    fs::create_dir(&source).expect("create source");
    fs::write(source.join("first"), b"shared payload").expect("write first");
    fs::hard_link(source.join("first"), source.join("second")).expect("link second");
    symlink("first", source.join("link")).expect("create symlink");
    let mut progress = TransferProgress::default();
    let mut deferred = Vec::new();
    transfer_with(
        &work,
        "source",
        &work,
        "destination",
        EngineOptions {
            jobs: Some(Jobs::new(4).expect("four workers")),
            ..EngineOptions::default()
        },
        &mut progress,
        &mut deferred,
    )
    .expect("transfer hardlink group");
    let destination = work.join("destination");
    assert_eq!(
        fs::metadata(destination.join("first")).expect("stat first").ino(),
        fs::metadata(destination.join("second")).expect("stat second").ino()
    );
    assert_eq!(
        fs::metadata(destination.join("first")).expect("stat first").nlink(),
        2
    );
    assert_eq!(
        fs::read_link(destination.join("link")).expect("read link"),
        std::path::PathBuf::from("first")
    );
    assert_eq!(progress.completed_files, 4);
}

#[test]
fn sync_quick_check_skips_unchanged_and_updates_drifted_files() {
    let fixture = Fixture::new();
    let root = fixture.path("work");
    let source = root.join("src");
    let destination = root.join("dst");
    fs::create_dir(&source).expect("create source");
    fs::create_dir(&destination).expect("create destination");
    fs::write(source.join("same.txt"), b"same bytes").expect("write same");
    fs::write(destination.join("same.txt"), b"same bytes").expect("write destination same");
    set_times(&source.join("same.txt"), 1_700_000_100, 500_000_000);
    set_times(&destination.join("same.txt"), 1_700_000_100, 500_000_000);
    fs::write(source.join("drift.txt"), b"new content").expect("write drift");
    fs::write(destination.join("drift.txt"), b"old content").expect("write destination drift");
    set_times(&source.join("drift.txt"), 1_700_000_200, 0);
    set_times(&destination.join("drift.txt"), 1_700_000_100, 0);

    let cwd = open_directory(&root).expect("open cwd");
    let cancellation = Cancellation::default();
    let progress = sync_contents(&cwd, "src", "dst", &cancellation).expect("sync contents");
    assert_eq!(progress.transferred_files, 1);
    assert_eq!(progress.skipped_files, 1);
    assert_eq!(
        fs::read(destination.join("drift.txt")).expect("read drift"),
        b"new content"
    );

    let progress = sync_contents(&cwd, "src", "dst", &cancellation).expect("second sync");
    assert_eq!(progress.transferred_files, 0);
    assert_eq!(progress.skipped_files, 2);
}

#[test]
fn sync_delete_removes_destination_only_entries_and_keeps_others() {
    let fixture = Fixture::new();
    let root = fixture.path("work");
    let source = root.join("src");
    let destination = root.join("dst");
    fs::create_dir_all(source.join("nested")).expect("create source");
    fs::create_dir_all(destination.join("nested")).expect("create destination");
    fs::write(source.join("kept"), b"kept").expect("write kept");
    fs::write(destination.join("kept"), b"kept").expect("write destination kept");
    fs::write(destination.join("extra"), b"extra").expect("write extra");
    fs::write(destination.join("nested/extra-nested"), b"extra").expect("write nested extra");
    let cwd = open_directory(&root).expect("open cwd");
    let progress = sync_contents_delete(&cwd, "src", "dst").expect("sync with delete");
    assert!(progress.deleted_files >= 2);
    assert!(!destination.join("extra").exists());
    assert!(!destination.join("nested/extra-nested").exists());
    assert_eq!(fs::read(destination.join("kept")).expect("read kept"), b"kept");
    assert!(destination.join("nested").is_dir());

    let progress = sync_contents(&cwd, "src", "dst", &Cancellation::default())
        .expect("sync without delete");
    assert_eq!(progress.deleted_files, 0);
}

#[test]
fn sync_dry_run_reports_without_mutating_either_tree() {
    let fixture = Fixture::new();
    let root = fixture.path("work");
    let source = root.join("src");
    let destination = root.join("dst");
    fs::create_dir(&source).expect("create source");
    fs::create_dir(&destination).expect("create destination");
    fs::write(source.join("new"), b"new").expect("write new");
    fs::write(destination.join("removed"), b"removed").expect("write removed");
    let before = snapshot(&root);
    let cwd = open_directory(&root).expect("open cwd");
    let cancellation = Cancellation::default();
    let progress = xsh_root::transfer::sync_tree(xsh_root::transfer::SyncRequest {
        cwd: cwd.as_fd(),
        operands: xsh_root::transfer::SyncOperands::Contents {
            source: "src".into(),
            directory: "dst".into(),
        },
        options: xsh_root::transfer::SyncOptions {
            delete: true,
            dry_run: true,
            engine: EngineOptions::default(),
        },
        cancellation: &cancellation,
    })
    .expect("dry-run sync");
    assert_eq!(progress.transferred_files, 1);
    assert_eq!(progress.deleted_files, 1);
    assert_eq!(snapshot(&root), before);
}

#[test]
fn sync_cancellation_stops_before_the_first_entry_and_keeps_the_destination() {
    let fixture = Fixture::new();
    let root = fixture.path("work");
    fs::create_dir_all(root.join("src")).expect("create source");
    fs::create_dir_all(root.join("dst")).expect("create destination");
    fs::write(root.join("src/file"), b"source").expect("write source");
    fs::write(root.join("dst/file"), b"destination").expect("write destination");
    let cwd = open_directory(&root).expect("open cwd");
    let cancellation = Cancellation::default();
    cancellation.cancel();
    let error = sync_contents(&cwd, "src", "dst", &cancellation)
        .expect_err("a cancelled sync must report cancellation");
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert_eq!(
        fs::read(root.join("dst/file")).expect("read destination"),
        b"destination"
    );
    assert_eq!(fs::read_dir(root.join("dst")).expect("read dst").count(), 1);
}

#[test]
fn sync_preserves_non_utf8_names_and_broken_symlinks() {
    let fixture = Fixture::new();
    let root = fixture.path("work");
    let source = root.join("src");
    let destination = root.join("dst");
    fs::create_dir(&source).expect("create source");
    fs::create_dir(&destination).expect("create destination");
    let raw_name = std::ffi::OsStr::from_bytes(b"raw-\xff-\xfe.bin");
    let raw_supported = fs::write(source.join(raw_name), b"raw payload").is_ok();
    if !raw_supported {
        eprintln!("raw-name fixture unavailable on this filesystem");
    }
    let broken = std::path::PathBuf::from("nonexistent-relative-target");
    symlink(&broken, source.join("link")).expect("create broken symlink");
    let cwd = open_directory(&root).expect("open cwd");
    let cancellation = Cancellation::default();
    let progress = sync_contents(&cwd, "src", "dst", &cancellation).expect("sync contents");
    assert!(progress.transferred_files >= 1);
    assert_eq!(
        fs::read_link(destination.join("link")).expect("read link"),
        broken
    );
    let progress = sync_contents(&cwd, "src", "dst", &cancellation).expect("second sync");
    assert_eq!(progress.transferred_files - progress.transferred_files, 0);
    if raw_supported {
        assert_eq!(
            fs::read(destination.join(raw_name)).expect("read raw destination"),
            b"raw payload"
        );
    }
}

fn sync_contents(
    cwd: &File,
    source: &str,
    destination: &str,
    cancellation: &Cancellation,
) -> io::Result<xsh_root::transfer::SyncProgress> {
    xsh_root::transfer::sync_tree(xsh_root::transfer::SyncRequest {
        cwd: cwd.as_fd(),
        operands: xsh_root::transfer::SyncOperands::Contents {
            source: source.into(),
            directory: destination.into(),
        },
        options: xsh_root::transfer::SyncOptions::default(),
        cancellation,
    })
}

fn sync_contents_delete(
    cwd: &File,
    source: &str,
    destination: &str,
) -> io::Result<xsh_root::transfer::SyncProgress> {
    xsh_root::transfer::sync_tree(xsh_root::transfer::SyncRequest {
        cwd: cwd.as_fd(),
        operands: xsh_root::transfer::SyncOperands::Contents {
            source: source.into(),
            directory: destination.into(),
        },
        options: xsh_root::transfer::SyncOptions {
            delete: true,
            ..xsh_root::transfer::SyncOptions::default()
        },
        cancellation: &Cancellation::default(),
    })
}

#[test]
fn sync_read_only_destination_file_is_replaced_and_keeps_its_mode() {
    let fixture = Fixture::new();
    let root = fixture.path("work");
    let source = root.join("src");
    let destination = root.join("dst");
    fs::create_dir(&source).expect("create source");
    fs::create_dir(&destination).expect("create destination");
    fs::write(source.join("readonly.txt"), b"initial readonly content").expect("write source");
    fs::set_permissions(source.join("readonly.txt"), fs::Permissions::from_mode(0o444))
        .expect("set source mode");
    let source_file = source.join("readonly.txt");
    let cwd = open_directory(&root).expect("open cwd");
    let mut progress = sync_contents(&cwd, "src", "dst", &Cancellation::default())
        .expect("first sync");
    assert_eq!(progress.transferred_files, 1);
    let target = destination.join("readonly.txt");
    assert_eq!(
        fs::symlink_metadata(&target).expect("stat target").mode() & 0o777,
        0o444
    );
    fs::set_permissions(&source_file, fs::Permissions::from_mode(0o644)).expect("widen source");
    fs::write(&source_file, b"updated readonly content").expect("update source");
    fs::set_permissions(&source_file, fs::Permissions::from_mode(0o444)).expect("restore source");
    progress = sync_contents(&cwd, "src", "dst", &Cancellation::default()).expect("second sync");
    assert_eq!(progress.transferred_files, 1);
    assert_eq!(fs::read(&target).expect("read target"), b"updated readonly content");
    assert_eq!(
        fs::symlink_metadata(&target).expect("stat target").mode() & 0o777,
        0o444
    );
    fs::set_permissions(&source_file, fs::Permissions::from_mode(0o644))
        .expect("restore source mode");
    fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).expect("restore target mode");
}

#[test]
fn sync_into_maps_each_source_basename() {
    let fixture = Fixture::new();
    let root = fixture.path("work");
    fs::create_dir_all(root.join("left")).expect("create left");
    fs::create_dir_all(root.join("right")).expect("create right");
    fs::create_dir_all(root.join("out")).expect("create out");
    fs::write(root.join("left/first"), b"first").expect("write first");
    fs::write(root.join("right/second"), b"second").expect("write second");
    let cwd = open_directory(&root).expect("open cwd");
    let progress = xsh_root::transfer::sync_tree(xsh_root::transfer::SyncRequest {
        cwd: cwd.as_fd(),
        operands: xsh_root::transfer::SyncOperands::Into {
            directory: "out".into(),
            sources: vec!["left/first".into(), "right/second".into()],
        },
        options: xsh_root::transfer::SyncOptions::default(),
        cancellation: &Cancellation::default(),
    })
    .expect("sync into");
    assert_eq!(progress.transferred_files, 2);
    assert_eq!(fs::read(root.join("out/first")).expect("read first"), b"first");
    assert_eq!(fs::read(root.join("out/second")).expect("read second"), b"second");
}

#[test]
fn sync_exact_mirrors_a_directory_into_a_new_destination() {
    let fixture = Fixture::new();
    let root = fixture.path("work");
    fs::create_dir_all(root.join("source/nested")).expect("create source");
    fs::write(root.join("source/nested/file"), b"mirrored").expect("write file");
    let cwd = open_directory(&root).expect("open cwd");
    let progress = xsh_root::transfer::sync_tree(xsh_root::transfer::SyncRequest {
        cwd: cwd.as_fd(),
        operands: xsh_root::transfer::SyncOperands::Exact {
            source: "source".into(),
            destination: "mirror".into(),
        },
        options: xsh_root::transfer::SyncOptions::default(),
        cancellation: &Cancellation::default(),
    })
    .expect("exact sync");
    assert!(progress.transferred_files >= 1);
    assert_eq!(
        fs::read(root.join("mirror/nested/file")).expect("read mirror"),
        b"mirrored"
    );
}

#[test]
fn sync_rejects_a_dry_run_free_destination_that_is_a_file_for_contents() {
    let fixture = Fixture::new();
    let root = fixture.path("work");
    fs::create_dir(root.join("src")).expect("create source");
    fs::write(root.join("not-a-directory"), b"file").expect("write file");
    let cwd = open_directory(&root).expect("open cwd");
    let error = sync_contents(&cwd, "src", "not-a-directory", &Cancellation::default())
        .expect_err("a file destination must be refused");
    assert_eq!(error.kind(), io::ErrorKind::NotADirectory);
}

#[test]
fn observation_reports_every_version_field() {
    let fixture = Fixture::new();
    let path = fixture.path("source");
    fs::write(&path, b"observation").expect("write source");
    set_times(&path, 1_700_000_123, 456_000_000);
    let observed = observe_path(&path);
    let metadata = fs::metadata(&path).expect("stat source");
    assert_eq!(observed.mode & 0o7777, metadata.mode() & 0o7777);
    assert_eq!(observed.uid, metadata.uid());
    assert_eq!(observed.gid, metadata.gid());
    assert_eq!(observed.links, metadata.nlink());
    assert_eq!(observed.size, metadata.len());
    assert_eq!(observed.mtime.seconds, 1_700_000_123);
    assert_eq!(observed.mtime.nanoseconds, 456_000_000);
    assert!(observed.same_version(observed));
}

#[test]
fn iterative_transfer_preserves_a_tree_beyond_the_path_limit() {
    let fixture = Fixture::new();
    let work = fixture.path("work");
    let source = work.join("source");
    fs::create_dir(&source).expect("create source");
    const WIDE_FILES: u32 = 768;
    for index in 0..WIDE_FILES {
        fs::write(source.join(format!("wide-{index:05}")), payload(index)).expect("write wide leaf");
    }
    const NAME_BYTES: usize = 64;
    let source_fd = open_directory(&source).expect("open source");
    let mut directory = rustix::fs::openat(
        source_fd.as_fd(),
        c".",
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .expect("reopen source");
    let mut path_bytes = 0;
    let mut index = 0;
    while path_bytes <= 4096 {
        let name = format!("deep-{index:03}-{}", "x".repeat(NAME_BYTES - 9));
        path_bytes += name.len() + 1;
        rustix::fs::mkdirat(directory.as_fd(), &name, rustix::fs::Mode::RWXU)
            .expect("create deep level");
        directory = rustix::fs::openat(
            directory.as_fd(),
            &name,
            rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
        .expect("open deep level");
        index += 1;
    }
    assert!(path_bytes > 4096);
    let terminal = rustix::fs::openat(
        directory.as_fd(),
        c"terminal",
        rustix::fs::OFlags::WRONLY | rustix::fs::OFlags::CREATE | rustix::fs::OFlags::EXCL | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )
    .expect("create deep terminal");
    std::io::Write::write_all(
        &mut File::from(terminal),
        b"deep content beyond PATH_MAX\0\xff",
    )
    .expect("write deep terminal");

    let mut progress = TransferProgress::default();
    let mut deferred = Vec::new();
    transfer_with(
        &work,
        "source",
        &work,
        "destination",
        EngineOptions::default(),
        &mut progress,
        &mut deferred,
    )
    .expect("iterative transfer of a deep and wide tree");

    let destination = work.join("destination");
    for index in 0..WIDE_FILES {
        assert_eq!(
            fs::read(destination.join(format!("wide-{index:05}"))).expect("read wide leaf"),
            payload(index),
            "wide-{index:05}"
        );
    }
    let destination_fd = open_directory(&destination).expect("open destination");
    let mut directory = rustix::fs::openat(
        destination_fd.as_fd(),
        c".",
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .expect("reopen destination");
    let mut path_bytes = 0;
    let mut index = 0;
    while path_bytes <= 4096 {
        let name = format!("deep-{index:03}-{}", "x".repeat(NAME_BYTES - 9));
        path_bytes += name.len() + 1;
        directory = rustix::fs::openat(
            directory.as_fd(),
            &name,
            rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
        .expect("open deep destination level");
        index += 1;
    }
    let mut terminal = Vec::new();
    std::io::Read::read_to_end(
        &mut File::from(
            rustix::fs::openat(
                directory.as_fd(),
                c"terminal",
                rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::CLOEXEC,
                rustix::fs::Mode::empty(),
            )
            .expect("open deep terminal"),
        ),
        &mut terminal,
    )
    .expect("read deep terminal");
    assert_eq!(terminal, b"deep content beyond PATH_MAX\0\xff");
    assert_eq!(progress.failed_files, 0);
    // Every nested directory, every wide leaf, the deep terminal and the root.
    assert_eq!(
        progress.completed_files,
        u64::from(WIDE_FILES) + index as u64 + 2
    );
    assert_eq!(progress.completed_files, progress.discovered_files);
}

#[test]
fn descriptor_budget_rejects_unaffordable_parallelism_before_mutation() {
    const CHILD: &str = "XSH_FS_RESOURCE_CHILD";
    const FIXTURE: &str = "XSH_FS_RESOURCE_FIXTURE";
    let Some(mode) = std::env::var_os(CHILD) else {
        let fixture = Fixture::new();
        let work = fixture.path("work");
        fs::create_dir(work.join("source")).expect("create source");
        fs::write(work.join("source/file"), b"resource admission payload").expect("write source");
        let status = std::process::Command::new(std::env::current_exe().expect("test binary"))
            .args(["--exact", "engine::descriptor_budget_rejects_unaffordable_parallelism_before_mutation", "--ignored", "--nocapture"])
            .env(CHILD, "descriptors")
            .env(FIXTURE, &work)
            .status()
            .expect("run descriptor child");
        assert!(status.success(), "descriptor child failed: {status}");
        assert_eq!(
            fs::read(work.join("source/file")).expect("read source"),
            b"resource admission payload"
        );
        return;
    };
    if mode != "descriptors" {
        return;
    }
    let root = std::path::PathBuf::from(std::env::var_os(FIXTURE).expect("fixture path"));
    let existing = rustix::process::getrlimit(rustix::process::Resource::Nofile);
    rustix::process::setrlimit(
        rustix::process::Resource::Nofile,
        rustix::process::Rlimit {
            current: Some(48),
            maximum: existing.maximum,
        },
    )
    .expect("lower the descriptor limit");
    let admitted = worker_count(&EngineOptions::default()).expect("admit one worker");
    assert_eq!(admitted, 1);
    let refused = worker_count(&EngineOptions {
        jobs: Some(Jobs::new(4).expect("four workers")),
        ..EngineOptions::default()
    })
    .expect_err("explicit parallelism beyond the budget must be refused");
    assert_eq!(refused.kind(), io::ErrorKind::InvalidInput);
    let work = open_directory(&root).expect("open fixture");
    let mut progress = TransferProgress::default();
    let mut deferred = Vec::new();
    transfer_tree(TreeTransfer {
        source_parent: work.as_fd(),
        source_name: OsStr::new("source"),
        destination_parent: work.as_fd(),
        destination_name: OsStr::new("destination"),
        engine: EngineOptions {
            jobs: Some(Jobs::new(4).expect("four workers")),
            ..EngineOptions::default()
        },
        cancellation: &Cancellation::default(),
        progress: &mut progress,
        deferred_flags: &mut deferred,
    })
    .expect_err("an unaffordable worker request must fail before mutation");
    assert!(!root.join("destination").exists());
}

#[test]
fn sync_replacement_cleans_staging_when_the_source_disappears() {
    let fixture = Fixture::new();
    let root = fixture.path("work");
    let source = root.join("src");
    let destination = root.join("dst");
    fs::create_dir(&source).expect("create source");
    fs::create_dir(&destination).expect("create destination");
    fs::create_dir_all(destination.join("sub")).expect("create destination subtree");
    let target = destination.join("sub/target.bin");
    fs::write(&target, b"original-destination-content").expect("write target");
    let cwd = open_directory(&root).expect("open cwd");

    let error = xsh_root::transfer::sync_tree(xsh_root::transfer::SyncRequest {
        cwd: cwd.as_fd(),
        operands: xsh_root::transfer::SyncOperands::Exact {
            source: "src/missing.bin".into(),
            destination: "dst/sub/target.bin".into(),
        },
        options: xsh_root::transfer::SyncOptions::default(),
        cancellation: &Cancellation::default(),
    })
    .expect_err("a missing source must fail the sync");
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    assert_eq!(
        fs::read(&target).expect("read target"),
        b"original-destination-content"
    );
    for entry in fs::read_dir(destination.join("sub")).expect("read destination subtree") {
        let entry = entry.expect("destination entry");
        assert_eq!(
            entry.file_name(),
            std::ffi::OsString::from("target.bin"),
            "a failed replacement must not leave a staging artifact"
        );
    }
}

#[test]
fn sync_cancellation_leaves_no_staging_artifact_and_keeps_the_previous_destination() {
    let fixture = Fixture::new();
    let root = fixture.path("work");
    let source = root.join("src");
    let destination = root.join("dst");
    fs::create_dir(&source).expect("create source");
    fs::create_dir(&destination).expect("create destination");
    for index in 1..=10 {
        fs::write(source.join(format!("file-{index}.bin")), vec![b'X'; 64 * 1024]).expect("write source");
        fs::write(destination.join(format!("file-{index}.bin")), vec![b'Y'; 64 * 1024])
            .expect("write destination");
    }
    let cwd = open_directory(&root).expect("open cwd");
    let cancellation = Cancellation::default();
    cancellation.cancel();
    let error = sync_contents(&cwd, "src", "dst", &cancellation)
        .expect_err("a cancelled sync must report cancellation");
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    for index in 1..=10 {
        assert_eq!(
            fs::read(destination.join(format!("file-{index}.bin"))).expect("read destination"),
            vec![b'Y'; 64 * 1024],
            "pre-existing file {index} must be preserved on cancellation"
        );
    }
    let entries = fs::read_dir(&destination).expect("read destination").count();
    assert_eq!(entries, 10);
}

#[test]
fn sync_replaces_a_read_only_destination_directory_tree() {
    let fixture = Fixture::new();
    let root = fixture.path("work");
    let source = root.join("src");
    let destination = root.join("dst");
    fs::create_dir(&source).expect("create source");
    fs::create_dir(&destination).expect("create destination");
    fs::write(source.join("file"), b"updated").expect("write source");
    fs::write(destination.join("file"), b"stale").expect("write destination");
    let cwd = open_directory(&root).expect("open cwd");
    let progress = sync_contents_delete(&cwd, "src", "dst").expect("sync");
    assert!(progress.transferred_files >= 1);
    assert_eq!(fs::read(destination.join("file")).expect("read destination"), b"updated");
}

#[test]
fn sync_contents_requires_a_directory_source() {
    let fixture = Fixture::new();
    let root = fixture.path("work");
    fs::write(root.join("source"), b"single file").expect("write source");
    fs::create_dir(root.join("destination")).expect("create destination");
    let cwd = open_directory(&root).expect("open cwd");
    let error = sync_contents(&cwd, "source", "destination", &Cancellation::default())
        .expect_err("--contents requires a directory source");
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
}

#[test]
fn sync_parallel_workers_replace_a_wide_destination_directory() {
    let fixture = Fixture::new();
    let root = fixture.path("work");
    let source = root.join("src");
    let destination = root.join("dst");
    fs::create_dir(&source).expect("create source");
    fs::create_dir(&destination).expect("create destination");
    const FILES: u32 = 128;
    for index in 0..FILES {
        fs::write(source.join(format!("file-{index:04}")), payload(index)).expect("write source");
        fs::write(destination.join(format!("file-{index:04}")), b"stale").expect("write destination");
    }
    let cwd = open_directory(&root).expect("open cwd");
    let progress = xsh_root::transfer::sync_tree(xsh_root::transfer::SyncRequest {
        cwd: cwd.as_fd(),
        operands: xsh_root::transfer::SyncOperands::Contents {
            source: "src".into(),
            directory: "dst".into(),
        },
        options: xsh_root::transfer::SyncOptions {
            engine: EngineOptions {
                jobs: Some(Jobs::new(4).expect("four workers")),
                ..EngineOptions::default()
            },
            ..xsh_root::transfer::SyncOptions::default()
        },
        cancellation: &Cancellation::default(),
    })
    .expect("parallel sync");
    assert_eq!(progress.transferred_files, u64::from(FILES));
    for index in 0..FILES {
        assert_eq!(
            fs::read(destination.join(format!("file-{index:04}"))).expect("read destination"),
            payload(index),
            "file-{index:04}"
        );
    }
    for entry in fs::read_dir(&destination).expect("read destination") {
        let name = entry.expect("destination entry").file_name();
        assert!(
            !name.to_string_lossy().starts_with(".xsh-sync-"),
            "a parallel sync must not leave a staging artifact: {name:?}"
        );
    }
}

#[test]
fn sync_does_not_traverse_a_destination_symlink() {
    let fixture = Fixture::new();
    let root = fixture.path("work");
    let source = root.join("src");
    let destination = root.join("dest");
    let outside = root.join("outside");
    fs::create_dir_all(source.join("sub")).expect("create source");
    fs::create_dir_all(&destination).expect("create destination");
    fs::create_dir_all(&outside).expect("create outside");
    fs::write(source.join("sub/copied"), b"inside-copy").expect("write source");
    fs::write(outside.join("sentinel"), b"outside-sentinel").expect("write sentinel");
    symlink("../outside", destination.join("sub")).expect("create destination symlink");
    let cwd = open_directory(&root).expect("open cwd");
    let progress = sync_contents_delete(&cwd, "src", "dest").expect("sync contents");
    assert_eq!(fs::read(outside.join("sentinel")).expect("read sentinel"), b"outside-sentinel");
    assert!(!outside.join("copied").exists(), "transfers must not follow the destination symlink");
    let mut outside_names = fs::read_dir(&outside)
        .expect("read outside")
        .flatten()
        .map(|entry| entry.file_name())
        .collect::<Vec<_>>();
    outside_names.sort();
    assert_eq!(
        outside_names,
        vec![std::ffi::OsString::from("sentinel")],
        "nothing may be written outside the destination"
    );
    let replaced = fs::symlink_metadata(destination.join("sub")).expect("stat destination sub");
    assert!(replaced.is_dir() && !replaced.is_symlink());
    assert_eq!(
        fs::read(destination.join("sub/copied")).expect("read copied"),
        b"inside-copy"
    );
    assert!(progress.transferred_files >= 1);
}

#[test]
fn sync_contents_source_inside_destination_is_rejected_before_mutation() {
    let fixture = Fixture::new();
    let root = fixture.path("work");
    let destination = root.join("dest");
    let source = destination.join("src");
    fs::create_dir_all(&source).expect("create nested source");
    let only_copy = source.join("only-copy");
    fs::write(&only_copy, b"source-must-survive").expect("write source");
    let cwd = open_directory(&root).expect("open cwd");
    let error = sync_contents_delete(&cwd, "dest/src", "dest")
        .expect_err("a source nested in the destination must be refused");
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(error.to_string().contains("overlap"), "{error}");
    assert_eq!(fs::read(&only_copy).expect("read source"), b"source-must-survive");
    assert!(destination.is_dir());
}

#[test]
fn sync_overlap_is_rejected_for_operand_aliases() {
    let fixture = Fixture::new();
    let root = fixture.path("work");
    let destination = root.join("dest");
    let source = destination.join("src");
    fs::create_dir_all(&source).expect("create nested source");
    let only_copy = source.join("only-copy");
    fs::write(&only_copy, b"alias-source").expect("write source");
    symlink(&destination, root.join("alias")).expect("create alias");
    let cwd = open_directory(&root).expect("open cwd");

    let error = sync_contents_delete(&cwd, "alias/src", "dest")
        .expect_err("a symlinked alias of the destination must be refused");
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert_eq!(fs::read(&only_copy).expect("read source"), b"alias-source");

    let error = sync_contents_delete(&cwd, "dest/../dest/src", "dest")
        .expect_err("a dotted alias of the destination must be refused");
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert_eq!(fs::read(&only_copy).expect("read source"), b"alias-source");
}

#[test]
fn sync_overlap_is_rejected_for_multiple_source_into() {
    let fixture = Fixture::new();
    let root = fixture.path("work");
    let source = root.join("src");
    let destination = root.join("dest");
    let inside = destination.join("inside");
    fs::create_dir_all(&source).expect("create source");
    fs::create_dir_all(&inside).expect("create inside");
    fs::write(source.join("file"), b"source-file").expect("write source");
    let inside_file = inside.join("only-copy");
    fs::write(&inside_file, b"inside-source").expect("write inside source");
    let cwd = open_directory(&root).expect("open cwd");
    let error = xsh_root::transfer::sync_tree(xsh_root::transfer::SyncRequest {
        cwd: cwd.as_fd(),
        operands: xsh_root::transfer::SyncOperands::Into {
            directory: "dest".into(),
            sources: vec!["src".into(), "dest/inside".into()],
        },
        options: xsh_root::transfer::SyncOptions {
            delete: true,
            ..xsh_root::transfer::SyncOptions::default()
        },
        cancellation: &Cancellation::default(),
    })
    .expect_err("a source nested in the destination must be refused");
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert_eq!(fs::read(source.join("file")).expect("read source"), b"source-file");
    assert_eq!(fs::read(&inside_file).expect("read inside source"), b"inside-source");
    assert!(!destination.join("src").exists(), "no mapping may run after overlap rejection");
}

#[test]
fn sync_delete_permission_failure_is_reported_and_counts_nothing() {
    if rustix::process::geteuid().as_raw() == 0 {
        eprintln!("root ignores directory write permission; the deletion denial is unrepresentable");
        return;
    }
    let fixture = Fixture::new();
    let root = fixture.path("work");
    let source = root.join("src");
    let destination = root.join("dest");
    let locked = destination.join("locked");
    fs::create_dir_all(&source).expect("create source");
    fs::create_dir_all(&locked).expect("create locked");
    let kept = locked.join("kept");
    fs::write(&kept, b"must-be-kept").expect("write kept");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555))
        .expect("restrict locked directory");
    let cwd = open_directory(&root).expect("open cwd");
    let error = sync_contents_delete(&cwd, "src", "dest")
        .expect_err("a denied deletion must fail the sync");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("restore locked");
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert!(error.to_string().contains("cannot delete"), "{error}");
    assert_eq!(fs::read(&kept).expect("read kept"), b"must-be-kept");
    assert!(kept.exists(), "failed deletion must retain the entry");
}

#[test]
fn sync_exact_destination_through_a_symlinked_parent_is_followed() {
    let fixture = Fixture::new();
    let root = fixture.path("work");
    fs::write(root.join("file"), b"payload").expect("write file");
    fs::create_dir(root.join("real")).expect("create real");
    symlink("real", root.join("link")).expect("create symlink");
    let cwd = open_directory(&root).expect("open cwd");
    let progress = xsh_root::transfer::sync_tree(xsh_root::transfer::SyncRequest {
        cwd: cwd.as_fd(),
        operands: xsh_root::transfer::SyncOperands::Exact {
            source: "file".into(),
            destination: "link/file".into(),
        },
        options: xsh_root::transfer::SyncOptions::default(),
        cancellation: &Cancellation::default(),
    })
    .expect("exact sync through a symlinked parent");
    assert_eq!(progress.transferred_files, 1);
    assert!(fs::symlink_metadata(root.join("link")).expect("stat link").is_symlink());
    assert_eq!(fs::read(root.join("real/file")).expect("read real file"), b"payload");
}

#[test]
fn sync_delete_is_dry_run_safe_and_reports_counts() {
    let fixture = Fixture::new();
    let root = fixture.path("work");
    fs::create_dir_all(root.join("src")).expect("create src");
    fs::create_dir_all(root.join("dst")).expect("create dst");
    fs::write(root.join("dst/extra"), b"extra").expect("write extra");
    let before = snapshot_dir(&root.join("dst"));
    let cwd = open_directory(&root).expect("open cwd");
    let progress = xsh_root::transfer::sync_tree(xsh_root::transfer::SyncRequest {
        cwd: cwd.as_fd(),
        operands: xsh_root::transfer::SyncOperands::Contents {
            source: "src".into(),
            directory: "dst".into(),
        },
        options: xsh_root::transfer::SyncOptions {
            delete: true,
            dry_run: true,
            engine: EngineOptions::default(),
        },
        cancellation: &Cancellation::default(),
    })
    .expect("dry-run delete");
    assert_eq!(progress.deleted_files, 1);
    assert_eq!(snapshot_dir(&root.join("dst")), before);
}

fn snapshot_dir(path: &std::path::Path) -> Vec<(std::ffi::OsString, u64)> {
    let mut entries = fs::read_dir(path)
        .expect("read snapshot directory")
        .flatten()
        .map(|entry| {
            (
                entry.file_name(),
                entry.metadata().expect("stat snapshot entry").ino(),
            )
        })
        .collect::<Vec<_>>();
    entries.sort();
    entries
}
