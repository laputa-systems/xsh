//! ish PTY tests that the original port left without a parity scenario. Each
//! one keeps the reference test's name so the coverage table stays mechanical;
//! the script is adapted to the driver's prompt/Oscilloscope synchronization.

use super::scenarios::parity;
use super::{Fixture, Live, key, run_scenario};

const READY_SLEEP: &str = "#!/bin/sh\necho ready\nsleep 60\n";

parity!(
    approved_builtin_classes_remain_available,
    Fixture::new().dir("sub"),
    |sh: &mut Live| {
        for (label, line) in [
            ("echo", "echo output"),
            ("printf", "printf '%s' output"),
            ("pwd", "pwd"),
            ("true", "true"),
            ("false", "false"),
            ("colon", ":"),
            ("test", "test 1 = 1"),
            ("bracket", "[ 1 = 1 ]"),
            ("kill", "kill -0 $$"),
            ("export", "export POLICY_VAR=value"),
            ("unset", "unset POLICY_VAR"),
            ("set", "set POLICY_VAR -value +value"),
            ("command", "command -p -- printf output"),
            ("cd", "cd sub && pwd"),
            ("history", "history; echo listed"),
        ] {
            sh.line(line);
            sh.frame(label);
        }
    }
);

parity!(
    builtin_lookup_completion_and_explicit_paths_share_the_policy,
    Fixture::new()
        .executable("bin/exec", "#!/bin/sh\nprintf explicit-exec\nprintf entered >explicit-marker\n")
        .executable("bin/trap", "#!/bin/sh\nprintf explicit-trap\n"),
    |sh: &mut Live| {
        sh.line("set PATH $HOME/bin");
        for (label, query) in [
            ("command_v_exec", "command -v exec >query-output 2>query-error"),
            ("command_v_trap", "command -V trap >query-output 2>query-error"),
        ] {
            sh.line(query);
            sh.frame(label);
            sh.line("echo $?");
            sh.frame("status");
            sh.line("wc -c < query-output");
            sh.frame("query_output_size");
        }
        for (label, query) in [
            ("w_exec", "w exec"),
            ("which_trap", "which trap"),
            ("type_exec", "type exec"),
        ] {
            sh.line(query);
            sh.frame(label);
            sh.line("echo $?");
            sh.frame("status");
        }
        sh.line("command -v exit >supported-lookup");
        sh.line("cat supported-lookup");
        sh.frame("supported_lookup");
        sh.line("command exit >refused-exit");
        sh.line("echo $?");
        sh.frame("command_exit_status");
        sh.line("test -e refused-exit; echo $?");
        sh.frame("refused_exit_absent");
        sh.text("exe");
        sh.keys(key::TAB);
        sh.frame("completion_hides_refused");
        sh.keys_to_prompt(key::CTRL_C);
        sh.text("ech");
        sh.keys(key::TAB);
        sh.frame("completion_shows_approved");
        sh.keys_to_prompt(key::CTRL_C);
        sh.line("./bin/exec");
        sh.effect_file("explicit_marker", "explicit-marker");
        sh.line("mkdir exec");
        sh.line("set PATH");
        sh.line("exec");
        sh.line("echo $?");
        sh.frame("refused_exec_status");
        sh.line("pwd >cwd-after-refusal");
        sh.effect_file("cwd_after_refusal", "cwd-after-refusal");
    }
);

parity!(
    path_binary_wins_over_implicit_cd,
    Fixture::new()
        .dir("foo")
        .executable("bin/foo", "#!/bin/sh\necho path-foo-ran\n"),
    |sh: &mut Live| {
        sh.line("export PATH=$HOME/bin");
        sh.line("foo");
        sh.frame("after_foo");
        sh.line("pwd");
        sh.frame("after_pwd");
    }
);

parity!(
    external_commands_run_in_the_directory_a_list_cd_selected,
    Fixture::new().dir("sub"),
    |sh: &mut Live| {
        sh.line("cd sub && /bin/pwd");
        sh.frame("after_list_cd");
        sh.line("cd .. && sh -c 'pwd'");
        sh.frame("after_list_sh");
    }
);

