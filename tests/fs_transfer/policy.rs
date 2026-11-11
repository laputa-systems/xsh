use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::os::fd::AsFd;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};

use xsh_root::transfer::{
    Action, Cancellation, Collision, MergeRequest, Operands, Operation, PlannedOperand,
    PolicyOptions, PolicyRequest, Preflight, Progress, Target, decide, merge_tree, observe_fd,
    preflight,
};

use super::support::{Fixture, observe_path, open_directory, publish_at};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    File,
    Directory,
    Symlink,
    DanglingSymlink,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Decision {
    Transfer,
    Replace,
    Merge,
    Skip,
    Error,
}

impl Decision {
    fn status(self) -> i32 {
        if self == Self::Error { 1 } else { 0 }
    }
}

struct RunResult {
    status: i32,
}

fn status_for(error: &io::Error) -> i32 {
    if error.kind() == io::ErrorKind::InvalidInput {
        2
    } else {
        1
    }
}

fn exact(source: &str, destination: &str) -> Operands {
    Operands::Exact {
        source: source.into(),
        destination: destination.into(),
    }
}

fn final_name(path: &Path) -> &OsStr {
    path.file_name()
        .unwrap_or_else(|| path.as_os_str())
}

fn source_parent(cwd: &Path, path: &Path) -> io::Result<fs::File> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    open_directory(&cwd.join(parent))
}

fn destination_parent(
    cwd: &Path,
    plan: &Preflight,
    operand: &PlannedOperand,
) -> io::Result<fs::File> {
    match &plan.into_directory {
        Some(directory) => directory.try_clone().map(fs::File::from),
        None => {
            let parent = operand
                .destination
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            open_directory(&cwd.join(parent))
        }
    }
}

fn apply_operand(
    cwd: &Path,
    plan: &Preflight,
    index: usize,
    options: PolicyOptions,
    progress: &mut Progress,
) -> io::Result<()> {
    let operand = &plan.operands[index];
    if matches!(operand.action, Action::Skip | Action::SameEntry) {
        progress.skipped_files += 1;
        return Ok(());
    }
    let source_file = source_parent(cwd, &operand.source)?;
    let destination_file = destination_parent(cwd, plan, operand)?;
    if observe_fd(source_file.as_fd())?.identity != operand.source_parent {
        return Err(io::Error::other("source parent changed after preflight"));
    }
    if observe_fd(destination_file.as_fd())?.identity != operand.destination_parent {
        return Err(io::Error::other(
            "destination parent changed after preflight",
        ));
    }
    let source_path = cwd.join(&operand.source);
    match operand.action {
        Action::Merge => merge_tree(MergeRequest {
            source_parent: source_file.as_fd(),
            source_name: final_name(&source_path),
            destination_parent: destination_file.as_fd(),
            destination_name: &operand.destination_name,
            expected_source: operand.source_observed,
            expected_destination: operand
                .destination_observed
                .expect("merge destination was observed"),
            options,
            durable: false,
            cancellation: &Cancellation::default(),
            progress,
        }),
        Action::Transfer | Action::Replace => publish_at(
            source_file.as_fd(),
            final_name(&source_path),
            destination_file.as_fd(),
            &operand.destination_name,
            operand.action,
            operand.source_observed,
            if operand.action == Action::Replace {
                operand.destination_observed
            } else {
                None
            },
            false,
            progress,
        ),
        Action::Skip | Action::SameEntry => Ok(()),
    }
}

fn remove_source(cwd: &Path, operand: &PlannedOperand) -> io::Result<()> {
    let path = cwd.join(&operand.source);
    let metadata = fs::symlink_metadata(&path)?;
    if metadata.file_type().is_dir() {
        fs::remove_dir_all(&path)
    } else {
        fs::remove_file(&path)
    }
}

