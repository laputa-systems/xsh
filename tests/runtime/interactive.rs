#![allow(clippy::single_call_fn)]

//! `xshi` as a process: the command line, the piped test session, and the
//! behavior the `ish` reference shell does not define. Terminal behavior that
//! `ish` does define is compared screen by screen in `interactive/parity`.

use super::common::*;
use std::process::Output;

#[path = "interactive/parity.rs"]
mod parity;

use parity::{Fixture, key};

#[test]
fn hello_example_runs_through_cli() {
    let output = Command::new(cargo_env!("CARGO_BIN_EXE_xsh"))
        .arg("tests/fixtures/runtime/cli-simple.xsh")
        .output()
        .expect("run xsh");

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "hello\n");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn args_example_prints_script_arguments() {
    let output = Command::new(cargo_env!("CARGO_BIN_EXE_xsh"))
        .args(["tests/fixtures/runtime/cli-args.xsh", "--", "one", "two"])
        .output()
        .expect("run xsh");

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "one\ntwo\n");
}

#[test]
fn xsh_interactive_flags_point_to_xshi() {
    for flag in ["-i", "--interactive"] {
        let output = Command::new(cargo_env!("CARGO_BIN_EXE_xsh"))
            .arg(flag)
            .output()
            .expect("run xsh");

        assert_eq!(output.status.code(), Some(2));
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains("xshi"), "{stderr}");
    }
}

#[test]
fn utility_names_are_not_implicit_script_commands() {
    for (name, source) in [
        ("builtin-echo", "echo hi\n"),
        ("builtin-false", "false\n"),
        ("builtin-rg", "rg needle root\n"),
        ("builtin-fd", "fd needle root\n"),
        ("builtin-tree", "tree root\n"),
        ("builtin-env", "env NAME=value true\n"),
        ("builtin-pstree", "pstree\n"),
    ] {
        let output = run_temp_script(name, source);

        assert_eq!(output.status.code(), Some(2));
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains("err[check.unresolved-proc-command]"));
        assert!(stderr.contains("unresolved proc command"));
    }
}

#[test]
fn xsh_ignores_xshi_config_aliases_and_history() {
    let home = Home::new("xsh-ignores-xshi-config");
    std::fs::create_dir_all(home.path().join(".config/xshi")).expect("create config dir");
    std::fs::write(home.path().join(".config/xshi/config.ish"), "alias echo print\n")
        .expect("write config");

    let path = temp_xsh_path("xsh-ignores-interactive-config");
    std::fs::write(&path, "echo hi\n").expect("write temp script");

    let output = Command::new(cargo_env!("CARGO_BIN_EXE_xsh"))
        .env("HOME", home.path())
        .arg(path.to_str().unwrap())
        .output()
        .expect("run xsh");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("err[check.unresolved-proc-command]"));
    assert!(!home.path().join(".local/share/xshi/history").exists());

    std::fs::remove_file(path).expect("remove temp script");
}

/// An isolated HOME for one xshi run. xshi records history and denv state on
/// exit, so no test may run against the developer's real HOME.
struct Home(PathBuf);

