//! Native `cp`, `mv`, `rsync` and `rmrf` scenarios from ish's `tests/pty.rs`.
//!
//! Every scenario runs the same commands against both shells and records the
//! visible terminal plus persistent file effects. Fixtures carry ish's own
//! test environment: `TMPDIR` names an empty `.tmp` under HOME, so leftover
//! engine scratch is visible. A transfer large enough to interrupt is waited
//! on through the shared `.fdu-quarantine-v2-*` staging directory both engines
//! create beside the destination; rows whose layout depends on when a key or
//! signal lands (`^C`/`^Z` echo, progress clearing, entry counts) are recorded
//! only through the stable text they must contain.

use super::scenarios::parity;
use super::{Fixture, Live, key, run_scenario};

/// ish's native filesystem test environment.
fn native_fs() -> Fixture {
    Fixture::new().dir(".tmp").env("TMPDIR", "{HOME}/.tmp")
}

/// ish's `native_fs_cancellable_tree` (many small files, then an 8 MiB
/// payload) with four times its files, so a copy on XSH's faster engine still
/// outlasts the delivery of the interrupting key or signal.
fn cancellable_tree() -> Fixture {
    let mut fixture = native_fs();
    let small = "O".repeat(512);
    for index in 0..8192 {
        fixture = fixture.file(&format!("source-tree/file-{index:05}"), &small);
    }
    fixture.file("source-tree/payload", &"m".repeat(8 * 1024 * 1024))
}

/// ish's `native_fs_tree`: `count` files of the bytes `0..4096` as `u8`.
fn byte_tree(mut fixture: Fixture, name: &str, count: usize) -> Fixture {
    let contents: Vec<u8> = (0..4096).map(|index| index as u8).collect();
    for index in 0..count {
        fixture = fixture.file_bytes(&format!("{name}/file-{index:05}"), &contents);
    }
    fixture
}

/// Records whether `relative` exists under HOME.
fn effect_exists(sh: &mut Live, name: &str, relative: &str) {
    let exists = sh.home().join(relative).exists();
    sh.effect(name, if exists { "present" } else { "absent" });
}

/// Records whether the engine scratch directory is empty.
fn effect_scratch_empty(sh: &mut Live) {
    let empty = std::fs::read_dir(sh.home().join(".tmp"))
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(false);
    sh.effect("scratch_empty", if empty { "empty" } else { "entries left" });
}

/// The visible rows reporting a cancellation. The entry count depends on when
/// the interruption landed and becomes `<N>`; so does whether the terminal's
/// `^C` echo survives, since a progress line drawn before the key arrived is
/// erased together with it, so a leading echo is dropped.
fn effect_cancel_notice(sh: &mut Live, name: &str) {
    let rows: Vec<String> = sh
        .screen_text()
        .lines()
        .filter(|row| row.contains("cancelled"))
        .map(|row| row.strip_prefix("^C").unwrap_or(row))
        .map(|row| {
            let Some(end) = row.find(" entries processed") else {
                return row.to_owned();
            };
            let start = row[..end]
                .rfind(|ch: char| !ch.is_ascii_digit())
                .map_or(0, |index| index + 1);
            format!("{}<N>{}", &row[..start], &row[end..])
        })
        .collect();
    sh.effect(name, &rows.join("\n"));
}

/// Starts a staged tree copy and waits until its staging tree is filling,
/// returning the staging path.
fn start_staged_copy(sh: &mut Live) -> std::path::PathBuf {
    sh.text("cp -r --jobs 1 source-tree copy-tree");
    sh.send_now(key::ENTER);
    let stage = sh.wait_for_staging();
    effect_exists(sh, "copy_while_staging", "copy-tree");
    stage
}

parity!(
    native_fs_copy_and_move_lists_keep_successes_and_continue_after_failures,
    native_fs()
        .file("first", "first bytes\u{0}")
        .file("second", "second bytes\n")
        .file("third", "third bytes\u{0}"),
    |sh: &mut Live| {
        sh.line("cp first first-copy && mv second second-moved");
        sh.frame("lists");
        sh.line("echo native-fs-status-$?");
        sh.frame("first_status");
        sh.line("cp missing fallback-copy || mv third third-moved");
        sh.frame("fallback");
        sh.line("echo native-fs-status-$?");
        sh.frame("fallback_status");
        sh.line("mv missing failed-move && cp first skipped-copy");
        sh.frame("failed_chain");
        sh.line("echo native-fs-status-$?");
        sh.frame("failed_status");
        sh.effect_file("first_copy", "first-copy");
        sh.effect_file("second_moved", "second-moved");
        sh.effect_file("third_moved", "third-moved");
        sh.effect_listing("after_lists", "");
    }
);