fn run(
    cwd: &Path,
    operation: Operation,
    operands: &Operands,
    options: PolicyOptions,
) -> RunResult {
    let directory = open_directory(cwd).expect("open fixture cwd");
    let plan = match preflight(
        directory.as_fd(),
        &PolicyRequest {
            operation,
            operands,
            options,
            home: None,
        },
    ) {
        Ok(plan) => plan,
        Err(error) => {
            return RunResult {
                status: status_for(&error.error),
            };
        }
    };
    if options.dry_run {
        return RunResult {
            status: dry_run_status(cwd, &plan, options),
        };
    }
    let mut progress = Progress::default();
    for index in 0..plan.operands.len() {
        if apply_operand(cwd, &plan, index, options, &mut progress).is_err() {
            return RunResult { status: 1 };
        }
        let retires = operation == Operation::Move
            && !matches!(
                plan.operands[index].action,
                Action::Skip | Action::SameEntry
            );
        if retires && remove_source(cwd, &plan.operands[index]).is_err() {
            return RunResult { status: 1 };
        }
    }
    RunResult { status: 0 }
}

fn dry_run_status(cwd: &Path, plan: &Preflight, options: PolicyOptions) -> i32 {
    for operand in &plan.operands {
        if operand.action == Action::Merge
            && merge_would_error(cwd, operand, options).is_err()
        {
            return 1;
        }
    }
    0
}

fn merge_would_error(
    cwd: &Path,
    operand: &PlannedOperand,
    options: PolicyOptions,
) -> io::Result<()> {
    walk_plan(
        &cwd.join(&operand.source),
        &cwd.join(&operand.destination),
        options,
    )
}

fn walk_plan(source: &Path, destination: &Path, options: PolicyOptions) -> io::Result<()> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        let source_observed = observe_path(&entry.path());
        let destination_path = destination.join(&name);
        let destination_observed = match fs::symlink_metadata(&destination_path) {
            Ok(_) => Some(observe_path(&destination_path)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        let action = decide(
            source_observed,
            destination_observed.map_or(Target::Absent, Target::Existing),
            &options,
        )?;
        if action == Action::Merge {
            walk_plan(&entry.path(), &destination_path, options)?;
        }
    }
    Ok(())
}

