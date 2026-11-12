//! Remaining builtin, listing, lookup, and job-control scenarios from ish's
//! `tests/pty.rs` that the earlier scenario set did not replay.

use super::scenarios::parity;
use super::{Fixture, Live, key, run_scenario};

const READY_SLEEP: &str = "#!/bin/sh\necho ready\nsleep 60\n";
const READY_SLEEP_STDERR: &str = "#!/bin/sh\necho ready >&2\nsleep 60\n";

parity!(
    l_wraps_long_names_at_narrow_terminal_width,
    Fixture::new()
        .file("abcdefghijklmnopqrstuvwxyz0123456789.txt", "x")
        .file("short.txt", "y")
        .size(24, 40),
    |sh: &mut Live| {
        sh.line("l");
        sh.frame("after_l");
    }
);

parity!(
    l_hides_dotfiles,
    Fixture::new()
        .file("visible.txt", "")
        .file(".hidden", ""),
    |sh: &mut Live| {
        sh.line("l");
        sh.frame("after_l");
    }
);

parity!(
    ll_shows_dotfiles,
    Fixture::new()
        .file("visible.txt", "")
        .file(".hidden", ""),
    |sh: &mut Live| {
        sh.line("ll");
        sh.frame("after_ll");
    }
);

parity!(
    which_same_name_alias_shows_underlying_binary,
    Fixture::new().executable("bin/mytool", "#!/bin/sh\necho hi\n"),
    |sh: &mut Live| {
        sh.line("set PATH $HOME/bin");
        sh.line("alias mytool mytool --flag");
        sh.line("w mytool");
        sh.frame("after_w");
    }
);

parity!(
    set_refuses_option_and_positional_forms_without_redirection_effects,
    Fixture::new().file("set-target", "preserved"),
    |sh: &mut Live| {
        sh.line("set -e > set-target");
        sh.frame("rejected");
        sh.line("echo $?");
        sh.frame("status");
        sh.line("set 7 value");
        sh.frame("positional");
        sh.line("set NAME=value");
        sh.frame("assignment_name");
        sh.line("set EMPTY");
        sh.line("set OPTIONS -e +e --");
        sh.line("printf '[%s][%s]' \"$EMPTY\" \"$OPTIONS\"");
        sh.frame("allowed_values");
        sh.effect_file("set_target", "set-target");
    }
);

parity!(
    which_refuses_unsupported_posix_builtins,
    Fixture::new()
        .executable("bin/exec", "#!/bin/sh\nexit 0\n")
        .executable("bin/trap", "#!/bin/sh\nexit 0\n"),
    |sh: &mut Live| {
        sh.line("set PATH $HOME/bin");
        sh.line("w exec");
        sh.frame("w_exec");
        sh.line("which trap");
        sh.frame("which_trap");
        sh.line("type exec");
        sh.frame("type_exec");
        sh.line("echo $?");
        sh.frame("status");
    }
);

parity!(
    failed_builtins_update_question_status,
    Fixture::new().size(24, 200).dir("project"),
    |sh: &mut Live| {
        sh.line("cd missing-directory");
        sh.frame("failed_cd");
        sh.line("echo builtin-status-$?");
        sh.frame("cd_status");
        sh.line("z project && true");
        sh.frame("rejected");
        sh.line("echo rejected-status-$?");
        sh.frame("rejected_status");
    }
);

parity!(
    state_builtins_in_a_list_are_refused_without_running_anything,
    Fixture::new().size(24, 200),
    |sh: &mut Live| {
        sh.line("z nothing; echo ran");
        sh.frame("z_list");
        sh.line("fg && echo ran");
        sh.frame("fg_list");
    }
);