parity!(
    native_fs_exact_targets_and_into_directories_preserve_real_bytes,
    native_fs()
        .file_bytes("exact-source", &[0, 255, 13, 10, 0, 42])
        .file_bytes("into-source", &[0, 255, 13, 10, 0, 42])
        .file_bytes("move-source", &[0, 255, 13, 10, 0, 42])
        .file_bytes("move-into-source", &[0, 255, 13, 10, 0, 42])
        .dir("copy-into")
        .dir("move-into"),
    |sh: &mut Live| {
        sh.line("cp exact-source exact-target");
        sh.line("cp --into copy-into into-source");
        sh.line("mv move-source move-target");
        sh.line("mv --into move-into move-into-source");
        sh.frame("after_transfers");
        sh.effect_file("exact_target", "exact-target");
        sh.effect_tree("copy_into", "copy-into");
        sh.effect_file("move_target", "move-target");
        sh.effect_tree("move_into", "move-into");
        effect_exists(sh, "move_source", "move-source");
        effect_exists(sh, "move_into_source", "move-into-source");
    }
);

parity!(
    native_fs_multi_source_into_trailing_directory,
    native_fs()
        .file("s1", "first")
        .file("s2", "second")
        .file("m1", "m-first")
        .file("m2", "m-second")
        .file("pack/p1", "p-first")
        .file("pack/p2", "p-second")
        .dir("target"),
    |sh: &mut Live| {
        sh.line("cp s1 s2 target");
        sh.line("echo native-fs-status-$?");
        sh.frame("copy_status");
        sh.line("mv m1 m2 target");
        sh.line("echo native-fs-status-$?");
        sh.frame("move_status");
        sh.line("mv pack/* .");
        sh.line("echo native-fs-status-$?");
        sh.frame("glob_status");
        sh.effect_tree("after_multi_source", "target");
        sh.effect_listing("pack_after_move", "pack");
        sh.effect_file("p1", "p1");
        sh.effect_file("p2", "p2");
        effect_exists(sh, "m1_source", "m1");
        effect_exists(sh, "m2_source", "m2");
    }
);

parity!(
    native_fs_redirections_prevent_failed_transfers_and_capture_stderr,
    native_fs().file_bytes("source", b"redirected bytes\0\xff"),
    |sh: &mut Live| {
        sh.line("cp source blocked > absent-directory/output");
        sh.frame("blocked");
        sh.line("echo native-fs-status-$?");
        sh.frame("blocked_status");
        sh.line("cp missing never-created 2> copy-error");
        sh.line("echo native-fs-status-$?");
        sh.frame("missing_status");
        sh.line("cp --verbose source verbose-copy 2> copy-verbose");
        sh.line("echo native-fs-status-$?");
        sh.frame("verbose_status");
        sh.effect_file("blocked_absent", "blocked");
        sh.effect_file("never_created_absent", "never-created");
        sh.effect_file("copy_error", "copy-error");
        sh.effect_file("copy_verbose", "copy-verbose");
        sh.effect_file("verbose_copy", "verbose-copy");
    }
);

parity!(
    native_fs_parent_placement_accepts_command_alias_expanded_name_and_argument_substitution,
    native_fs().file("source", "parent bytes\u{0}"),
    |sh: &mut Live| {
        sh.line("command cp \"$(echo source)\" command-copy");
        sh.line("alias native-copy cp");
        sh.line("native-copy source alias-copy");
        sh.line("export NATIVE_FS_COMMAND=cp");
        sh.line("$NATIVE_FS_COMMAND source expanded-copy");
        sh.line("echo native-fs-status-$?");
        sh.frame("after_parent_placement");
        sh.effect_file("command_copy", "command-copy");
        sh.effect_file("alias_copy", "alias-copy");
        sh.effect_file("expanded_copy", "expanded-copy");
    }
);

parity!(
    native_fs_standalone_negation_runs_in_the_shell_process,
    native_fs().file("source", "negated copy bytes"),
    |sh: &mut Live| {
        sh.line("! cp source negated-copy");
        sh.line("echo native-fs-status-$?");
        sh.frame("negated_copy");
        sh.line("! mv absent-source negated-move");
        sh.line("echo native-fs-status-$?");
        sh.frame("negated_failed_move");
        sh.line("! mv negated-copy negated-move-success");
        sh.line("echo native-fs-status-$?");
        sh.frame("negated_move");
        sh.line("export NATIVE_FS_COMMAND=cp");
        sh.line(
            "! command command $NATIVE_FS_COMMAND negated-move-success nested-negated-copy > nested-negated-redirection",
        );
        sh.line("echo native-fs-status-$?");
        sh.frame("nested_negation");
        sh.line("alias negated-copy-command cp");
        sh.line("! negated-copy-command source alias-negated-copy");
        sh.line("echo native-fs-status-$?");
        sh.frame("alias_negation");
        effect_exists(sh, "negated_copy_moved_away", "negated-copy");
        effect_exists(sh, "negated_move_absent", "negated-move");
        sh.effect_file("moved_success", "negated-move-success");
        sh.effect_file("nested_negated", "nested-negated-copy");
        sh.effect_file("alias_negated", "alias-negated-copy");
        effect_exists(sh, "nested_redirection", "nested-negated-redirection");
    }
);

