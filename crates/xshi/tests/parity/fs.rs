//! Native `cp`, `mv`, `rsync` and `rmrf` scenarios from ish's `tests/pty.rs`.
//!
//! Every scenario runs the same commands against both shells and records the
//! visible terminal plus persistent file effects. A transfer large enough to
//! observe is waited on through the shared `.fdu-quarantine-v2-*` staging
//! directory both engines create under HOME.

use super::scenarios::parity;
use super::{Fixture, Live, key, run_scenario};

fn cancellable_tree() -> Fixture {
    let block = "m".repeat(1024 * 1024);
    let mut fixture = Fixture::new();
    for index in 0..512 {
        fixture = fixture.file(&format!("source-tree/payload-{index:03}"), &block);
    }
    for index in 0..2048 {
        fixture = fixture.file(&format!("source-tree/file-{index:05}"), "OOOOOOOO");
    }
    fixture
}

parity!(
    native_fs_copy_and_move_lists_keep_successes_and_continue_after_failures,
    Fixture::new()
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
    Fixture::new()
        .file(
            "exact-source",
            "\u{0}\u{ff}\r\n\u{0}*",
        )
        .file(
            "into-source",
            "\u{0}\u{ff}\r\n\u{0}*",
        )
        .file(
            "move-source",
            "\u{0}\u{ff}\r\n\u{0}*",
        )
        .file(
            "move-into-source",
            "\u{0}\u{ff}\r\n\u{0}*",
        )
        .dir("copy-into")
        .dir("move-into"),
    |sh: &mut Live| {
        sh.line("cp exact-source exact-target");
        sh.line("cp --into copy-into into-source");
        sh.line("mv move-source move-target");
        sh.line("mv --into move-into move-into-source");
        sh.frame("after_transfers");
        sh.effect_tree("copy_into", "copy-into");
        sh.effect_tree("move_into", "move-into");
    }
);

parity!(
    native_fs_multi_source_into_trailing_directory,
    Fixture::new()
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
    }
);

parity!(
    native_fs_redirections_prevent_failed_transfers_and_capture_stderr,
    Fixture::new().file("source", "redirected bytes\u{0}\u{ff}"),
    |sh: &mut Live| {
        sh.line("cp source blocked > absent-directory/output");
        sh.line("echo native-fs-status-$?");
        sh.line("cp missing never-created 2> copy-error");
        sh.line("echo native-fs-status-$?");
        sh.line("cp --verbose source verbose-copy 2> copy-verbose");
        sh.line("echo native-fs-status-$?");
        sh.effect_file("blocked_absent", "blocked");
        sh.effect_file("never_created_absent", "never-created");
        sh.effect_file("copy_error", "copy-error");
        sh.effect_file("copy_verbose", "copy-verbose");
        sh.effect_file("verbose_copy", "verbose-copy");
    }
);

parity!(
    native_fs_parent_placement_accepts_command_alias_expanded_name_and_argument_substitution,
    Fixture::new().file("source", "parent bytes\u{0}"),
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
    Fixture::new().file("source", "negated copy bytes"),
    |sh: &mut Live| {
        sh.line("! cp source negated-copy");
        sh.line("echo native-fs-status-$?");
        sh.line("! mv absent-source negated-move");
        sh.line("echo native-fs-status-$?");
        sh.line("! mv negated-copy negated-move-success");
        sh.line("echo native-fs-status-$?");
        sh.line("export NATIVE_FS_COMMAND=cp");
        sh.line(
            "! command command $NATIVE_FS_COMMAND negated-move-success nested-negated-copy > nested-negated-redirection",
        );
        sh.line("echo native-fs-status-$?");
        sh.line("alias negated-copy-command cp");
        sh.line("! negated-copy-command source alias-negated-copy");
        sh.line("echo native-fs-status-$?");
        sh.effect_file("negated_copy", "negated-copy");
        sh.effect_file("moved_success", "negated-move-success");
        sh.effect_file("nested_negated", "nested-negated-copy");
        sh.effect_file("alias_negated", "alias-negated-copy");
        sh.effect_file("negated_move_success", "negated-move-success");
        sh.effect_file("nested_redirection", "nested-negated-redirection");
    }
);