parity!(
    external_input_redirections_preserve_foreground_terminal_ownership,
    Fixture::new().file("input", "redirected-input\n"),
    |sh: &mut Live| {
        sh.line("/bin/cat <input");
        sh.frame("after_redirected_cat");
    }
);

parity!(
    closed_input_redirection_keeps_terminal_ownership,
    Fixture::new().file("input", "redirected-input\n"),
    |sh: &mut Live| {
        sh.line("/bin/sh -c 'exit 0' 0<&-");
        sh.line("echo $?");
        sh.frame("closed_input_status");
    }
);

parity!(
    ordinary_exit_148_does_not_create_a_suspended_job,
    Fixture::new().executable("bin/exit148", "#!/bin/sh\nexit 148\n"),
    |sh: &mut Live| {
        sh.line("./bin/exit148");
        sh.line("fg");
        sh.frame("after_fg");
        sh.line("echo $?");
        sh.frame("status");
    }
);

parity!(
    multiline_preserves_unicode_quotes_and_backslash_newlines,
    Fixture::new(),
    |sh: &mut Live| {
        sh.text("printf '%s' 'é");
        sh.keys(key::ENTER);
        sh.text("日本\\");
        sh.keys(key::ENTER);
        sh.text("語' > quoted");
        sh.keys_to_prompt(key::ENTER);
        sh.effect_file("quoted", "quoted");

        sh.text("printf '%s' é\\");
        sh.keys(key::ENTER);
        sh.text("語 > escaped");
        sh.keys_to_prompt(key::ENTER);
        sh.effect_file("escaped", "escaped");
    }
);

parity!(
    interactive_final_history_replay_restores_the_recorded_directory,
    Fixture::new()
        .file("parent space/child/.keep", "")
        .cwd("parent space"),
    |sh: &mut Live| {
        sh.line("cd child");
        sh.keys(key::UP);
        sh.frame("recalled");
        sh.keys_to_prompt(key::ENTER);
        sh.frame("replayed");
        sh.line("pwd > replay-pwd");
        sh.effect_file("replay_pwd", "replay-pwd");
    }
);

parity!(
    interactive_final_history_replay_in_a_plain_directory,
    Fixture::new().file("parent/child/.keep", "").cwd("parent"),
    |sh: &mut Live| {
        sh.line("cd child");
        sh.keys(key::UP);
        sh.keys_to_prompt(key::ENTER);
        sh.line("pwd > replay-pwd");
        sh.effect_file("replay_pwd", "replay-pwd");
    }
);

parity!(
    interactive_final_z_matches_quoted_directory_history,
    Fixture::new().dir("project space"),
    |sh: &mut Live| {
        sh.line("cd 'project space'");
        sh.line("cd ..");
        sh.line("z project");
        sh.frame("after_z");
        sh.line("pwd > z-pwd");
        sh.effect_file("z_pwd", "z-pwd");
    }
);

parity!(
    prompt_identity_displays_unicode_and_replaces_only_invalid_display_bytes,
    Fixture::new(),
    |sh: &mut Live| {
        sh.line("USER='\u{e080}'");
        sh.frame("private_use_user");
        sh.line("USER=$(printf '\\200')");
        sh.frame("invalid_user");
        sh.line("USER=testuser");
        sh.frame("restored_user");
    }
);

parity!(
    prompt_identity_displays_unicode_user,
    Fixture::new(),
    |sh: &mut Live| {
        sh.line("USER='Zoë\u{e080}'");
        sh.frame("unicode_user");
        sh.line("USER=$(printf '更新\\n\\033[2J')");
        sh.frame("control_user");
        sh.line("USER=testuser");
        sh.frame("restored_user");
    }
);