parity!(
    native_fs_static_dynamic_and_nested_fork_contexts_refuse_before_redirections,
    native_fs().file("source", "must stay untouched"),
    |sh: &mut Live| {
        sh.line("/bin/echo ignored | cp source static-copy > static-redirection");
        sh.line("echo native-fs-status-$?");
        sh.frame("static_pipeline");
        sh.line("export NATIVE_FS_COMMAND=cp");
        sh.line(
            "/bin/echo ignored | $NATIVE_FS_COMMAND source dynamic-copy > dynamic-redirection",
        );
        sh.line("echo native-fs-status-$?");
        sh.frame("dynamic_pipeline");
        sh.line("echo \"$(cp source nested-copy > nested-redirection)\"");
        sh.line("echo native-fs-status-$?");
        sh.frame("nested_substitution");
        sh.line("echo \"$(echo $(cp source deep-copy > deep-redirection))\"");
        sh.line("echo native-fs-status-$?");
        sh.frame("deep_substitution");
        for path in [
            "static-copy",
            "static-redirection",
            "dynamic-copy",
            "dynamic-redirection",
            "nested-copy",
            "nested-redirection",
            "deep-copy",
            "deep-redirection",
        ] {
            effect_exists(sh, path, path);
        }
    }
);

parity!(
    native_fs_quick_no_progress_copy_stays_quiet,
    native_fs().file("source", "tiny bytes"),
    |sh: &mut Live| {
        sh.line("cp source copy");
        sh.frame("quick_copy");
        sh.line("cp --no-progress source quiet-copy");
        sh.frame("quiet_copy");
        sh.effect_file("copy", "copy");
        sh.effect_file("quiet_copy", "quiet-copy");
    }
);

parity!(
    native_fs_ctrl_c_during_copy_returns_130_to_a_usable_prompt,
    cancellable_tree(),
    |sh: &mut Live| {
        let modes = sh.terminal_modes();
        let stage = start_staged_copy(sh);
        sh.keys_to_prompt(key::CTRL_C);
        effect_cancel_notice(sh, "cancel_notice");
        sh.line("echo native-fs-status-$?");
        sh.frame_rows_containing("status", "native-fs-status-");
        sh.effect("stage", if stage.exists() { "left behind" } else { "removed" });
        effect_exists(sh, "source_retained", "source-tree/file-00000");
        effect_exists(sh, "copy_tree", "copy-tree");
        effect_scratch_empty(sh);
        let restored = sh.terminal_modes() == modes;
        sh.effect("modes", if restored { "restored" } else { "changed" });
        sh.line("/bin/echo native-fs-alive");
        sh.frame_rows_containing("alive", "native-fs-alive");
    }
);

parity!(
    native_fs_ctrl_z_is_ignored_during_copy_and_external_job_control_still_works,
    cancellable_tree().executable(
        "bin/sleep",
        "#!/bin/sh\nprintf 'pty-child-ready\\n'\nexec /bin/sleep 60\n",
    ),
    |sh: &mut Live| {
        let modes = sh.terminal_modes();
        let stage = start_staged_copy(sh);
        sh.send_now(key::CTRL_Z);
        sh.wait_prompt();
        let suspended = sh.screen_text().contains("stopped:");
        sh.effect("copy_suspended", if suspended { "suspended" } else { "not suspended" });
        sh.line("echo native-fs-status-$?");
        sh.frame_rows_containing("status", "native-fs-status-");
        sh.effect("stage", if stage.exists() { "left behind" } else { "removed" });
        sh.effect_file("copied", "copy-tree/file-00000");
        let restored = sh.terminal_modes() == modes;
        sh.effect("modes", if restored { "restored" } else { "changed" });
        sh.text("./bin/sleep");
        sh.send_now(key::ENTER);
        sh.wait_screen_contains("pty-child-ready");
        sh.keys(key::CTRL_Z);
        sh.wait_screen_contains("stopped:");
        sh.wait_prompt();
        sh.frame_rows_containing("stopped", "stopped:");
        sh.text("fg");
        sh.send_now(key::ENTER);
        sh.wait_screen_contains("resuming:");
        sh.frame_rows_containing("resuming", "resuming:");
        sh.keys_to_prompt(key::CTRL_C);
    }
);

fn signalled_copy(sh: &mut Live, signal: i32) {
    let stage = start_staged_copy(sh);
    sh.signal(signal);
    sh.expect_exit_restored("exit");
    sh.effect("stage", if stage.exists() { "left behind" } else { "removed" });
    effect_exists(sh, "source_retained", "source-tree/file-00000");
    effect_exists(sh, "copy_tree", "copy-tree");
    effect_scratch_empty(sh);
}