parity!(
    state_builtin_in_a_list_and_undocumented_syntax_are_refused_without_running_anything,
    Fixture::new().size(24, 200),
    |sh: &mut Live| {
        for (label, command) in [
            ("z_list", "z nothing; echo ran$((1+1))"),
            ("fg_list", "fg && echo ran$((1+1))"),
            ("background", "sleep 1 & echo ran$((1+1))"),
            ("if", "if true; then echo ran$((1+1)); fi"),
            ("for", "for i in a; do echo ran$((1+1)); done"),
            ("case", "case x in x) echo ran$((1+1));; esac"),
            ("function", "f() { echo ran$((1+1)); }"),
            ("eval", "eval echo ran$((1+1))"),
            ("exec", "exec echo ran$((1+1))"),
            ("parameter_operator", "echo ${HOME:-ran$((1+1))}"),
        ] {
            sh.line(command);
            sh.frame(label);
        }
        sh.line("/bin/echo ok${HOME}; echo ran$((1+1)) | cat");
        sh.frame("documented_syntax");
    }
);

parity!(
    unsupported_builtins_are_refused_without_target_effects,
    Fixture::new().file("refused-target", "preserved"),
    |sh: &mut Live| {
        for (label, command) in [
            ("trap", "trap ignored TERM >refused-target"),
            ("exec", "exec /bin/true >refused-target"),
            ("eval", "eval /bin/true >refused-target"),
            ("source", "source /dev/null >refused-target"),
            ("dot", ". /dev/null >refused-target"),
        ] {
            sh.line(command);
            sh.frame(label);
        }
        sh.line("echo $?");
        sh.frame("status");
        sh.line("echo alive");
        sh.frame("alive");
        sh.effect_file("refused-target", "refused-target");
    }
);

parity!(bg_is_not_a_builtin, Fixture::new(), |sh: &mut Live| {
    sh.line("bg");
    sh.frame("bg");
    sh.line("echo $?");
    sh.frame("status");
});

parity!(
    fg_drops_the_rest_of_a_stopped_and_or_list,
    Fixture::new().executable("bin/sleep", READY_SLEEP),
    |sh: &mut Live| {
        sh.line_until("./bin/sleep || /bin/touch forbidden-remainder", "ready");
        sh.keys_to_prompt(key::CTRL_Z);
        sh.line_until("fg", "resuming:");
        sh.keys_to_prompt(key::CTRL_C);
        sh.line("seq 1 40");
        sh.frame("after_scroll");
        sh.line("test -e forbidden-remainder; echo $?");
        sh.frame("remainder_status");
    }
);

parity!(
    stopped_pipeline_discards_the_remaining_list,
    Fixture::new().executable("bin/sleep", READY_SLEEP_STDERR),
    |sh: &mut Live| {
        sh.line_until(
            "./bin/sleep | true; /bin/touch forbidden-remainder",
            "ready",
        );
        sh.keys_to_prompt(key::CTRL_Z);
        sh.line_until("fg", "resuming:");
        sh.keys_to_prompt(key::CTRL_C);
        sh.line("seq 1 40");
        sh.frame("after_scroll");
        sh.line("test -e forbidden-remainder; echo $?");
        sh.frame("remainder_status");
    }
);

parity!(
    pipeline_suspend_and_resume_retains_every_stage,
    Fixture::new().executable("bin/sleep", READY_SLEEP),
    |sh: &mut Live| {
        sh.line_until("./bin/sleep | /bin/cat", "ready");
        sh.keys_to_prompt(key::CTRL_Z);
        sh.frame("stopped");
        sh.line_until("fg", "resuming:");
        sh.frame("resumed");
        sh.keys_to_prompt(key::CTRL_C);
        sh.frame("after_interrupt");
        sh.line("echo pipeline-alive");
        sh.frame("alive");
    }
);

parity!(
    pipeline_suspend_and_resume_keeps_job_owned,
    Fixture::new().executable("bin/sleep", READY_SLEEP),
    |sh: &mut Live| {
        sh.line_until("./bin/sleep | /bin/cat", "ready");
        sh.keys_to_prompt(key::CTRL_Z);
        sh.frame("stopped");
        sh.line_until("fg", "resuming:");
        sh.frame("resumed");
        sh.keys_to_prompt(key::CTRL_C);
        sh.frame("after_interrupt");
        sh.line("echo alive");
        sh.frame("alive");
    }
);