#[test]
fn generated_top_level_policy_matrix_matches_the_logical_model() {
    let kinds = [Kind::File, Kind::Directory, Kind::Symlink, Kind::DanglingSymlink];
    let collisions = [Collision::Error, Collision::Skip, Collision::Replace];

    for operation in [Operation::Copy, Operation::Move] {
        for source_kind in kinds {
            for destination_kind in kinds.into_iter().map(Some).chain(std::iter::once(None)) {
                for collision in collisions {
                    for merge in [false, true] {
                        let fixture = Fixture::new();
                        let cwd = fixture.path("work");
                        create_entry(&cwd, "source", source_kind, "source");
                        if let Some(destination_kind) = destination_kind {
                            create_entry(&cwd, "destination", destination_kind, "destination");
                        }
                        let decision = policy_model(source_kind, destination_kind, collision, merge);
                        let result = run(
                            &cwd,
                            operation,
                            &exact("source", "destination"),
                            PolicyOptions {
                                collision,
                                merge,
                                dry_run: false,
                            },
                        );
                        assert_eq!(
                            result.status,
                            decision.status(),
                            "{operation:?}: {source_kind:?} -> {destination_kind:?}, {collision:?}, merge={merge}"
                        );
                        let source_remains = fs::symlink_metadata(cwd.join("source")).is_ok();
                        let should_retire = operation == Operation::Move
                            && matches!(
                                decision,
                                Decision::Transfer | Decision::Replace | Decision::Merge
                            );
                        assert_eq!(
                            source_remains, !should_retire,
                            "{operation:?}: {source_kind:?} -> {destination_kind:?}, {collision:?}, merge={merge}, {decision:?}"
                        );

                        if decision.status() == 0 {
                            assert_successful_model_result(
                                &cwd,
                                destination_kind,
                                source_kind,
                                decision,
                            );
                        } else if let Some(destination_kind) = destination_kind {
                            assert_entry_unchanged(
                                &cwd,
                                "destination",
                                destination_kind,
                                "destination",
                            );
                        } else {
                            panic!("the model rejected an absent destination");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn into_maps_multiple_sources_with_raw_names_and_trailing_slashes() {
    let fixture = Fixture::new();
    let cwd = fixture.path("work");
    fs::write(cwd.join("first"), b"one").expect("write first source");
    let proposed_name = OsString::from_vec(b"raw-\xff-name".to_vec());
    let raw_name = match fs::write(cwd.join(&proposed_name), b"two") {
        Ok(()) => proposed_name,
        Err(error) if error.raw_os_error() == Some(libc::EILSEQ) => {
            eprintln!("raw-name path leg excluded: filesystem rejected malformed UTF-8");
            let fallback = OsString::from("raw-fallback");
            fs::write(cwd.join(&fallback), b"two").expect("write fallback source");
            fallback
        }
        Err(error) => panic!("write second source: {error}"),
    };
    fs::write(cwd.join("-leading"), b"three").expect("write leading dash source");
    fs::create_dir(cwd.join("out")).expect("create output directory");

    let operands = Operands::Into {
        directory: "out".into(),
        first: "first///".into(),
        rest: vec![PathBuf::from(OsStr::new(&raw_name)), "-leading".into()],
    };
    assert_eq!(run(&cwd, Operation::Copy, &operands, PolicyOptions::default()).status, 0);
    assert_eq!(fs::read(cwd.join("out/first")).expect("read first output"), b"one");
    assert_eq!(
        fs::read(cwd.join("out").join(&raw_name)).expect("read raw output"),
        b"two"
    );
    assert_eq!(
        fs::read(cwd.join("out/-leading")).expect("read leading dash output"),
        b"three"
    );
}

#[test]
fn into_holds_a_final_symlink_to_its_directory() {
    let fixture = Fixture::new();
    let cwd = fixture.path("work");
    fs::create_dir(cwd.join("real-output")).expect("create resolved output directory");
    symlink("real-output", cwd.join("output-link")).expect("link output directory");
    fs::write(cwd.join("source"), b"data").expect("write source");

    let operands = Operands::Into {
        directory: "output-link".into(),
        first: "source".into(),
        rest: Vec::new(),
    };
    assert_eq!(run(&cwd, Operation::Copy, &operands, PolicyOptions::default()).status, 0);
    assert_eq!(
        fs::read(cwd.join("real-output/source")).expect("read resolved output"),
        b"data"
    );
    assert_eq!(
        fs::read_link(cwd.join("output-link")).expect("read output directory link"),
        PathBuf::from("real-output")
    );
}

#[test]
fn intermediate_parent_symlinks_resolve_and_into_stays_anchored_after_swap() {
    let fixture = Fixture::new();
    let cwd = fixture.path("work");
    fs::create_dir(cwd.join("real-source")).expect("create source directory");
    fs::write(cwd.join("real-source/first"), b"first").expect("write first source");
    fs::write(cwd.join("real-source/second"), b"second").expect("write second source");
    symlink("real-source", cwd.join("source-link")).expect("link source parent");
    for directory in ["container-one", "container-two"] {
        fs::create_dir(cwd.join(directory)).expect("create output parent");
        fs::create_dir(cwd.join(directory).join("out")).expect("create output directory");
    }
    symlink("container-one", cwd.join("container-link")).expect("link output parent");
    let operands = Operands::Into {
        directory: "container-link/out".into(),
        first: "source-link/first".into(),
        rest: vec!["source-link/second".into()],
    };
    let directory = open_directory(&cwd).expect("open fixture cwd");
    let plan = preflight(
        directory.as_fd(),
        &PolicyRequest {
            operation: Operation::Copy,
            operands: &operands,
            options: PolicyOptions::default(),
            home: None,
        },
    )
    .expect("preflight into operands");
    assert_eq!(plan.operands.len(), 2);
    let mut progress = Progress::default();
    apply_operand(&cwd, &plan, 0, PolicyOptions::default(), &mut progress)
        .expect("publish first operand");
    fs::remove_file(cwd.join("container-link")).expect("remove output link");
    symlink("container-two", cwd.join("container-link")).expect("retarget output link");
    apply_operand(&cwd, &plan, 1, PolicyOptions::default(), &mut progress)
        .expect("publish second operand");

    assert_eq!(
        fs::read(cwd.join("container-one/out/first")).expect("read first output"),
        b"first"
    );
    assert_eq!(
        fs::read(cwd.join("container-one/out/second")).expect("read second output"),
        b"second"
    );
    assert_eq!(
        fs::read_dir(cwd.join("container-two/out"))
            .expect("read retargeted directory")
            .count(),
        0
    );
    assert_eq!(
        fs::read_link(cwd.join("container-link")).expect("read retargeted link"),
        PathBuf::from("container-two")
    );
}

#[test]
fn into_rejects_ambiguous_basenames_before_mutation() {
    for source in ["/", ".", "..", "source/.", "source/.."] {
        let fixture = Fixture::new();
        let cwd = fixture.path("work");
        fs::write(cwd.join("source"), b"data").expect("write source");
        fs::create_dir(cwd.join("out")).expect("create output directory");
        let operands = Operands::Into {
            directory: "out".into(),
            first: source.into(),
            rest: Vec::new(),
        };
        assert_eq!(
            run(&cwd, Operation::Copy, &operands, PolicyOptions::default()).status,
            2,
            "source {source:?}"
        );
        assert_eq!(
            fs::read_dir(cwd.join("out")).expect("read output directory").count(),
            0
        );
        assert_eq!(fs::read(cwd.join("source")).expect("read source"), b"data");
    }
}

#[test]
fn exact_target_appends_into_an_existing_directory() {
    let fixture = Fixture::new();
    let cwd = fixture.path("work");
    fs::write(cwd.join("source"), b"data").expect("write source");
    fs::create_dir(cwd.join("destination")).expect("create destination directory");

    assert_eq!(
        run(
            &cwd,
            Operation::Copy,
            &exact("source", "destination/"),
            PolicyOptions::default()
        )
        .status,
        0
    );
    assert_eq!(
        fs::read(cwd.join("destination/source")).expect("read appended output"),
        b"data"
    );
    assert_eq!(fs::read(cwd.join("source")).expect("read source"), b"data");
}

#[test]
fn overlapping_sources_and_destinations_fail_before_mutation() {
    let fixture = Fixture::new();
    let cwd = fixture.path("work");
    fs::create_dir(cwd.join("tree")).expect("create source directory");
    fs::write(cwd.join("tree/leaf"), b"leaf").expect("write nested source");
    fs::create_dir(cwd.join("out")).expect("create output directory");
    let overlapping = Operands::Into {
        directory: "out".into(),
        first: "tree".into(),
        rest: vec!["tree/leaf".into()],
    };
    assert_eq!(
        run(
            &cwd,
            Operation::Copy,
            &overlapping,
            PolicyOptions {
                collision: Collision::Replace,
                ..PolicyOptions::default()
            }
        )
        .status,
        2
    );
    assert_eq!(
        fs::read_dir(cwd.join("out")).expect("read output directory").count(),
        0
    );

    let nested = exact("tree", "tree/new");
    assert_eq!(run(&cwd, Operation::Copy, &nested, PolicyOptions::default()).status, 2);
    assert!(!cwd.join("tree/new").exists());

    fs::create_dir(cwd.join("a")).expect("create first source directory");
    fs::write(cwd.join("a/same"), b"first").expect("write first source");
    fs::write(cwd.join("out/same"), b"existing").expect("write output leaf");
    let conflicting = Operands::Into {
        directory: "out".into(),
        first: "a/same".into(),
        rest: vec!["out/same".into()],
    };
    assert_eq!(
        run(
            &cwd,
            Operation::Copy,
            &conflicting,
            PolicyOptions {
                collision: Collision::Replace,
                ..PolicyOptions::default()
            }
        )
        .status,
        2
    );
    assert_eq!(
        fs::read(cwd.join("out/same")).expect("read preserved output"),
        b"existing"
    );
}

#[test]
fn duplicate_into_outputs_fail_before_any_source_transfer() {
    let fixture = Fixture::new();
    let cwd = fixture.path("work");
    fs::create_dir(cwd.join("left")).expect("create left source directory");
    fs::create_dir(cwd.join("right")).expect("create right source directory");
    fs::create_dir(cwd.join("out")).expect("create output directory");
    fs::write(cwd.join("left/shared"), b"left source").expect("write left source");
    fs::write(cwd.join("right/shared"), b"right source").expect("write right source");
    let operands = Operands::Into {
        directory: "out".into(),
        first: "left/shared".into(),
        rest: vec!["right/shared".into()],
    };
    assert_eq!(
        run(
            &cwd,
            Operation::Copy,
            &operands,
            PolicyOptions {
                collision: Collision::Replace,
                ..PolicyOptions::default()
            }
        )
        .status,
        2
    );
    assert_eq!(fs::read(cwd.join("left/shared")).expect("read left source"), b"left source");
    assert_eq!(fs::read(cwd.join("right/shared")).expect("read right source"), b"right source");
    assert_eq!(
        fs::read_dir(cwd.join("out")).expect("read output directory").count(),
        0
    );
}

#[test]
fn ancestry_alias_through_a_symlinked_parent_fails_before_mutation() {
    let fixture = Fixture::new();
    let cwd = fixture.path("work");
    fs::create_dir(cwd.join("tree")).expect("create source directory");
    fs::write(cwd.join("tree/leaf"), b"leaf").expect("write nested source");
    symlink("tree", cwd.join("tree-link")).expect("link source directory");
    fs::create_dir(cwd.join("out")).expect("create output directory");
    let operands = Operands::Into {
        directory: "out".into(),
        first: "tree".into(),
        rest: vec!["tree-link/leaf".into()],
    };
    assert_eq!(
        run(
            &cwd,
            Operation::Copy,
            &operands,
            PolicyOptions {
                collision: Collision::Replace,
                ..PolicyOptions::default()
            }
        )
        .status,
        2
    );
    assert_eq!(fs::read(cwd.join("tree/leaf")).expect("read nested source"), b"leaf");
    assert_eq!(
        fs::read_dir(cwd.join("out")).expect("read output directory").count(),
        0
    );
}

#[test]
fn dry_run_collision_and_merge_policies_preserve_names_modes_and_bytes() {
    for operation in [Operation::Copy, Operation::Move] {
        for merge in [false, true] {
            for collision in [Collision::Error, Collision::Skip, Collision::Replace] {
                let fixture = Fixture::new();
                let cwd = fixture.path("work");
                if merge {
                    fs::create_dir(cwd.join("source")).expect("create source directory");
                    fs::create_dir(cwd.join("destination")).expect("create destination directory");
                    fs::write(cwd.join("source/shared"), b"source collision")
                        .expect("write source collision");
                    fs::write(cwd.join("source/new"), b"source new")
                        .expect("write source-only leaf");
                    fs::write(cwd.join("destination/shared"), b"destination collision")
                        .expect("write destination collision");
                    fs::write(cwd.join("destination/keep"), b"destination-only")
                        .expect("write destination-only leaf");
                    fs::set_permissions(
                        cwd.join("source/shared"),
                        fs::Permissions::from_mode(0o640),
                    )
                    .expect("set source file mode");
                    fs::set_permissions(cwd.join("source/new"), fs::Permissions::from_mode(0o600))
                        .expect("set source-only mode");
                    fs::set_permissions(
                        cwd.join("destination/shared"),
                        fs::Permissions::from_mode(0o604),
                    )
                    .expect("set destination file mode");
                    fs::set_permissions(
                        cwd.join("destination/keep"),
                        fs::Permissions::from_mode(0o444),
                    )
                    .expect("set destination-only mode");
                    fs::set_permissions(cwd.join("source"), fs::Permissions::from_mode(0o750))
                        .expect("set source directory mode");
                    fs::set_permissions(
                        cwd.join("destination"),
                        fs::Permissions::from_mode(0o710),
                    )
                    .expect("set destination directory mode");
                } else {
                    fs::write(cwd.join("source"), b"source bytes").expect("write source");
                    fs::write(cwd.join("destination"), b"destination bytes")
                        .expect("write destination");
                    fs::set_permissions(cwd.join("source"), fs::Permissions::from_mode(0o640))
                        .expect("set source mode");
                    fs::set_permissions(
                        cwd.join("destination"),
                        fs::Permissions::from_mode(0o604),
                    )
                    .expect("set destination mode");
                }
                let before = tree_snapshot(&cwd).expect("snapshot working directory");
                let before_root = entry_names(fixture.root()).expect("snapshot fixture root");
                let result = run(
                    &cwd,
                    operation,
                    &exact("source", "destination"),
                    PolicyOptions {
                        collision,
                        merge,
                        dry_run: true,
                    },
                );
                let expected = if collision == Collision::Error { 1 } else { 0 };
                assert_eq!(
                    result.status, expected,
                    "{operation:?}, merge={merge}, {collision:?}"
                );
                assert_eq!(
                    tree_snapshot(&cwd).expect("snapshot working directory after dry-run"),
                    before,
                    "{operation:?}, merge={merge}, {collision:?}"
                );
                assert_eq!(
                    entry_names(fixture.root()).expect("snapshot fixture root after dry-run"),
                    before_root
                );
            }
        }
    }
}

#[test]
fn dry_run_of_an_absent_directory_root_creates_no_destination_or_scratch() {
    for operation in [Operation::Copy, Operation::Move] {
        let fixture = Fixture::new();
        let cwd = fixture.path("work");
        fs::create_dir(cwd.join("source")).expect("create source directory");
        fs::write(cwd.join("source/file"), b"source bytes").expect("write source file");
        fs::set_permissions(cwd.join("source"), fs::Permissions::from_mode(0o750))
            .expect("set source directory mode");
        fs::set_permissions(cwd.join("source/file"), fs::Permissions::from_mode(0o640))
            .expect("set source file mode");
        let before = tree_snapshot(&cwd).expect("snapshot working directory");
        let result = run(
            &cwd,
            operation,
            &exact("source", "new-tree"),
            PolicyOptions {
                dry_run: true,
                ..PolicyOptions::default()
            },
        );
        assert_eq!(result.status, 0, "{operation:?}");
        assert_eq!(
            tree_snapshot(&cwd).expect("snapshot working directory after dry-run"),
            before
        );
        assert!(!cwd.join("new-tree").exists());
    }
}

#[test]
fn merge_directory_policy_precedes_leaf_no_clobber() {
    let fixture = Fixture::new();
    let cwd = fixture.path("work");
    fs::create_dir(cwd.join("source")).expect("create source directory");
    fs::write(cwd.join("source/shared"), b"source").expect("write source leaf");
    fs::write(cwd.join("source/new"), b"new").expect("write new source leaf");
    fs::create_dir(cwd.join("destination")).expect("create destination directory");
    fs::write(cwd.join("destination/shared"), b"destination").expect("write destination leaf");

    assert_eq!(
        run(
            &cwd,
            Operation::Copy,
            &exact("source", "destination"),
            PolicyOptions {
                collision: Collision::Skip,
                merge: true,
                dry_run: false,
            }
        )
        .status,
        0
    );
    assert_eq!(
        fs::read(cwd.join("destination/shared")).expect("read colliding leaf"),
        b"destination"
    );
    assert_eq!(
        fs::read(cwd.join("destination/new")).expect("read new leaf"),
        b"new"
    );
}

#[test]
fn same_entry_is_a_noop_while_hardlink_aliases_follow_collision_policy() {
    let fixture = Fixture::new();
    let cwd = fixture.path("work");
    fs::write(cwd.join("source"), b"data").expect("write source");
    fs::hard_link(cwd.join("source"), cwd.join("alias")).expect("create hardlink alias");

    assert_eq!(
        run(
            &cwd,
            Operation::Copy,
            &exact("source", "source"),
            PolicyOptions::default()
        )
        .status,
        0
    );

    for (collision, expected) in [
        (Collision::Error, 1),
        (Collision::Skip, 0),
        (Collision::Replace, 1),
    ] {
        assert_eq!(
            run(
                &cwd,
                Operation::Copy,
                &exact("source", "alias"),
                PolicyOptions {
                    collision,
                    ..PolicyOptions::default()
                }
            )
            .status,
            expected,
            "{collision:?}"
        );
        assert_eq!(fs::read(cwd.join("alias")).expect("read alias"), b"data");
    }
}

#[test]
fn move_protects_working_directory_ancestors_and_allows_exact_noop() {
    let fixture = Fixture::new();
    let cwd = fixture.path("work");
    assert_eq!(
        run(&cwd, Operation::Move, &exact(".", "."), PolicyOptions::default()).status,
        0
    );

    let work = fixture.path("work/sub");
    fs::create_dir(&work).expect("create nested working directory");
    let operands = exact("..", "../../moved");
    assert_eq!(run(&work, Operation::Move, &operands, PolicyOptions::default()).status, 1);
    assert!(work.is_dir());
    assert!(!fixture.path("work/moved").exists());
}

fn create_entry(directory: &Path, name: &str, kind: Kind, label: &str) {
    let path = directory.join(name);
    match kind {
        Kind::File => fs::write(path, label.as_bytes()).expect("write model file"),
        Kind::Directory => {
            fs::create_dir(&path).expect("create model directory");
            fs::write(path.join(format!("{label}-child")), label.as_bytes())
                .expect("write model child");
        }
        Kind::Symlink => {
            let target = format!("{label}-target");
            fs::write(directory.join(&target), label.as_bytes()).expect("write symlink target");
            symlink(target, path).expect("create model symlink");
        }
        Kind::DanglingSymlink => {
            symlink(format!("{label}-missing"), path).expect("create dangling symlink");
        }
    }
}

fn policy_model(
    source: Kind,
    destination: Option<Kind>,
    collision: Collision,
    merge: bool,
) -> Decision {
    let Some(destination) = destination else {
        return Decision::Transfer;
    };
    if destination == Kind::Directory && !(source == Kind::Directory && merge) {
        return Decision::Transfer;
    }
    if source == Kind::Directory && destination == Kind::Directory && merge {
        return Decision::Merge;
    }
    if collision == Collision::Skip {
        return Decision::Skip;
    }
    if collision == Collision::Replace
        && matches!(
            (source, destination),
            (Kind::File, Kind::File)
                | (Kind::Symlink, Kind::Symlink)
                | (Kind::DanglingSymlink, Kind::Symlink)
                | (Kind::Symlink, Kind::DanglingSymlink)
                | (Kind::DanglingSymlink, Kind::DanglingSymlink)
        )
    {
        return Decision::Replace;
    }
    Decision::Error
}

fn assert_successful_model_result(
    cwd: &Path,
    destination: Option<Kind>,
    source: Kind,
    decision: Decision,
) {
    if destination.is_none() {
        assert_model_entry(cwd, "destination", source, "source");
        return;
    }
    if decision == Decision::Merge {
        assert_eq!(
            fs::read(cwd.join("destination/source-child")).expect("read merged child"),
            b"source"
        );
        assert_eq!(
            fs::read(cwd.join("destination/destination-child"))
                .expect("read preserved merged child"),
            b"destination"
        );
        return;
    }
    if destination == Some(Kind::Directory) && decision == Decision::Transfer {
        assert_model_entry(cwd, "destination/source", source, "source");
        assert_eq!(
            fs::read(cwd.join("destination/destination-child"))
                .expect("read preserved destination child"),
            b"destination"
        );
        return;
    }
    if decision == Decision::Replace {
        assert_model_entry(cwd, "destination", source, "source");
    } else if decision == Decision::Skip {
        assert_entry_unchanged(cwd, "destination", destination.expect("destination exists"), "destination");
    } else {
        panic!("an absent destination must transfer");
    }
}

fn assert_model_entry(cwd: &Path, name: &str, kind: Kind, label: &str) {
    let path = cwd.join(name);
    match kind {
        Kind::File => assert_eq!(fs::read(path).expect("read model output"), label.as_bytes()),
        Kind::Directory => assert_eq!(
            fs::read(path.join(format!("{label}-child"))).expect("read model directory output"),
            label.as_bytes()
        ),
        Kind::Symlink => {
            assert_eq!(
                fs::read_link(path).expect("read model link output"),
                PathBuf::from(format!("{label}-target"))
            );
            assert_eq!(
                fs::read(cwd.join(format!("{label}-target"))).expect("read model link referent"),
                label.as_bytes()
            );
        }
        Kind::DanglingSymlink => assert_eq!(
            fs::read_link(path).expect("read dangling model link output"),
            PathBuf::from(format!("{label}-missing"))
        ),
    }
}

fn assert_entry_unchanged(cwd: &Path, name: &str, kind: Kind, label: &str) {
    let path = cwd.join(name);
    match kind {
        Kind::File => assert_eq!(
            fs::read(path).expect("read preserved model file"),
            label.as_bytes()
        ),
        Kind::Directory => assert_eq!(
            fs::read(path.join(format!("{label}-child"))).expect("read preserved model child"),
            label.as_bytes()
        ),
        Kind::Symlink => {
            assert_eq!(
                fs::read_link(path).expect("read preserved model link"),
                PathBuf::from(format!("{label}-target"))
            );
            assert_eq!(
                fs::read(cwd.join(format!("{label}-target")))
                    .expect("read preserved link referent"),
                label.as_bytes()
            );
        }
        Kind::DanglingSymlink => assert_eq!(
            fs::read_link(path).expect("read preserved dangling link"),
            PathBuf::from(format!("{label}-missing"))
        ),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SnapshotEntry {
    path: PathBuf,
    mode: u32,
    identity: (u64, u64),
    ownership: (u32, u32),
    modified: (i64, i64),
    changed: (i64, i64),
    kind: SnapshotKind,
    bytes: Option<Vec<u8>>,
    link_target: Option<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SnapshotKind {
    File,
    Directory,
    Symlink,
    Other,
}

fn tree_snapshot(root: &Path) -> io::Result<Vec<SnapshotEntry>> {
    fn append(root: &Path, relative: &Path, snapshot: &mut Vec<SnapshotEntry>) -> io::Result<()> {
        let directory = root.join(relative);
        let mut children = fs::read_dir(&directory)?.collect::<Result<Vec<_>, _>>()?;
        children.sort_by_key(fs::DirEntry::file_name);
        for child in children {
            let relative_path = relative.join(child.file_name());
            let path = root.join(&relative_path);
            let metadata = fs::symlink_metadata(&path)?;
            let file_type = metadata.file_type();
            let kind = if file_type.is_file() {
                SnapshotKind::File
            } else if file_type.is_dir() {
                SnapshotKind::Directory
            } else if file_type.is_symlink() {
                SnapshotKind::Symlink
            } else {
                SnapshotKind::Other
            };
            snapshot.push(SnapshotEntry {
                path: relative_path.clone(),
                mode: metadata.mode() & 0o7777,
                identity: (metadata.dev(), metadata.ino()),
                ownership: (metadata.uid(), metadata.gid()),
                modified: (metadata.mtime(), metadata.mtime_nsec()),
                changed: (metadata.ctime(), metadata.ctime_nsec()),
                bytes: if kind == SnapshotKind::File {
                    Some(fs::read(&path)?)
                } else {
                    None
                },
                link_target: if kind == SnapshotKind::Symlink {
                    Some(fs::read_link(&path)?)
                } else {
                    None
                },
                kind: kind.clone(),
            });
            if kind == SnapshotKind::Directory {
                append(root, &relative_path, snapshot)?;
            }
        }
        Ok(())
    }

    let metadata = fs::symlink_metadata(root)?;
    let mut snapshot = vec![SnapshotEntry {
        path: PathBuf::from("."),
        mode: metadata.mode() & 0o7777,
        identity: (metadata.dev(), metadata.ino()),
        ownership: (metadata.uid(), metadata.gid()),
        modified: (metadata.mtime(), metadata.mtime_nsec()),
        changed: (metadata.ctime(), metadata.ctime_nsec()),
        kind: SnapshotKind::Directory,
        bytes: None,
        link_target: None,
    }];
    append(root, Path::new(""), &mut snapshot)?;
    snapshot.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(snapshot)
}

fn entry_names(directory: &Path) -> io::Result<Vec<OsString>> {
    let mut names = fs::read_dir(directory)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<Result<Vec<_>, _>>()?;
    names.sort();
    Ok(names)
}