impl Home {
    fn new(label: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = temp_path(&format!("{label}-{}", NEXT.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create isolated HOME");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    /// xshi in the piped test-session mode, with its state confined to this HOME.
    fn xshi(&self, args: &[&str]) -> Command {
        let mut command = Command::new(cargo_env!("CARGO_BIN_EXE_xshi"));
        command
            .args(args)
            .current_dir(&self.0)
            .env("HOME", &self.0)
            // A developer's own denv state must not leak in: it would decide
            // whether the repository's `.envrc` is reported.
            .env_remove("__DENV_DIR")
            .env_remove("__DENV_STATE")
            .env_remove("__DENV_DIRTY")
            .env("XSHI_ALLOW_NON_TTY_FOR_TESTS", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    fn run_with(&self, args: &[&str], input: &str) -> Output {
        run_with_input(self.xshi(args), input)
    }

    /// Feeds `input` (without `exit`) to a session that starts in this HOME.
    fn session(&self, input: &str) -> Output {
        run_with_input(self.xshi(&["--no-config"]), &format!("{input}\nexit\n"))
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run_with_input(mut command: Command, input: &str) -> Output {
    let mut child = command.spawn().expect("spawn xshi");
    child
        .stdin
        .take()
        .expect("child stdin")
        .write_all(input.as_bytes())
        .expect("write session input");
    child.wait_with_output().expect("wait for xshi")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn write_test_executable(path: &Path, source: &str) {
    std::fs::write(path, source).expect("write executable");
    let mut permissions = std::fs::metadata(path).expect("metadata").permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).expect("set executable permissions");
}

#[test]
fn xshi_requires_tty_for_normal_startup() {
    let home = Home::new("needs-tty");
    let output = Command::new(cargo_env!("CARGO_BIN_EXE_xshi"))
        .env("HOME", home.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run xshi");

    assert_eq!(output.status.code(), Some(2));
    let stderr = stderr(&output);
    assert!(stderr.contains("requires stdin and stdout to be terminals"), "{stderr}");
}

#[test]
fn piped_session_runs_builtins_and_exits_cleanly() {
    let home = Home::new("piped-session");
    let output = home.session("echo hi");

    assert!(output.status.success());
    assert!(stdout(&output).contains("hi\n"));
    assert_eq!(stderr(&output), "");
}

#[test]
fn exit_status_reaches_the_parent_process() {
    let home = Home::new("exit-status");
    let output = home.run_with(&["--no-config"], "exit 7\n");
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(stderr(&output), "");

    let output = home.run_with(&["--no-config"], "false\nexit\n");
    assert_eq!(output.status.code(), Some(0), "exit without an argument succeeds");
}

#[test]
fn login_profile_is_loaded_for_sessions_but_not_for_command_flag() {
    let home = Home::new("login-profile");
    let bin = home.path().join("bin");
    std::fs::create_dir_all(&bin).expect("create profile bin");
    write_test_executable(&bin.join("xshi-profile-probe"), "#!/bin/sh\nprintf 'from-profile\\n'\n");
    let profile = home.path().join("profile");
    std::fs::write(&profile, format!("export PATH={}\n", bin.display())).expect("write profile");

    let mut session = home.xshi(&["--no-config"]);
    session.env("XSHI_PROFILE_PATH", &profile).env("PATH", "");
    let output = run_with_input(session, "xshi-profile-probe\nexit\n");
    assert!(output.status.success());
    assert!(stdout(&output).contains("from-profile\n"), "{}", stdout(&output));

    let mut one_shot = home.xshi(&["--no-config", "-c", "xshi-profile-probe"]);
    one_shot.env("XSHI_PROFILE_PATH", &profile).env("PATH", "");
    let output = run_with_input(one_shot, "");
    assert!(!stdout(&output).contains("from-profile\n"), "{}", stdout(&output));
}

#[test]
fn command_flag_runs_one_line_with_shell_expansion() {
    let home = Home::new("command-flag");

    let output = run_with_input(home.xshi(&["--no-config", "-c", "echo $((1 + 2))"]), "");
    assert!(output.status.success());
    assert_eq!(stdout(&output), "3\n");
    assert_eq!(stderr(&output), "");

    let output = run_with_input(home.xshi(&["--no-config", "-c", ": && echo ok && :"]), "");
    assert_eq!(stdout(&output), "ok\n");
}

#[test]
fn command_flag_hands_stdin_to_the_command_like_ssh_does() {
    let home = Home::new("command-stdin");
    let path = home.path().join("received");
    let command = format!("cat > {0} && cat {0} && rm {0}", path.display());
    let output = run_with_input(home.xshi(&["--no-config", "-c", &command]), "ssh-shell-ok");

    assert!(output.status.success());
    assert_eq!(stdout(&output), "ssh-shell-ok");
    assert_eq!(stderr(&output), "");
    assert!(!path.exists());
}

#[test]
fn a_program_reading_stdin_does_not_consume_the_piped_session() {
    let home = Home::new("stdin-theft");
    let output = home.session("cat\necho after");

    assert!(output.status.success());
    assert!(stdout(&output).contains("after\n"), "{}", stdout(&output));
    assert_eq!(stderr(&output), "");
}

#[test]
fn xshi_pty_master_does_not_survive_exec() {
    parity::run_xshi_only(&Fixture::new().size(24, 400), |sh| {
        sh.line(cargo_env!("CARGO_BIN_EXE_xsh-test-show-fds"));
        let screen = sh.screen_text();
        let rows: Vec<&str> = screen.lines().filter(|row| !row.trim().is_empty()).collect();
        assert_eq!(rows.len(), 2, "the helper printed a descriptor: {screen}");
    });
}

#[test]
fn commands_resolve_from_path_and_arguments_pass_through_verbatim() {
    let home = Home::new("path-resolution");
    let bin = home.path().join("bin");
    std::fs::create_dir_all(&bin).expect("create bin");
    std::fs::write(home.path().join("marker.txt"), "").expect("write marker");
    write_test_executable(&bin.join("ls"), "#!/bin/sh\nprintf 'external-ls\\n'\n");
    write_test_executable(&bin.join("sudo"), "#!/bin/sh\nprintf '%s\\n' \"$@\"\n");

    let output = home.session(&format!(
        "set PATH {}\nls\nsudo ls -l\nsudo /bin/ls\nsudo command-not-builtin",
        bin.display()
    ));

    let stdout = stdout(&output);
    assert!(stdout.contains("external-ls\n"), "{stdout}");
    assert!(!stdout.contains("marker.txt"), "{stdout}");
    assert!(stdout.contains("ls\n-l\n"), "{stdout}");
    assert!(stdout.contains("/bin/ls\n"), "{stdout}");
    assert!(stdout.contains("command-not-builtin\n"), "{stdout}");
    assert_eq!(stderr(&output), "");
}

#[test]
fn lists_and_statuses_follow_the_previous_command() {
    let home = Home::new("lists");
    let output = home.session(
        "/bin/sh -c \"exit 5\" || echo fallback\n\
         /bin/sh -c \"exit 0\" && echo ok\n\
         true && echo yes\n\
         false || echo no\n\
         /bin/sh -c \"exit 7\"\n\
         echo status=$?\n\
         : && echo colon",
    );

    let stdout = stdout(&output);
    for expected in ["fallback\n", "ok\n", "yes\n", "no\n", "status=7\n", "colon\n"] {
        assert!(stdout.contains(expected), "missing {expected:?}: {stdout}");
    }
    assert_eq!(stderr(&output), "");
}

#[test]
fn a_pipelines_status_is_its_last_stage() {
    let home = Home::new("pipeline-status");
    let output = home.session(
        "sh -c 'exit 3' | cat\n\
         echo external=$?\n\
         echo hi | sh -c 'exit 4'\n\
         echo last=$?\n\
         false | echo mixed\n\
         echo builtin-first=$?\n\
         echo hi | false\n\
         echo builtin-last=$?",
    );

    let stdout = stdout(&output);
    for expected in ["external=0\n", "last=4\n", "mixed\n", "builtin-first=0\n", "builtin-last=1\n"] {
        assert!(stdout.contains(expected), "missing {expected:?}: {stdout}");
    }
}

#[test]
fn session_builtins_in_a_pipeline_leave_the_shell_alone() {
    let home = Home::new("builtin-pipeline");
    std::fs::create_dir_all(home.path().join("one")).expect("create one");
    std::fs::create_dir_all(home.path().join("two")).expect("create two");
    let output = home.session(&format!(
        "cd {0}/one\ncd {0}/two | cat\npwd\nset KEPT no | cat\nprintenv KEPT || echo unset",
        home.path().display()
    ));

    let stdout = stdout(&output);
    assert!(stdout.contains(&format!("{}/one\n", home.path().display())), "{stdout}");
    assert!(stdout.contains("unset\n"), "{stdout}");
}

#[test]
fn redirections_resolve_against_the_session_directory() {
    let home = Home::new("redirections");
    std::fs::create_dir_all(home.path().join("work")).expect("create work");
    let output = home.session("cd work\n/usr/bin/printf hi > out\ncat < out");

    assert!(stdout(&output).contains("hi"), "{}", stdout(&output));
    assert_eq!(std::fs::read_to_string(home.path().join("work/out")).unwrap(), "hi");
    assert_eq!(stderr(&output), "");
}

#[test]
fn both_streams_can_be_redirected_or_piped_together() {
    let home = Home::new("both-streams");
    let output = home.session(
        "sh -c 'echo out; echo err >&2' &> both.txt\n\
         sh -c 'echo more >&2' &>> both.txt\n\
         sh -c 'echo piped-err >&2' |& cat\n\
         sh -c 'echo dup-err >&2' 2>&1 | cat",
    );

    let both = std::fs::read_to_string(home.path().join("both.txt")).unwrap();
    assert_eq!(both, "out\nerr\nmore\n");
    let stdout = stdout(&output);
    assert!(stdout.contains("piped-err\n") && stdout.contains("dup-err\n"), "{stdout}");
    assert_eq!(stderr(&output), "");
}

#[test]
fn eval_runs_its_arguments_as_a_line_in_this_shell() {
    let home = Home::new("eval");
    let output = home.session("eval set EVALED yes\necho $EVALED\neval \"echo a; echo b\"\neval\neval false\necho status=$?");

    let stdout = stdout(&output);
    for expected in ["yes\n", "a\nb\n", "status=1\n"] {
        assert!(stdout.contains(expected), "missing {expected:?}: {stdout}");
    }
    assert_eq!(stderr(&output), "");
}

#[test]
fn exec_runs_the_command_and_then_leaves_the_shell() {
    let home = Home::new("exec");
    let output = home.run_with(&["--no-config"], "exec echo replaced\necho not-reached\n");
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains("replaced\n"), "{}", stdout(&output));
    assert!(!stdout(&output).contains("not-reached"), "{}", stdout(&output));

    let output = home.run_with(&["--no-config"], "exec sh -c 'exit 3'\necho not-reached\n");
    assert_eq!(output.status.code(), Some(3));

    let output = home.run_with(&["--no-config"], "exec no-such-command-xshi\necho still-here\nexit\n");
    assert!(stdout(&output).contains("still-here\n"), "a failed exec keeps the shell");
    assert!(stderr(&output).contains("no-such-command-xshi: not found"), "{}", stderr(&output));
}

#[test]
fn redirections_apply_left_to_right() {
    let home = Home::new("redirection-order");
    let output = home.session(
        "sh -c 'echo out; echo err >&2' 2>&1 > stderr-first.txt | cat\n\
         sh -c 'echo out; echo err >&2' > stdout-first.txt 2>&1 | cat\n\
         echo [$(sh -c 'echo out; echo err >&2' 2>&1)]\n\
         echo [$(sh -c 'echo out; echo err >&2' > captured-away.txt)]",
    );

    let read = |name: &str| std::fs::read_to_string(home.path().join(name)).unwrap();
    assert_eq!(read("stderr-first.txt"), "out\n", "`2>&1 > file` leaves stderr on the pipe");
    assert_eq!(read("stdout-first.txt"), "out\nerr\n", "`> file 2>&1` sends both to the file");
    assert_eq!(read("captured-away.txt"), "out\n");
    let stdout = stdout(&output);
    assert!(stdout.contains("err\n"), "stderr reached the pipeline: {stdout}");
    assert!(stdout.contains("[out err]\n"), "{stdout}");
    assert!(stdout.contains("[]\n"), "{stdout}");
    assert!(stderr(&output).contains("err"), "the redirected substitution left stderr alone");
}

#[test]
fn globs_are_sorted_recursive_and_quotable() {
    let home = Home::new("globs");
    for file in ["b.txt", "a.txt", ".hidden.txt", "root.log", "x1", "x2", "xa", "sub/deep.log"] {
        let path = home.path().join(file);
        std::fs::create_dir_all(path.parent().unwrap()).expect("create parent");
        std::fs::write(path, "").expect("write file");
    }

    let output = home.session(
        "echo *.txt\n\
         echo .*.txt\n\
         echo **/*.log\n\
         echo x[12] x[!12] x?\n\
         echo \"*.txt\" '*.txt' \\*.txt\n\
         echo *.missing\n\
         echo status=$?",
    );

    let stdout = stdout(&output);
    for expected in [
        "a.txt b.txt\n",
        ".hidden.txt\n",
        "root.log sub/deep.log\n",
        "x1 x2 xa x1 x2 xa\n",
        "*.txt *.txt *.txt\n",
        "status=2\n",
    ] {
        assert!(stdout.contains(expected), "missing {expected:?}: {stdout}");
    }
    assert!(stderr(&output).contains("glob pattern matched no paths: *.missing"), "{}", stderr(&output));
}

#[test]
fn command_substitution_splits_globs_and_trims() {
    let home = Home::new("substitution");
    std::fs::write(home.path().join("match.txt"), "").expect("write match");
    let output = home.session(
        "echo before-$(printf sub)-after\n\
         echo `printf tick`\n\
         echo $(printf 'a   b')\n\
         echo \"$(printf 'a\\nb\\n\\n')\"\n\
         echo \"$(printf '*.txt')\"\n\
         echo $(printf '*.txt')\n\
         echo \"$(echo $(echo nested))\"",
    );

    let stdout = stdout(&output);
    for expected in [
        "before-sub-after\n",
        "tick\n",
        "a b\n",
        "a\nb\n",
        "*.txt\n",
        "match.txt\n",
        "nested\n",
    ] {
        assert!(stdout.contains(expected), "missing {expected:?}: {stdout}");
    }
    assert_eq!(stderr(&output), "");
}

#[test]
fn variables_are_exported_only_when_asked() {
    let home = Home::new("variables");
    let output = home.session(
        "set EXPORTED one\n\
         SHELL_ONLY=two\n\
         SCOPED=three printenv SCOPED\n\
         printenv SCOPED || echo scoped-gone\n\
         printenv SHELL_ONLY || echo not-exported\n\
         echo $SHELL_ONLY $EXPORTED\n\
         export SHELL_ONLY\n\
         printenv SHELL_ONLY\n\
         unset EXPORTED\n\
         echo [$EXPORTED]",
    );

    let stdout = stdout(&output);
    for expected in ["three\n", "scoped-gone\n", "not-exported\n", "two one\n", "two\n", "[]\n"] {
        assert!(stdout.contains(expected), "missing {expected:?}: {stdout}");
    }
    assert_eq!(stderr(&output), "");
}

#[test]
fn expansion_of_quotes_tilde_and_escapes() {
    let home = Home::new("expansion");
    std::fs::create_dir_all(home.path().join("subdir")).expect("create subdir");
    let output = home.session(
        "set WORD world\n\
         echo hello-$WORD '$WORD' \"$WORD\" \\$WORD \"\\$WORD\"\n\
         cd ~/subdir\n\
         pwd",
    );

    let stdout = stdout(&output);
    assert!(stdout.contains("hello-world $WORD world $WORD $WORD\n"), "{stdout}");
    assert!(stdout.contains(&format!("{}/subdir\n", home.path().display())), "{stdout}");
    assert_eq!(stderr(&output), "");
}

#[test]
fn malformed_lines_report_and_the_session_continues() {
    let home = Home::new("malformed");
    let output = home.session("let nope =\necho after\nBAD-NAME=value\necho still");

    let stdout = stdout(&output);
    assert!(stdout.contains("after\n") && stdout.contains("still\n"), "{stdout}");
    let stderr = stderr(&output);
    assert!(stderr.contains("parse."), "{stderr}");
    assert!(stderr.contains("invalid environment assignment"), "{stderr}");
}

#[test]
fn l_lists_hidden_entries_and_sees_files_created_by_commands() {
    let home = Home::new("listing");
    std::fs::write(home.path().join("visible.txt"), "x").expect("write visible");
    std::fs::write(home.path().join(".hidden"), "x").expect("write hidden");

    let output = home.session("l\ntouch after.txt\nl");

    let stdout = stdout(&output);
    assert!(stdout.contains("visible.txt") && stdout.contains(".hidden"), "{stdout}");
    let (before, after) = stdout.split_once("after.txt").map_or((stdout.as_str(), ""), |(b, a)| (b, a));
    assert!(!before.contains("after.txt"));
    assert!(after.contains("visible.txt"), "the second listing includes the new file: {stdout}");
    assert_eq!(stderr(&output), "");
}

#[test]
fn z_jumps_to_a_directory_recorded_in_history() {
    let home = Home::new("z-jump");
    let target = home.path().join("work/project-alpha");
    std::fs::create_dir_all(&target).expect("create target");
    std::fs::create_dir_all(home.path().join(".local/share/xshi")).expect("create history dir");
    std::fs::write(
        home.path().join(".local/share/xshi/history"),
        format!("cd {}\n", target.display()),
    )
    .expect("write history");

    let output = home.session("z alpha\npwd");

    assert!(stdout(&output).contains(&format!("{}\n", target.display())), "{}", stdout(&output));
    assert_eq!(stderr(&output), format!("{}\n", target.display()), "z announces where it went");
}

#[test]
fn a_session_records_history_and_the_next_one_recalls_it() {
    let home = Home::new("history-roundtrip");
    let first = home.session("echo remembered\nz nowhere");
    assert!(first.status.success());

    let log = home.path().join(".local/share/xshi");
    assert!(log.join("history.bin").exists(), "exit folds the log into the cache");
    assert_eq!(std::fs::metadata(log.join("history")).unwrap().len(), 0);

    let output = home.session("history");
    assert!(stdout(&output).contains("echo remembered\n"), "{}", stdout(&output));
}

#[test]
fn a_damaged_history_cache_is_reported_and_history_keeps_working() {
    let home = Home::new("damaged-cache");
    let dir = home.path().join(".local/share/xshi");
    std::fs::create_dir_all(&dir).expect("create data dir");
    std::fs::write(dir.join("history.bin"), b"ISH\x05 damaged").expect("write damaged cache");
    std::fs::write(dir.join("history"), "echo from the log\n").expect("write log");

    let output = home.session("history\necho new");

    assert!(stdout(&output).contains("echo from the log\n"), "{}", stdout(&output));
    assert!(stderr(&output).contains("history cache corrupt"), "{}", stderr(&output));
    assert_eq!(std::fs::read(dir.join("history.bin")).unwrap(), b"ISH\x05 damaged");

    let repaired = home.session("history rebuild\nhistory");
    assert!(stdout(&repaired).contains("echo new\n"), "{}", stdout(&repaired));
    assert!(dir.join("history.bin.corrupt").exists());
    assert_eq!(home.session("history").status.code(), Some(0));
}

#[test]
fn config_file_aliases_and_variables_load_unless_disabled() {
    let home = Home::new("config");
    std::fs::create_dir_all(home.path().join(".config/xshi")).expect("create config dir");
    std::fs::write(
        home.path().join(".config/xshi/config.ish"),
        "# comment\nset XSHI_FROM_CONFIG configured\nalias probe echo from-config\n",
    )
    .expect("write config");

    let output = home.run_with(&[], "probe $XSHI_FROM_CONFIG\nexit\n");
    assert!(stdout(&output).contains("from-config configured\n"), "{}", stdout(&output));
    assert_eq!(stderr(&output), "");

    let output = home.run_with(&["--no-config"], "probe\nexit\n");
    assert!(!stdout(&output).contains("from-config"), "{}", stdout(&output));
    assert!(stderr(&output).contains("probe: not found"), "{}", stderr(&output));
}

#[test]
fn history_search_survives_a_peer_shell_resetting_history() {
    parity::run_xshi_only(&Fixture::new().history(&["echo old"]), |sh| {
        let mut peer = sh.spawn_peer();
        peer.line("history reset");
        sh.enter();
        sh.wait_prompt();
        sh.keys(key::CTRL_R);
        assert!(!sh.screen_text().contains("echo old"), "{}", sh.screen_text());
        sh.keys(key::ESC);
        sh.absorb(peer, "peer");
    });
}

#[test]
fn dollar_completes_shell_variables_exported_or_not() {
    parity::run_xshi_only(&Fixture::new().env("XSHI_ZEBRA_ONE", "1"), |sh| {
        sh.line("set XSHI_ZEBRA_TWO 2");
        sh.line("XSHI_ZEBRA_SH=3");

        sh.text("echo $XSHI_Z");
        sh.keys(key::TAB);
        let grid = sh.screen_text();
        for name in ["XSHI_ZEBRA_ONE", "XSHI_ZEBRA_SH", "XSHI_ZEBRA_TWO"] {
            assert!(grid.contains(name), "{name} missing from the grid:\n{grid}");
        }

        sh.text("EBRA_S");
        sh.keys(key::TAB);
        sh.keys(key::ENTER);
        let row = sh.screen_text().lines().rev().find(|row| row.contains("echo")).map(str::to_owned);
        assert!(
            row.as_deref().is_some_and(|row| row.ends_with("echo $XSHI_ZEBRA_SH")),
            "accepted without quoting or a trailing slash: {row:?}"
        );
        sh.keys_to_prompt(key::ENTER);
        assert!(sh.screen_text().contains("\n3\n"), "{}", sh.screen_text());
    });
}

#[test]
fn dollar_completion_works_inside_double_quotes_and_ignores_escaped_dollars() {
    parity::run_xshi_only(&Fixture::new().env("XSHI_QUOTED_VALUE", "ok"), |sh| {
        sh.text("echo \"value=$XSHI_QUOTED_V");
        sh.keys(key::TAB);
        let row = sh.screen_text().lines().rev().find(|row| row.contains("echo")).map(str::to_owned);
        assert!(
            row.as_deref().is_some_and(|row| row.ends_with("echo \"value=$XSHI_QUOTED_VALUE")),
            "{row:?}"
        );
        sh.keys(key::CTRL_U);

        sh.text("echo \\$XSHI_QUOTED_V");
        sh.keys(key::TAB);
        assert!(!sh.screen_text().contains("XSHI_QUOTED_VALUE"), "{}", sh.screen_text());
    });
}

#[test]
fn fg_resumes_the_rest_of_an_and_or_list() {
    let fixture = Fixture::new()
        .executable("bin/ok", "#!/bin/sh\necho ready\nsleep 2\n")
        .executable("bin/bad", "#!/bin/sh\necho ready\nsleep 2\nexit 3\n");
    parity::run_xshi_only(&fixture, |sh| {
        sh.line_until("./bin/ok && echo continued-$((1+1))", "ready");
        sh.keys_to_prompt(key::CTRL_Z);
        assert!(!sh.screen_text().contains("continued-2"), "the list waits while the job is stopped");
        sh.line("fg");
        assert!(sh.screen_text().contains("continued-2"), "{}", sh.screen_text());

        sh.line_until("./bin/bad || echo fallback-$((1+1))", "ready");
        sh.keys_to_prompt(key::CTRL_Z);
        sh.line("fg");
        assert!(sh.screen_text().contains("fallback-2"), "{}", sh.screen_text());

        sh.line_until("./bin/bad && echo skipped-$((1+1)); echo after-$((1+1))", "ready");
        sh.keys_to_prompt(key::CTRL_Z);
        sh.line("fg");
        let screen = sh.screen_text();
        assert!(!screen.contains("skipped-2"), "{screen}");
        assert!(screen.contains("after-2"), "{screen}");
    });
}

#[test]
fn forced_exit_terminates_a_stopped_job() {
    let fixture = Fixture::new().executable("bin/job", "#!/bin/sh\necho $$ > pid\necho ready\nsleep 60\n");
    parity::run_xshi_only(&fixture, |sh| {
        sh.line_until("./bin/job", "ready");
        sh.keys_to_prompt(key::CTRL_Z);
        let pid = std::fs::read_to_string(sh.home().join("pid")).expect("job wrote its pid");
        let pid = pid.trim().to_owned();

        sh.text("exit");
        sh.keys_to_prompt(key::ENTER);
        assert!(sh.screen_text().contains("suspended job"), "{}", sh.screen_text());
        sh.text("exit");
        sh.enter();
        sh.expect_exit("forced_exit");

        let alive = || Command::new("kill").args(["-0", &pid]).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
        let deadline = Instant::now() + Duration::from_secs(5);
        while alive() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!alive(), "the stopped job outlived the shell");
    });
}