parity!(
    prompt_refreshes_negative_git_lookup_after_repository_creation,
    Fixture::new().dir("repo"),
    |sh: &mut Live| {
        sh.line("mkdir -p repo/.git");
        sh.line("printf 'ref: refs/heads/late-branch\\n' > repo/.git/HEAD");
        sh.line("cd repo");
        sh.frame("after_cd");
    }
);

parity!(
    prompt_refreshes_negative_git_lookup_after_ancestor_repository_creation,
    Fixture::new().dir("ancestor/child"),
    |sh: &mut Live| {
        sh.line("mkdir -p ancestor/.git");
        sh.line("printf 'ref: refs/heads/ancestor-branch\\n' > ancestor/.git/HEAD");
        sh.line("cd ancestor/child");
        sh.frame("after_cd");
    }
);

parity!(
    external_commands_restore_terminal_signals_in_every_context,
    Fixture::new(),
    |sh: &mut Live| {
        let status = "/bin/grep -E 'SigIgn:|SigCgt:' /proc/self/status";
        sh.line(status);
        sh.frame("direct");
        sh.line(&format!("{status} | /bin/cat"));
        sh.frame("pipeline");
        sh.line(&format!("true && {status}"));
        sh.frame("list");
        sh.line(&format!("echo $({status})"));
        sh.frame("substitution");
    }
);

parity!(
    converged_command_reads_from_dev_tty,
    Fixture::new(),
    |sh: &mut Live| {
        sh.text("/bin/sh -c 'IFS= read -r x </dev/tty; printf TTY:%s \"$x\"'");
        sh.keys(key::ENTER);
        sh.keys_to_prompt(b"value\r");
        sh.frame("after_tty_read");
    }
);

parity!(
    single_command_sees_one_terminal_for_all_standard_streams,
    Fixture::new(),
    |sh: &mut Live| {
        sh.line("sh -c 'tty <&0; tty <&1; tty <&2'");
        sh.frame("after_tty");
    }
);

parity!(
    output_beyond_the_capture_limit_still_reaches_the_terminal_and_the_shell_continues,
    Fixture::new(),
    |sh: &mut Live| {
        sh.line_until("seq 1 400000", "400000");
        sh.wait_prompt();
        sh.line("echo alive");
        sh.frame_rows_containing("alive", "alive");
    }
);

parity!(
    killed_child_in_raw_mode_does_not_leak_into_the_next_command,
    Fixture::new(),
    |sh: &mut Live| {
        sh.line("sh -c 'stty raw -echo; kill -KILL $$'");
        sh.frame("after_killed_child");
        sh.line("echo alive");
        sh.frame("alive");
    }
);

parity!(
    git_log_pages_on_a_converged_terminal,
    Fixture::new().file(
        "repo/big.txt",
        &(0..200)
            .map(|index| format!("line {index:03}"))
            .collect::<Vec<_>>()
            .join("\n"),
    ),
    |sh: &mut Live| {
        sh.line("git init -q repo");
        sh.line("cd repo");
        sh.line("git add big.txt");
        sh.line("git -c user.email=test@example.com -c user.name=test commit -qm add");
        sh.line_until("git log -p", "line 000");
        sh.keys(b"q");
        sh.wait_prompt();
        sh.line("echo alive");
        sh.frame_rows_containing("alive", "alive");
    }
);

parity!(
    interactive_final_typeahead_after_enter_survives_the_next_prompt,
    Fixture::new(),
    |sh: &mut Live| {
        sh.send_raw(b"echo first > first | cat\recho second > second\recho $((12345+67890))\r");
        sh.wait_screen_contains("80235");
        sh.frame_rows_containing("typeahead_output", "80235");
        sh.effect_file("first", "first");
        sh.effect_file("second", "second");
    }
);

parity!(
    ctrl_p_dump_is_private_and_omits_typed_text,
    Fixture::new(),
    |sh: &mut Live| {
        sh.text("secret-token-123");
        sh.keys_silent(key::CTRL_P);
        sh.keys_to_prompt(key::CTRL_C);
        sh.frame("after_ctrl_c");
        sh.effect_dumps("dumps");
        sh.effect_dump_privacy("dump_privacy");
    }
);