parity!(
    all_external_stopped_pipeline_discards_the_remaining_list,
    Fixture::new().executable("bin/sleep", READY_SLEEP),
    |sh: &mut Live| {
        sh.line_until(
            "./bin/sleep | /bin/cat; /bin/touch forbidden-remainder",
            "ready",
        );
        sh.keys_to_prompt(key::CTRL_Z);
        sh.line_until("fg", "resuming:");
        sh.keys_to_prompt(key::CTRL_C);
        sh.line("seq 1 40");
        sh.frame("after_scroll");
        sh.line("test -e forbidden-remainder; echo $?");
        sh.frame("remainder_status");
    }
);

parity!(
    ctrl_c_interrupts_running_child,
    Fixture::new().executable("bin/sleep", READY_SLEEP),
    |sh: &mut Live| {
        sh.line_until("./bin/sleep", "ready");
        sh.keys_to_prompt(key::CTRL_C);
        sh.frame("after_interrupt");
        sh.line("echo alive");
        sh.frame("alive");
    }
);

parity!(
    exit_twice_with_a_stopped_job_exits,
    Fixture::new().executable("bin/job", READY_SLEEP),
    |sh: &mut Live| {
        sh.line_until("./bin/job", "ready");
        sh.keys_to_prompt(key::CTRL_Z);
        sh.line("exit");
        sh.frame("first_exit");
        sh.text("exit");
        sh.enter();
        sh.expect_exit("exit_status");
    }
);

parity!(
    confirmed_exit_with_a_stopped_job_keeps_the_requested_status,
    Fixture::new().executable("bin/job", READY_SLEEP),
    |sh: &mut Live| {
        sh.line_until("./bin/job", "ready");
        sh.keys_to_prompt(key::CTRL_Z);
        sh.line("exit 3");
        sh.frame("first_exit");
        sh.text("exit 3");
        sh.enter();
        sh.expect_exit("exit_status");
    }
);

parity!(
    second_ctrl_z_is_refused_and_the_first_job_stays_resumable,
    Fixture::new().executable("bin/job", READY_SLEEP),
    |sh: &mut Live| {
        sh.line_until("./bin/job", "ready");
        sh.keys_to_prompt(key::CTRL_Z);
        sh.frame("stopped");
        sh.line_until(
            "sh -c 'kill -STOP $$; echo continued$((1+1))'",
            "continued2",
        );
        sh.frame("second_stop_refused");
        sh.line_until("fg", "resuming:");
        sh.frame("resumed");
        sh.keys_to_prompt(key::CTRL_C);
        sh.enter();
        sh.wait_prompt();
        sh.line("echo alive");
        sh.frame_rows_containing("alive", "alive");
    }
);

parity!(exit_expands_its_status_operand, Fixture::new(), |sh: &mut Live| {
    sh.line("false");
    sh.text("exit $?");
    sh.enter();
    sh.expect_exit("dollar_status");
    let mut quoted = sh.spawn_peer();
    quoted.text("exit '7'");
    quoted.enter();
    quoted.expect_exit("quoted_status");
    sh.absorb(quoted, "quoted");
});

// Wide enough that the refusal fits one row: the shell-name prefixes differ in
// length, so an 80-column hard wrap would split the normalized text differently.
parity!(
    exit_refuses_invalid_operands_without_exiting,
    Fixture::new().size(24, 120),
    |sh: &mut Live| {
        sh.line("exit abc; echo status=$?");
        sh.frame("in_a_list");
        sh.line("exit abc");
        sh.line("echo status=$?");
        sh.frame("non_numeric");
        sh.line("exit 1 2");
        sh.line("echo status=$?");
        sh.frame("too_many");
        sh.line("exit $((1/0))");
        sh.line("echo status=$?");
        sh.frame("expansion_error");
        sh.text("exit 4");
        sh.enter();
        sh.expect_exit("exit_status");
    }
);

parity!(
    pipeline_stage_that_stops_itself_suspends_the_whole_job,
    Fixture::new().executable("bin/nil", "#!/bin/sh\ncat >/dev/null\n"),
    |sh: &mut Live| {
        sh.line_until("sh -c 'kill -STOP $$' | ./bin/nil", "stopped:");
        sh.frame("stopped");
        sh.line_until("fg", "resuming:");
        sh.frame("resumed");
        sh.keys_to_prompt(key::CTRL_C);
        sh.frame("after_interrupt");
        sh.line("echo alive");
        sh.frame("alive");
    }
);