parity!(
    native_fs_static_dynamic_and_nested_fork_contexts_refuse_before_redirections,
    Fixture::new().file("source", "must stay untouched"),
    |sh: &mut Live| {
        sh.line("/bin/echo ignored | cp source static-copy > static-redirection");
        sh.line("echo native-fs-status-$?");
        sh.line("export NATIVE_FS_COMMAND=cp");
        sh.line(
            "/bin/echo ignored | $NATIVE_FS_COMMAND source dynamic-copy > dynamic-redirection",
        );
        sh.line("echo native-fs-status-$?");
        sh.line("echo \"$(cp source nested-copy > nested-redirection)\"");
        sh.line("echo native-fs-status-$?");
        sh.line("echo \"$(echo $(cp source deep-copy > deep-redirection))\"");
        sh.line("echo native-fs-status-$?");
        sh.effect_file("static_absent", "static-copy");
        sh.effect_file("dynamic_absent", "dynamic-copy");
        sh.effect_file("nested_absent", "nested-copy");
        sh.effect_file("deep_absent", "deep-copy");
        sh.effect_file("static_redirection_absent", "static-redirection");
        sh.effect_file("dynamic_redirection_absent", "dynamic-redirection");
    }
);

parity!(
    native_fs_quick_no_progress_copy_stays_quiet,
    Fixture::new().file("source", "tiny bytes"),
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
        sh.text("cp -r --jobs 1 source-tree copy-tree");
        sh.send_raw(b"\r");
        sh.wait_for_staging("ctrl-c copy");
        sh.keys(key::CTRL_C);
        sh.wait_prompt();
        sh.line("echo native-fs-status-$?");
        sh.line("/bin/echo native-fs-alive");
        sh.effect_listing("source_retained", "source-tree");
        sh.effect_listing("copy_absent", "copy-tree");
        sh.effect_listing("scratch", ".tmp");
    }
);

parity!(
    native_fs_ctrl_z_is_ignored_during_copy_and_external_job_control_still_works,
    cancellable_tree().executable(
        "bin/sleep",
        "#!/bin/sh\necho pty-child-ready\nsleep 60\n",
    ),
    |sh: &mut Live| {
        sh.text("cp -r --jobs 1 source-tree copy-tree");
        sh.send_raw(b"\r");
        sh.wait_for_staging("ctrl-z copy");
        sh.send_raw(key::CTRL_Z);
        sh.wait_prompt();
        sh.line("echo stopped-status-$?");
        sh.effect_listing("source_still_present", "source-tree");
        sh.effect_file("copy_kept", "copy-tree/file-00000");
        sh.text("./bin/sleep");
        sh.enter();
        sh.wait_screen_contains("pty-child-ready");
        sh.keys(key::CTRL_Z);
        sh.wait_screen_contains("stopped:");
        sh.wait_prompt();
        sh.text("fg");
        sh.enter();
        sh.wait_screen_contains("resuming:");
        sh.keys_to_prompt(key::CTRL_C);
    }
);

parity!(
    native_fs_sigterm_cancels_and_restores_the_terminal,
    cancellable_tree(),
    |sh: &mut Live| {
        sh.text("cp -r --jobs 1 source-tree copy-tree");
        sh.send_raw(b"\r");
        sh.wait_for_staging("sigterm copy");
        sh.signal(libc::SIGTERM);
        sh.expect_exit_restored("sigterm_exit");
        assert!(!sh.staging_entry_present(), "SIGTERM left a staging tree");
    }
);

parity!(
    native_fs_sighup_cancels_and_restores_the_terminal,
    cancellable_tree(),
    |sh: &mut Live| {
        sh.text("cp -r --jobs 1 source-tree copy-tree");
        sh.send_raw(b"\r");
        sh.wait_for_staging("sighup copy");
        sh.signal(libc::SIGHUP);
        sh.expect_exit_restored("sighup_exit");
        assert!(!sh.staging_entry_present(), "SIGHUP left a staging tree");
    }
);