parity!(
    osc7_working_directory_emitted_to_interactive_terminal,
    Fixture::new(),
    |sh: &mut Live| {
        sh.line("true");
        sh.effect_raw_count("osc7_reports", b"\x1b]7;file://");
    }
);

parity!(
    source_is_refused,
    Fixture::new(),
    |sh: &mut Live| {
        sh.line("source foo.sh");
        sh.frame("after_source");
    }
);

parity!(
    expanded_targets_obey_ish_execution_policy,
    Fixture::new()
        .file("source-target.sh", "#!/bin/sh\n/usr/bin/touch source-marker\n")
        .file("dot-target.sh", "#!/bin/sh\n/usr/bin/touch dot-marker\n"),
    |sh: &mut Live| {
        for (label, line) in [
            (
                "eval_target",
                "set TARGET eval; $TARGET /usr/bin/touch eval-marker",
            ),
            (
                "source_target",
                "set TARGET source; $TARGET source-target.sh",
            ),
            ("dot_target", "set TARGET .; $TARGET dot-target.sh"),
            (
                "command_target",
                "set TARGET eval; command $TARGET /usr/bin/touch command-marker",
            ),
        ] {
            sh.line(line);
            sh.frame(label);
            sh.line("echo policy-status-$?");
            sh.frame("status");
        }
        sh.line("test -e eval-marker -o -e source-marker -o -e dot-marker -o -e command-marker; echo $?");
        sh.frame("markers_absent");
        sh.line("set TARGET eval; $TARGET /usr/bin/touch redirection-target > redirection-file");
        sh.line("echo policy-status-$?");
        sh.frame("redirection_refused");
        sh.line("test -e redirection-target -o -e redirection-file; echo $?");
        sh.frame("redirection_absent");
        sh.line("set TARGET eval; echo $(echo $(command $TARGET /usr/bin/touch nested-marker))");
        sh.line("test -e nested-marker; echo $?");
        sh.frame("nested_absent");
        sh.line("set TARGET exit; $TARGET 23");
        sh.line("echo policy-status-$?");
        sh.frame("exit_target_status");
        sh.line("set TARGET touch; $TARGET ordinary-command-marker");
        sh.line("test -e ordinary-command-marker; echo $?");
        sh.frame("ordinary_target_runs");
    }
);

parity!(
    alternate_screen_sequences_pass_through_relay,
    Fixture::new(),
    |sh: &mut Live| {
        sh.line("/usr/bin/printf '\\033[?1049hALT\\033[?1049l'");
        sh.effect_raw_count("alternate_screen_enter", b"\x1b[?1049h");
        sh.effect_raw_count("alternate_screen_leave", b"\x1b[?1049l");
        sh.line("echo restored");
        sh.frame("restored");
    }
);

parity!(
    foreground_terminal_mode_changes_on_stdout_apply_to_stdin,
    Fixture::new().executable(
        "bin/terminal-input",
        "#!/bin/sh\nstty -icanon -echo <&1\nprintf 'input-ready\\n'\nkey=$(dd bs=1 count=1 2>/dev/null)\nstty sane <&1\nprintf 'received:%s\\n' \"$key\"\n",
    ),
    |sh: &mut Live| {
        sh.line_until("./bin/terminal-input", "input-ready");
        sh.keys(b"x");
        sh.wait_screen_contains("received:x");
        sh.wait_prompt();
        sh.frame("after_input");
    }
);

parity!(
    sigint_during_editing_cancels_the_line,
    Fixture::new(),
    |sh: &mut Live| {
        sh.text("echo this-line-is-cancelled");
        sh.signal(libc::SIGINT);
        sh.wait_prompt();
        sh.line("echo sigint-cancel-marker");
        sh.frame_rows_containing("after_marker", "sigint-cancel-marker");
    }
);