parity!(
    pipeline_status_tolerates_an_upstream_sigpipe,
    Fixture::new(),
    |sh: &mut Live| {
        sh.line("yes | head -1");
        sh.line("echo $?");
        sh.frame("sigpipe_status");
        sh.line("cat /dev/null | head -1");
        sh.line("echo $?");
        sh.frame("empty_upstream_status");
    }
);

parity!(
    pipeline_keeps_running_stages_when_a_later_stage_is_not_found,
    Fixture::new().file("data.txt", "payload\n"),
    |sh: &mut Live| {
        sh.line("cat data.txt | nosuch | cat");
        sh.frame("middle_missing");
        sh.line("echo $?");
        sh.frame("middle_status");
        sh.line("cat data.txt | nosuch");
        sh.frame("last_missing");
        sh.line("echo $?");
        sh.frame("last_status");
    }
);

parity!(
    forced_exit_with_a_stopped_pipeline_leaves_no_stage_behind,
    Fixture::new().executable(
        "bin/stages",
        "#!/bin/sh\necho $$ > first-pid\necho ready\nsleep 60\n",
    ),
    |sh: &mut Live| {
        sh.line_until("./bin/stages | cat", "ready");
        sh.keys_to_prompt(key::CTRL_Z);
        sh.frame("stopped");
        let pgid: libc::pid_t = std::fs::read_to_string(sh.home().join("first-pid"))
            .expect("the stopped stage wrote its pid")
            .trim()
            .parse()
            .expect("the stage pid is a number");
        // SAFETY: kill takes no pointers, and signal 0 only probes whether
        // any process in the group remains.
        let alive = || unsafe { libc::kill(-pgid, 0) } == 0;
        assert!(alive(), "the first stage {pgid} leads the stopped job's group");
        sh.text("exit");
        sh.keys_to_prompt(key::ENTER);
        sh.frame("first_exit");
        sh.text("exit");
        sh.enter();
        sh.expect_exit("exit_status");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while alive() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(!alive(), "a process of the stopped job {pgid} outlived the shell");
    }
);

parity!(
    pipeline_reports_a_missing_stage_while_the_job_runs,
    Fixture::new().executable(
        "bin/late",
        "#!/bin/sh\nsleep 0.5\necho ready\nsleep 60\n",
    ),
    |sh: &mut Live| {
        sh.line_until("nosuch | ./bin/late", "ready");
        sh.keys_to_prompt(key::CTRL_Z);
        sh.frame("stopped");
        sh.line_until("fg", "resuming:");
        sh.keys_to_prompt(key::CTRL_C);
        sh.frame("after_interrupt");
        sh.line("echo $?");
        sh.frame("status");
    }
);

parity!(
    fg_reports_a_stopped_pipeline_killed_from_outside,
    Fixture::new().executable("bin/nil", "#!/bin/sh\ncat >/dev/null\n"),
    |sh: &mut Live| {
        sh.line_until(
            "sh -c 'echo $$ > pgid; kill -STOP $$' | ./bin/nil",
            "stopped:",
        );
        sh.frame("stopped");
        sh.line("kill -KILL -$(cat pgid)");
        sh.line("echo still-$?");
        sh.frame("after_kill");
        sh.line_until("fg", "resuming:");
        sh.frame("resumed");
        sh.line("echo after-fg-$?");
        sh.frame("after_fg");
    }
);

parity!(
    a_stopped_pipeline_continued_from_outside_waits_for_fg,
    Fixture::new().executable("bin/nil", "#!/bin/sh\nsleep 30\n"),
    |sh: &mut Live| {
        sh.line_until(
            "sh -c 'echo $$ > pgid; kill -STOP $$' | ./bin/nil",
            "stopped:",
        );
        sh.frame("stopped");
        sh.line("kill -CONT -$(cat pgid)");
        sh.line("echo continued-$?");
        sh.frame("continued");
        sh.line_until("fg", "resuming:");
        sh.frame("resumed");
        sh.keys_to_prompt(key::CTRL_C);
        sh.frame("after_interrupt");
        sh.line("echo alive");
        sh.frame("alive");
    }
);
