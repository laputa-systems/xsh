//! History scenarios from ish's `tests/pty.rs` and
//! `tests/history_search_paste.rs` that the original port did not replay.

use super::scenarios::parity;
use super::{Fixture, Live, key, paste, run_scenario};

fn log_path(sh: &Live) -> std::path::PathBuf {
    sh.home()
        .join(format!("{}.log", sh.kind().history_file()))
}

parity!(
    history_up_ignores_later_global_entries,
    Fixture::new().history(&["echo startup"]),
    |sh: &mut Live| {
        let mut peer = sh.spawn_peer();
        peer.line("echo later_global");
        sh.enter();
        sh.wait_prompt();
        sh.keys(key::UP);
        sh.frame("up_shows_startup");
        sh.keys(key::CTRL_U);
        sh.keys(key::CTRL_R);
        sh.text("later");
        sh.frame("search_finds_later");
        sh.keys(key::ESC);
        sh.absorb(peer, "peer");
    }
);

parity!(
    history_survives_abrupt_termination_and_restart,
    Fixture::new(),
    |sh: &mut Live| {
        sh.line("/bin/echo durable_before_kill");
        sh.keys(b"/bin/sh -c 'kill -KILL $PPID'");
        sh.enter();
        sh.expect_exit("killed");
        let mut restarted = sh.spawn_peer();
        restarted.keys(key::CTRL_R);
        restarted.text("durable_before_kill");
        restarted.frame("search");
        restarted.keys(key::ENTER);
        restarted.frame("accepted");
        restarted.keys_to_prompt(key::ENTER);
        restarted.frame("executed");
        sh.absorb(restarted, "restarted");
    }
);

parity!(
    history_recovers_partial_append_after_abrupt_termination,
    Fixture::new(),
    |sh: &mut Live| {
        sh.line("/bin/echo durable_before_partial_append");
        let log = log_path(sh);
        sh.keys(
            format!(
                "/bin/sh -c 'printf \"\\001\\002\\003\" >> {}; kill -KILL $PPID'",
                log.display()
            )
            .as_bytes(),
        );
        sh.enter();
        sh.expect_exit("killed");
        let mut restarted = sh.spawn_peer();
        restarted.line("/bin/echo durable_after_partial_append");
        restarted.frame("saved_after_recovery");
        restarted.keys(key::CTRL_R);
        restarted.text("durable_before_partial_append");
        restarted.frame("search_before");
        restarted.keys(key::ESC);
        sh.absorb(restarted, "restarted");
    }
);

parity!(
    history_shared_shell_processes_keep_all_commands,
    Fixture::new(),
    |sh: &mut Live| {
        let mut first = sh.spawn_peer();
        let mut second = sh.spawn_peer();
        for item in 0..2 {
            first.line(&format!("/bin/echo first_process_{item}"));
            second.line(&format!("/bin/echo second_process_{item}"));
        }
        first.text("exit");
        first.enter();
        first.expect_exit("first_exit");
        second.line("/bin/echo second_process_after_exit");
        second.keys(key::CTRL_R);
        second.text("first_process_1");
        second.frame("peer_search");
        second.keys(key::ESC);
        let mut third = sh.spawn_peer();
        third.line("history");
        third.frame("history_listing");
        sh.absorb(first, "first");
        sh.absorb(second, "second");
        sh.absorb(third, "third");
    }
);

parity!(
    history_write_failure_is_visible_and_command_still_runs,
    Fixture::new(),
    |sh: &mut Live| {
        let log = log_path(sh);
        let saved = sh.home().join("saved-history.log");
        std::fs::rename(&log, &saved).expect("move the history log");
        std::fs::create_dir(&log).expect("block the history log");
        sh.line("printf 'executed_%s\\n' despite_failure");
        sh.frame("failure_and_output");
    }
);

parity!(
    history_read_only_commands_preserve_another_writer,
    Fixture::new(),
    |sh: &mut Live| {
        let mut writer = sh.spawn_peer();
        writer.line("echo concurrent_history_marker");
        sh.line("history");
        sh.frame("listing");
        sh.line("history > history-output");
        sh.line("cat history-output");
        sh.frame("redirected");
        sh.absorb(writer, "writer");
    }
);

parity!(
    history_readers_keep_startup_log_after_home_changes,
    Fixture::new()
        .dir("new-home")
        .history(&["echo startup_log_marker"]),
    |sh: &mut Live| {
        sh.line(&format!(
            "set HOME {}",
            sh.home().join("new-home").display()
        ));
        sh.line("history");
        sh.frame("listing");
        sh.line("history > history-after-home-change");
        sh.line("cat history-after-home-change");
        sh.frame("redirected");
    }
);

parity!(
    history_reset_refuses_pipeline_and_command_substitution,
    Fixture::new().history(&["echo preserved_history_marker"]),
    |sh: &mut Live| {
        for (label, command) in [
            ("pipeline", "history reset | /bin/cat"),
            ("substitution", "echo $(history reset)"),
        ] {
            sh.line(command);
            sh.frame(label);
        }
        sh.line("history");
        sh.frame("preserved");
    }
);

parity!(
    pasted_history_query_filters_and_executes_selected_command,
    Fixture::new().history(&["echo alpha", "echo beta"]),
    |sh: &mut Live| {
        sh.keys(key::CTRL_R);
        sh.frame("search_open");
        sh.keys(&paste("alpha"));
        sh.frame("pasted_query");
        sh.keys(key::ENTER);
        sh.frame("accepted");
        sh.keys_to_prompt(key::ENTER);
        sh.frame("executed");
    }
);

parity!(
    pasted_history_query_edits_at_cursor_and_restores_candidates_after_delete,
    Fixture::new().history(&["echo alpha", "echo beta"]),
    |sh: &mut Live| {
        sh.keys(key::CTRL_R);
        sh.text("alha");
        sh.keys(key::CTRL_A);
        sh.keys(key::RIGHT);
        sh.keys(key::RIGHT);
        sh.keys(&paste("p"));
        sh.frame("pasted_at_cursor");
        sh.keys(key::CTRL_E);
        sh.keys(key::CTRL_U);
        sh.text("beta");
        sh.frame("deleted_paste");
        sh.keys(key::ENTER);
        sh.frame("accepted");
    }
);

parity!(
    pasted_history_query_resets_selection_after_narrowing,
    Fixture::new().history(&["echo alpha1", "echo alpha2", "echo beta"]),
    |sh: &mut Live| {
        sh.keys(key::CTRL_R);
        sh.text("alpha");
        sh.frame("alpha_candidates");
        sh.keys(key::DOWN);
        sh.keys(&paste("1"));
        sh.frame("narrowed");
        sh.keys(key::ENTER);
        sh.frame("accepted");
    }
);