parity!(
    interactive_final_sigterm_interrupts_oversized_paste_drain,
    Fixture::new(),
    |sh: &mut Live| {
        let mut payload = Vec::with_capacity(6 + 9 * 1024);
        payload.extend_from_slice(key::PASTE_START);
        payload.extend(std::iter::repeat_n(b'x', 9 * 1024));
        sh.send_raw(&payload);
        sh.signal(libc::SIGTERM);
        sh.expect_exit_restored("exit_status");
    }
);

parity!(
    sigterm_and_sighup_restore_the_terminal_and_exit_with_the_signal_status,
    Fixture::new(),
    |sh: &mut Live| {
        sh.text("/bin/sh -c 'kill -TERM $PPID'");
        sh.send_raw(b"\r");
        sh.expect_exit_restored("exit_status");
    }
);

parity!(
    shell_sighup_at_prompt_exits_and_restores_terminal,
    Fixture::new(),
    |sh: &mut Live| {
        sh.line("true");
        sh.signal(libc::SIGHUP);
        sh.expect_exit_restored("exit_status");
    }
);

const WIDE: u16 = 200;

parity!(
    denv_rejects_parent_helpers_after_parent_revocation_or_edit,
    Fixture::new()
        .size(24, WIDE)
        .file(
            "parent/.envrc",
            "export PARENT_VALUE=old\nprintf touched > parent-marker\n",
        )
        .file(
            "parent/project/.envrc",
            "export CANDIDATE=before\nsource_up || true\nexport CANDIDATE=after\n",
        )
        .allow_envrc("parent/.envrc")
        .allow_envrc("parent/project/.envrc"),
    |sh: &mut Live| {
        sh.line("cd parent");
        sh.line("denv deny");
        sh.line("rm -f parent-marker");
        sh.line("cd ..");
        sh.line("cd parent/project");
        sh.frame("after_cd");
        sh.line("echo =$PARENT_VALUE=:$CANDIDATE=:$__DENV_DIRTY=");
        sh.frame("state");
        sh.effect_listing("project_entries", "parent/project");
        sh.effect_listing("parent_entries", "parent");
    }
);

parity!(
    denv_rejects_parent_helpers_when_parent_is_untrusted,
    Fixture::new()
        .size(24, WIDE)
        .file(
            "parent/.envrc",
            "export PARENT_VALUE=old\nprintf touched > parent-marker\n",
        )
        .file(
            "parent/project/.envrc",
            "export CANDIDATE=before\nsource_up || true\nexport CANDIDATE=after\n",
        )
        .allow_envrc("parent/project/.envrc"),
    |sh: &mut Live| {
        sh.line("cd parent/project");
        sh.frame("after_cd");
        sh.line("echo =$PARENT_VALUE=:$CANDIDATE=:$__DENV_DIRTY=");
        sh.frame("state");
        sh.effect_listing("project_entries", "parent/project");
        sh.effect_listing("parent_entries", "parent");
    }
);

parity!(
    denv_rejects_parent_helpers_after_parent_edit,
    Fixture::new()
        .size(24, WIDE)
        .file(
            "parent/.envrc",
            "export PARENT_VALUE=old\nprintf touched > parent-marker\n",
        )
        .file(
            "parent/project/.envrc",
            "export CANDIDATE=before\nsource_up || true\nexport CANDIDATE=after\n",
        )
        .allow_envrc("parent/.envrc")
        .allow_envrc("parent/project/.envrc"),
    |sh: &mut Live| {
        sh.line("cd parent");
        sh.line("rm -f parent-marker");
        sh.rewrite_file(
            "parent/.envrc",
            "export PARENT_VALUE=changed\nprintf touched > parent-marker\n",
            10,
        );
        sh.line("cd ..");
        sh.line("cd parent/project");
        sh.frame("after_cd");
        sh.line("echo =$PARENT_VALUE=:$CANDIDATE=:$__DENV_DIRTY=");
        sh.frame("state");
        sh.effect_listing("project_entries", "parent/project");
        sh.effect_listing("parent_entries", "parent");
    }
);