parity!(
    native_fs_sigterm_cancels_and_restores_the_terminal,
    cancellable_tree(),
    |sh: &mut Live| signalled_copy(sh, libc::SIGTERM)
);

parity!(
    native_fs_sighup_cancels_and_restores_the_terminal,
    cancellable_tree(),
    |sh: &mut Live| signalled_copy(sh, libc::SIGHUP)
);

parity!(
    native_fs_broken_stderr_cancels_bounded_work_and_keeps_the_prompt_usable,
    byte_tree(native_fs(), "source-tree", 1024).dir("copy-into"),
    |sh: &mut Live| {
        sh.line("cp --verbose --jobs 4 --into copy-into source-tree/* 2>/dev/full");
        sh.line("echo native-fs-status-$?");
        sh.frame("status");
        effect_scratch_empty(sh);
        sh.line("/bin/echo native-fs-after-output-error");
        sh.frame("after_output_error");
    }
);

parity!(
    command_lookup_classifies_ish_commands_with_empty_path,
    native_fs()
        .file("source", "external copy bytes")
        .dir("destination-directory"),
    |sh: &mut Live| {
        sh.line("export PATH=");
        sh.line("command -v cp");
        sh.line("echo native-fs-status-$?");
        sh.frame("command_v_cp");
        sh.line("command -V mv");
        sh.line("echo native-fs-status-$?");
        sh.frame("command_capital_v_mv");
        sh.line("command -v /bin/cp");
        sh.line("echo native-fs-status-$?");
        sh.frame("command_v_path");
        sh.line("/bin/cp source destination-directory");
        sh.line("echo native-fs-status-$?");
        sh.frame("external_cp");
        sh.effect_file("external_copy", "destination-directory/source");
    }
);

parity!(
    native_fs_rsync_interactive,
    native_fs().file("source", "rsync sync bytes").dir("dst-dir"),
    |sh: &mut Live| {
        sh.line("command -V rsync");
        sh.frame("command_v_rsync");
        sh.line("rsync source dst-dir");
        sh.line("echo native-fs-status-$?");
        sh.frame("rsync");
        sh.effect_file("synced", "dst-dir/source");
    }
);

parity!(
    rmrf_removes_a_directory,
    native_fs().file("victim/nested/file", "content"),
    |sh: &mut Live| {
        sh.line("rmrf victim/");
        sh.frame("rmrf");
        sh.line("echo status-$?");
        sh.frame("status");
        sh.effect_tree("victim", "victim");
        sh.effect_listing("home", "");
    }
);

parity!(
    native_handlers_refuse_child_execution_before_target_redirections,
    native_fs(),
    |sh: &mut Live| {
        for name in ["l", "c", "history"] {
            sh.line(&format!("command {name} >native-target | /bin/cat"));
            sh.frame(&format!("{name}_pipeline"));
            effect_exists(sh, &format!("{name}_pipeline_target"), "native-target");
            sh.line(&format!("echo $(command {name} >native-target)"));
            sh.frame(&format!("{name}_substitution"));
            effect_exists(sh, &format!("{name}_substitution_target"), "native-target");
        }
        sh.line("printf parent-safe | /bin/cat");
        sh.frame("parent_pipeline");
        sh.line("echo $(printf substitution-safe)");
        sh.frame("parent_substitution");
        std::fs::write(sh.home().join("visible-entry"), b"").expect("write visible entry");
        sh.line("l >native-parent");
        sh.frame("parent_listing");
        let listed = std::fs::read_to_string(sh.home().join("native-parent")).unwrap_or_default();
        let names: Vec<&str> = listed
            .lines()
            .filter_map(|row| row.split_whitespace().next())
            .collect();
        sh.effect("native_parent_names", &names.join("\n"));
    }
);

parity!(
    native_copy_uses_current_store_and_prefix_tmpdir_values,
    native_fs()
        .file("source/file", "selected scratch")
        .file("scratch/.keep", ""),
    |sh: &mut Live| {
        sh.line("set TMPDIR scratch");
        sh.line("TMPDIR=missing cp source refused-copy");
        sh.frame("refused");
        sh.line("echo native-fs-status-$?");
        sh.frame("refused_status");
        effect_exists(sh, "refused_copy", "refused-copy");
        sh.line("cp source accepted-copy");
        sh.line("echo native-fs-status-$?");
        sh.frame("accepted_status");
        sh.effect_file("accepted_copy", "accepted-copy/file");
        sh.line("/bin/sh -c 'printf %s \"$TMPDIR\"' >selected-tmpdir");
        sh.effect_file("selected_tmpdir", "selected-tmpdir");
    }
);