fn read_home(sh: &Live, relative: &str) -> Vec<u8> {
    std::fs::read(sh.home().join(relative)).expect("read fixture output")
}

fn shells_to_check() -> Vec<(super::ShellKind, std::path::PathBuf)> {
    let mut shells = vec![(
        super::ShellKind::Xshi,
        std::path::PathBuf::from(env!("CARGO_BIN_EXE_xshi")),
    )];
    if let Some(ish) = super::ish_binary() {
        shells.push((super::ShellKind::Ish, ish));
    }
    shells
}

#[test]
fn fatal_crash_signals_restore_the_terminal_and_exit_with_the_signal_status() {
    for signal in [
        libc::SIGABRT,
        libc::SIGFPE,
        libc::SIGILL,
        libc::SIGTRAP,
        libc::SIGSEGV,
        libc::SIGBUS,
    ] {
        for (kind, binary) in shells_to_check() {
            let mut live = Live::spawn(kind, &binary, &Fixture::new());
            live.text("echo about-to-crash");
            live.signal(signal);
            let deadline = live.term.deadline(std::time::Duration::from_secs(5));
            let status = match live.term.wait_for_exit(deadline) {
                Ok(status) => status,
                Err(error) => panic!("{} did not exit on signal {signal}: {error}", kind.label()),
            };
            assert_eq!(
                status,
                ptytest::ExitStatus::Code(128 + signal),
                "{} signal {signal}",
                kind.label()
            );
            live.term
                .assert_terminal_restored(&live.baseline)
                .unwrap_or_else(|error| {
                    panic!("{} terminal not restored: {error:?}", kind.label())
                });
        }
    }
}

#[test]
fn occupied_job_slot_restores_new_job_terminal_modes_before_continuing() {
    for (kind, binary) in shells_to_check() {
        let fixture = Fixture::new().executable("bin/sleep", READY_SLEEP);
        let mut sh = Live::spawn(kind, &binary, &fixture);
        sh.line_until("./bin/sleep", "ready");
        sh.keys_to_prompt(key::CTRL_Z);
        sh.line("stty -g > baseline");
        sh.line("sh -c 'stty raw -echo; stty -g >raw-before; kill -STOP $$; stty -g >raw-after; stty sane'");
        assert_eq!(
            read_home(&sh, "raw-before"),
            read_home(&sh, "raw-after"),
            "{} restored the new job's modes before continuing",
            kind.label()
        );
        sh.line("stty -g > after");
        assert_eq!(
            read_home(&sh, "baseline"),
            read_home(&sh, "after"),
            "{} prompt modes changed after the second stop",
            kind.label()
        );
        sh.line_until("fg", "resuming:");
        sh.keys_to_prompt(key::CTRL_C);
        sh.line("echo alive");
    }
}

#[test]
fn stopped_child_in_raw_mode_restores_baseline_then_fg_restores_its_own_mode() {
    for (kind, binary) in shells_to_check() {
        let mut sh = Live::spawn(kind, &binary, &Fixture::new());
        sh.line("stty -g > baseline");
        sh.line("sh -c 'stty raw -echo; kill -STOP $$'");
        sh.line("stty -g > at-stop");
        assert_eq!(
            read_home(&sh, "baseline"),
            read_home(&sh, "at-stop"),
            "{} let the stopped child's raw mode reach the prompt",
            kind.label()
        );
        sh.line("fg");
        sh.line("stty -g > after-fg");
        assert_eq!(
            read_home(&sh, "baseline"),
            read_home(&sh, "after-fg"),
            "{} let the resumed child's raw mode reach the next command",
            kind.label()
        );
    }
}

#[test]
fn command_mode_matches_the_reference_shell() {
    let xshi = std::path::PathBuf::from(env!("CARGO_BIN_EXE_xshi"));
    let outputs = |binary: &std::path::Path| {
        let run = |args: &[&str], stdin: Option<&[u8]>| {
            let mut command = std::process::Command::new(binary);
            command
                .args(args)
                .env("HOME", std::env::temp_dir())
                .env("USER", "testuser");
            match stdin {
                Some(input) => {
                    use std::io::Write as _;
                    command.stdin(std::process::Stdio::piped());
                    command.stdout(std::process::Stdio::piped());
                    let mut child = command.spawn().expect("spawn shell");
                    child
                        .stdin
                        .as_mut()
                        .expect("stdin")
                        .write_all(input)
                        .expect("write stdin");
                    let output = child.wait_with_output().expect("wait for shell");
                    (output.status.code(), output.stdout)
                }
                None => {
                    let output = command.output().expect("run shell");
                    (output.status.code(), output.stdout)
                }
            }
        };
        vec![
            run(&["-c", "printf 'login-shell-contract'"], None),
            run(&["-c", "exit 37"], None),
            run(&["-c", "cat"], Some(b"piped through\n")),
        ]
    };
    let expected = vec![
        (Some(0), b"login-shell-contract".to_vec()),
        (Some(37), Vec::new()),
        (Some(0), b"piped through\n".to_vec()),
    ];
    assert_eq!(outputs(&xshi), expected, "xshi command mode");
    if let Some(ish) = super::ish_binary() {
        assert_eq!(outputs(&ish), expected, "ish command mode");
    }

    let path = std::env::temp_dir().join(format!("xshi-command-{}", std::process::id()));
    std::fs::create_dir_all(&path).expect("create command path directory");
    let config = path.join("config.ish");
    std::fs::write(&config, "this is not a command\n").expect("write command path file");
    let ish = super::ish_binary();
    let binaries: Vec<&std::path::PathBuf> =
        std::iter::once(&xshi).chain(ish.as_ref()).collect();
    for binary in &binaries {
        let output = std::process::Command::new(binary)
            .arg("-c")
            .arg(format!("printf '%s' {}", config.display()))
            .env("HOME", &path)
            .output()
            .expect("run shell command mode");
        assert_eq!(output.status.code(), Some(0));
        assert!(String::from_utf8_lossy(&output.stdout).ends_with("config.ish"));
    }
    std::fs::remove_dir_all(&path).ok();

    for binary in &binaries {
        for missing in [&["-c"][..], &["--config"][..]] {
            let output = std::process::Command::new(binary)
                .args(missing)
                .env("HOME", std::env::temp_dir())
                .output()
                .expect("run shell with a missing option argument");
            assert!(
                output.status.code().is_some_and(|code| code != 0),
                "{} {missing:?} accepted",
                binary.display()
            );
            assert!(!output.stderr.is_empty());
        }
    }
}

#[test]
fn script_mode_refused() {
    let xshi = std::path::PathBuf::from(env!("CARGO_BIN_EXE_xshi"));
    let output = std::process::Command::new(&xshi)
        .arg("script.sh")
        .env("HOME", std::env::temp_dir())
        .output()
        .expect("run xshi");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("interactive-only"),
        "{:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    if let Some(ish) = super::ish_binary() {
        let output = std::process::Command::new(ish)
            .arg("script.sh")
            .env("HOME", std::env::temp_dir())
            .output()
            .expect("run ish");
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("interactive-only"),
            "{:?}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn delegated_ish_c_emits_no_osc133_markers() {
    let xshi = std::path::PathBuf::from(env!("CARGO_BIN_EXE_xshi"));
    let output = std::process::Command::new(&xshi)
        .args(["-c", "echo hi"])
        .output()
        .expect("run xshi -c");
    assert!(output.status.success());
    let mut combined = output.stdout;
    combined.extend_from_slice(&output.stderr);
    assert!(
        !combined.windows(b"\x1b]133;".len()).any(|w| w == b"\x1b]133;"),
        "xshi -c must not emit OSC 133: {:?}",
        String::from_utf8_lossy(&combined)
    );
    assert!(
        !combined.windows(b"\x1b]7;".len()).any(|w| w == b"\x1b]7;"),
        "xshi -c must not emit OSC 7: {:?}",
        String::from_utf8_lossy(&combined)
    );
}
