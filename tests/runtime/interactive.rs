#![allow(clippy::single_call_fn)]

use super::common::*;
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;

const PTY_TIMEOUT: Duration = Duration::from_secs(15);

#[test]
fn hello_example_runs_through_cli() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .arg("examples/hello.xsh")
        .output()
        .expect("run xsh");

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "hello\n");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn args_example_prints_script_arguments() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .args(["examples/args.xsh", "--", "one", "two"])
        .output()
        .expect("run xsh");

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "one\ntwo\n");
}

#[test]
fn interactive_echo_runs_as_native_builtin() {
    let output = Command::new(env!("CARGO_BIN_EXE_xshi"))
        .arg("--no-config")
        .env("XSHI_ALLOW_NON_TTY_FOR_TESTS", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child
                .stdin
                .take()
                .expect("child stdin")
                .write_all(b"echo hi\nexit\n")?;
            child.wait_with_output()
        })
        .expect("run interactive xsh");

    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout).unwrap().contains("hi\n"));
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn xsh_interactive_flags_point_to_xshi() {
    for flag in ["-i", "--interactive"] {
        let output = Command::new(env!("CARGO_BIN_EXE_xsh"))
            .arg(flag)
            .output()
            .expect("run xsh");

        assert_eq!(output.status.code(), Some(2));
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains("xshi"), "{stderr}");
    }
}

#[test]
fn xshi_help_is_interactive_specific() {
    let output = Command::new(env!("CARGO_BIN_EXE_xshi"))
        .arg("--help")
        .output()
        .expect("run xshi");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Usage:\n  xshi"));
    assert!(!stdout.contains("SCRIPT"));
}

#[test]
fn xshi_c_accepts_shell_arithmetic_expansion() {
    let output = Command::new(env!("CARGO_BIN_EXE_xshi"))
        .args(["--no-config", "-c", "echo $((1 + 2))"])
        .output()
        .expect("run xshi -c");

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "3\n");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn xshi_c_handles_ssh_style_redirection_from_stdin() {
    let path = temp_path("xshi-c-ssh-redirection");
    let _ = std::fs::remove_file(&path);
    let command = format!(
        "cat > {} && cat {} && rm {}",
        path.display(),
        path.display(),
        path.display()
    );
    let output = Command::new(env!("CARGO_BIN_EXE_xshi"))
        .args(["--no-config", "-c", &command])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child
                .stdin
                .take()
                .expect("child stdin")
                .write_all(b"ssh-shell-ok")?;
            child.wait_with_output()
        })
        .expect("run xshi -c with stdin redirection");

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "ssh-shell-ok");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    assert!(!path.exists());
}

#[test]
fn xshi_requires_tty_for_normal_startup() {
    let output = Command::new(env!("CARGO_BIN_EXE_xshi"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run xshi");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("requires stdin and stdout to be terminals"),
        "{stderr}"
    );
}

#[test]
#[ignore = "flaky: PTY-driven, timing-sensitive; requires a controlling terminal"]
fn xshi_runs_prompt_loop_on_pty() {
    let mut pty = spawn_xshi_pty();

    let mut transcript = pty.read_until("$ ", Duration::from_secs(5));
    pty.write(b"echo pty-ok\r");
    transcript.push_str(&pty.read_until("pty-ok", Duration::from_secs(5)));
    pty.write(b"exit\r");
    let output = pty.wait(Duration::from_secs(5), &transcript);

    assert!(output.status.success(), "{transcript}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
#[ignore = "flaky: PTY-driven, timing-sensitive; requires a controlling terminal"]
fn xshi_pty_prompt_cursor_column_ignores_ansi_color() {
    let mut pty = spawn_xshi_pty();

    let transcript = pty.read_until("$ ", Duration::from_secs(5));
    let (visible_width, cursor_column) = prompt_cursor_measurement(&transcript);
    pty.write(b"exit\r");
    let output = pty.wait(Duration::from_secs(5), &transcript);

    assert!(output.status.success(), "{transcript}");
    assert_eq!(visible_width, cursor_column, "{transcript:?}");
}

#[test]
#[ignore = "flaky: PTY-driven, timing-sensitive; requires a controlling terminal"]
fn xshi_pty_line_editing_handles_backspace() {
    let mut pty = spawn_xshi_pty();

    let mut transcript = pty.read_until("$ ", Duration::from_secs(5));
    pty.write(b"echo pty-ox\x7fk\r");
    transcript.push_str(&pty.read_until("pty-ok", Duration::from_secs(5)));
    pty.write(b"exit\r");
    let output = pty.wait(Duration::from_secs(5), &transcript);

    assert!(output.status.success(), "{transcript}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
#[ignore = "flaky: PTY-driven, timing-sensitive; requires a controlling terminal"]
fn xshi_pty_space_expands_alias_in_editor() {
    let mut pty = spawn_xshi_pty();

    let mut transcript = pty.read_until("$ ", Duration::from_secs(5));
    pty.write(b"alias gp=\"git push -u\"\r");
    transcript.push_str(&pty.read_until("$ ", Duration::from_secs(5)));
    pty.write(b"gp ");
    transcript.push_str(&pty.read_until("git push -u ", Duration::from_secs(5)));
    pty.write(b"\x15exit\r");
    let output = pty.wait(Duration::from_secs(5), &transcript);

    assert!(output.status.success(), "{transcript}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
#[ignore = "flaky: PTY-driven, timing-sensitive; requires a controlling terminal"]
fn xshi_pty_up_arrow_cycles_through_history_entries() {
    let mut pty = spawn_xshi_pty();

    let mut transcript = pty.read_until("$ ", Duration::from_secs(5));
    pty.write(b"echo first-up-history\r");
    transcript.push_str(&pty.read_until("first-up-history\r\n", Duration::from_secs(5)));
    pty.write(b"echo second-up-history\r");
    transcript.push_str(&pty.read_until("second-up-history\r\n", Duration::from_secs(5)));
    pty.write(b"\x1b[A");
    transcript.push_str(&pty.read_until("$ echo second-up-history", Duration::from_secs(5)));
    pty.write(b"\x1b[A");
    transcript.push_str(&pty.read_until("$ echo first-up-history", Duration::from_secs(5)));
    pty.write(b"\r");
    transcript.push_str(&pty.read_until("first-up-history\r\n", Duration::from_secs(5)));
    pty.write(b"exit\r");
    let output = pty.wait(Duration::from_secs(5), &transcript);

    assert!(output.status.success(), "{transcript}");
    assert!(!transcript.contains("$ [A"), "{transcript}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
#[ignore = "flaky: PTY-driven, timing-sensitive; requires a controlling terminal"]
fn xshi_pty_ctrl_c_cancels_line_and_sets_exit_status() {
    let mut pty = spawn_xshi_pty();

    let mut transcript = pty.read_until("$ ", PTY_TIMEOUT);
    pty.write(b"echo should-not-run\x03");
    let cancel = pty.read_until("^C", PTY_TIMEOUT);
    let has_failure_prompt = cancel.contains("! ");
    transcript.push_str(&cancel);
    if !has_failure_prompt {
        transcript.push_str(&pty.read_until("! ", PTY_TIMEOUT));
    }
    pty.write(b"exit\r");
    let output = pty.wait(PTY_TIMEOUT, &transcript);

    assert_eq!(output.status.code(), Some(130), "{transcript}");
    assert!(!transcript.contains("should-not-run\r\n"), "{transcript}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
#[ignore = "flaky: PTY-driven, timing-sensitive; requires a controlling terminal"]
fn xshi_pty_tab_completion_lists_path_candidates() {
    let mut pty = spawn_xshi_pty();

    let mut transcript = pty.read_until("$ ", Duration::from_secs(5));
    pty.write(b"ls Cargo.\t");
    transcript.push_str(&pty.read_until("Cargo.toml", Duration::from_secs(5)));
    pty.write(b"\x15exit\r");
    let output = pty.wait(Duration::from_secs(5), &transcript);

    assert!(output.status.success(), "{transcript}");
    assert!(transcript.contains("Cargo.lock"), "{transcript}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
#[ignore = "flaky: PTY-driven, timing-sensitive; requires a controlling terminal"]
fn xshi_pty_cd_tilde_completion_uses_home() {
    let home = temp_xshi_home("xshi-home");
    std::fs::create_dir_all(home.join("xshi-home-dir")).expect("create home completion dir");
    let mut pty = spawn_xshi_pty_with(None, &[("HOME", home.as_path())]);

    let mut transcript = pty.read_until("$ ", PTY_TIMEOUT);
    pty.write(b"cd ~/xshi\t");
    transcript.push_str(&pty.read_until("xshi-home-dir/", PTY_TIMEOUT));
    pty.write(b"\x15exit\r");
    let output = pty.wait(PTY_TIMEOUT, &transcript);

    assert!(output.status.success(), "{transcript}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    let _ = std::fs::remove_dir_all(home);
}

#[test]
#[ignore = "flaky: PTY-driven, timing-sensitive; requires a controlling terminal"]
fn xshi_pty_completion_clear_removes_stale_grid_rows() {
    let root = temp_xshi_home("xshi-grid-clear");
    std::fs::create_dir_all(&root).expect("create completion root");
    for index in 0..24 {
        std::fs::write(root.join(format!("alpha-{index:02}")), "").expect("write candidate");
    }
    let mut pty = spawn_xshi_pty_with(Some(root.as_path()), &[]);

    let mut transcript = pty.read_until("$ ", PTY_TIMEOUT);
    pty.write(b"ls a\t");
    transcript.push_str(&pty.read_until("alpha-", PTY_TIMEOUT));
    pty.write(b"\t");
    transcript.push_str(&pty.read_until("alpha-23", PTY_TIMEOUT));
    pty.write(b"z");
    transcript.push_str(&pty.read_until("ls alpha-z", PTY_TIMEOUT));
    pty.write(b"\x15exit\r");
    transcript.push_str(&pty.read_until("exit", PTY_TIMEOUT));
    let output = pty.wait(PTY_TIMEOUT, &transcript);

    assert!(output.status.success(), "{transcript}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    let screen = terminal_screen(&transcript);
    assert!(
        screen
            .iter()
            .all(|line| !line.contains("alpha-00") && !line.contains("alpha-23")),
        "{screen:?}\ntranscript={transcript:?}"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
#[ignore = "flaky: PTY-driven, timing-sensitive; requires a controlling terminal"]
fn xshi_pty_enter_accepts_completion_without_submitting() {
    let root = temp_xshi_home("xshi-grid-enter");
    std::fs::create_dir_all(&root).expect("create completion root");
    std::fs::write(root.join("alpha-one"), "").expect("write candidate");
    std::fs::write(root.join("arc-two"), "").expect("write candidate");
    let mut pty = spawn_xshi_pty_with(Some(root.as_path()), &[]);

    let mut transcript = pty.read_until("$ ", Duration::from_secs(5));
    pty.write(b"ls a\t");
    transcript.push_str(&pty.read_until("alpha-", Duration::from_secs(5)));
    pty.write(b"\r\x15exit\r");
    let output = pty.wait(Duration::from_secs(5), &transcript);

    assert!(output.status.success(), "{transcript}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
#[ignore = "flaky: PTY-driven, timing-sensitive; requires a controlling terminal"]
fn xshi_pty_right_arrow_accepts_history_autosuggestion() {
    let mut pty = spawn_xshi_pty();

    let mut transcript = pty.read_until("$ ", Duration::from_secs(5));
    pty.write(b"echo autosuggest-ok\r");
    transcript.push_str(&pty.read_until("$ ", Duration::from_secs(5)));
    pty.write(b"ech");
    transcript.push_str(&pty.read_until("o autosuggest-ok", Duration::from_secs(5)));
    pty.write(b"\x1b[C\r");
    transcript.push_str(&pty.read_until("autosuggest-ok", Duration::from_secs(5)));
    pty.write(b"exit\r");
    let output = pty.wait(Duration::from_secs(5), &transcript);

    assert!(output.status.success(), "{transcript}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

fn read_until_with_prompt(pty: &mut PtyXshi, transcript: &mut String, needle: &str) {
    let chunk = pty.read_until(needle, Duration::from_secs(5));
    let has_prompt = chunk.contains("$ ");
    transcript.push_str(&chunk);
    if !has_prompt {
        transcript.push_str(&pty.read_until("$ ", Duration::from_secs(5)));
    }
}

#[test]
#[ignore = "flaky: PTY-driven, timing-sensitive; requires a controlling terminal"]
fn xshi_pty_ctrl_r_opens_history_search_from_empty_prompt() {
    let home = temp_xshi_home("xshi-history-empty");
    let mut pty = spawn_xshi_pty_with(None, &[("HOME", home.as_path())]);

    let mut transcript = pty.read_until("$ ", Duration::from_secs(5));
    pty.write(b"echo ctrl-r-empty-marker\r");
    read_until_with_prompt(&mut pty, &mut transcript, "ctrl-r-empty-marker\r\n");
    transcript.push_str(&pty.read_until_quiet(Duration::from_millis(50), Duration::from_secs(5)));
    pty.write(b"\x12");
    transcript.push_str(&pty.read_until("echo ctrl-r-empty-marker", Duration::from_secs(5)));
    pty.write(b"\r");
    transcript.push_str(&pty.read_until("$ echo ctrl-r-empty-marker", Duration::from_secs(5)));
    pty.write(b"\x15exit\r");
    let output = pty.wait(Duration::from_secs(5), &transcript);

    assert!(output.status.success(), "{transcript}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    let _ = std::fs::remove_dir_all(home);
}

#[test]
#[ignore = "flaky: PTY-driven, timing-sensitive; requires a controlling terminal"]
fn xshi_pty_ctrl_r_incremental_history_search_accepts_match() {
    let home = temp_xshi_home("xshi-history-incremental");
    let mut pty = spawn_xshi_pty_with(None, &[("HOME", home.as_path())]);

    let mut transcript = pty.read_until("$ ", Duration::from_secs(5));
    pty.write(b"echo alpha-search-marker\r");
    read_until_with_prompt(&mut pty, &mut transcript, "alpha-search-marker\r\n");
    pty.write(b"echo beta-search-marker\r");
    read_until_with_prompt(&mut pty, &mut transcript, "beta-search-marker\r\n");
    transcript.push_str(&pty.read_until_quiet(Duration::from_millis(50), Duration::from_secs(5)));
    pty.write(b"\x12alpha");
    transcript.push_str(&pty.read_until("\x1b[7mecho alpha-search-marker", Duration::from_secs(5)));
    pty.write(b"\r\r");
    transcript.push_str(&pty.read_until("$ ", Duration::from_secs(5)));
    pty.write(b"exit\r");
    let output = pty.wait(Duration::from_secs(5), &transcript);

    assert!(output.status.success(), "{transcript}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    let _ = std::fs::remove_dir_all(home);
}

#[test]
#[ignore = "flaky: PTY-driven, timing-sensitive; requires a controlling terminal"]
fn xshi_pty_ctrl_r_down_arrow_sequence_can_arrive_split() {
    let home = temp_xshi_home("xshi-history-split-down");
    let mut pty = spawn_xshi_pty_with(None, &[("HOME", home.as_path())]);

    let mut transcript = pty.read_until("$ ", PTY_TIMEOUT);
    pty.write(b"echo first-split-history\r");
    read_until_with_prompt(&mut pty, &mut transcript, "first-split-history\r\n");
    pty.write(b"echo second-split-history\r");
    read_until_with_prompt(&mut pty, &mut transcript, "second-split-history\r\n");
    transcript.push_str(&pty.read_until_quiet(Duration::from_millis(50), Duration::from_secs(5)));
    pty.write(b"\x12");
    transcript.push_str(&pty.read_until("\x1b[7mecho second-split-history", PTY_TIMEOUT));
    pty.write(b"\x1b");
    std::thread::sleep(Duration::from_millis(25));
    pty.write(b"[B");
    transcript.push_str(&pty.read_until("\x1b[7mecho first-split-history", PTY_TIMEOUT));
    pty.write(b"\r");
    transcript.push_str(&pty.read_until("$ echo first-split-history", PTY_TIMEOUT));
    pty.write(b"\x15exit\r");
    let output = pty.wait(PTY_TIMEOUT, &transcript);

    assert!(output.status.success(), "{transcript}");
    assert!(!transcript.contains("$ [B"), "{transcript}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    let _ = std::fs::remove_dir_all(home);
}

#[test]
#[ignore = "flaky: PTY-driven, timing-sensitive; requires a controlling terminal"]
fn xshi_pty_ctrl_r_down_arrow_final_byte_can_arrive_split() {
    let home = temp_xshi_home("xshi-history-split-final-down");
    let mut pty = spawn_xshi_pty_with(None, &[("HOME", home.as_path())]);

    let mut transcript = pty.read_until("$ ", PTY_TIMEOUT);
    pty.write(b"echo first-final-split-history\r");
    read_until_with_prompt(&mut pty, &mut transcript, "first-final-split-history\r\n");
    pty.write(b"echo second-final-split-history\r");
    read_until_with_prompt(&mut pty, &mut transcript, "second-final-split-history\r\n");
    transcript.push_str(&pty.read_until_quiet(Duration::from_millis(50), Duration::from_secs(5)));
    pty.write(b"\x12");
    transcript.push_str(&pty.read_until("\x1b[7mecho second-final-split-history", PTY_TIMEOUT));
    pty.write(b"\x1b[");
    std::thread::sleep(Duration::from_millis(25));
    pty.write(b"B");
    transcript.push_str(&pty.read_until("\x1b[7mecho first-final-split-history", PTY_TIMEOUT));
    pty.write(b"\r");
    transcript.push_str(&pty.read_until("$ echo first-final-split-history", PTY_TIMEOUT));
    pty.write(b"\x15exit\r");
    let output = pty.wait(PTY_TIMEOUT, &transcript);

    assert!(output.status.success(), "{transcript}");
    assert!(!transcript.contains("$ B"), "{transcript}");
    assert!(!transcript.contains("$ [B"), "{transcript}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    let _ = std::fs::remove_dir_all(home);
}

#[test]
#[ignore = "flaky: PTY-driven, timing-sensitive; requires a controlling terminal"]
fn xshi_pty_ctrl_r_up_down_navigate_without_leaking_escape_bytes() {
    let home = temp_xshi_home("xshi-history-nav");
    let mut pty = spawn_xshi_pty_with(None, &[("HOME", home.as_path())]);

    let mut transcript = pty.read_until("$ ", Duration::from_secs(5));
    pty.write(b"echo first-nav-history\r");
    read_until_with_prompt(&mut pty, &mut transcript, "first-nav-history\r\n");
    pty.write(b"echo second-nav-history\r");
    read_until_with_prompt(&mut pty, &mut transcript, "second-nav-history\r\n");
    transcript.push_str(&pty.read_until_quiet(Duration::from_millis(50), Duration::from_secs(5)));
    pty.write(b"\x12");
    transcript.push_str(&pty.read_until("\x1b[7mecho second-nav-history", Duration::from_secs(5)));
    pty.write(b"\x1b[B");
    transcript.push_str(&pty.read_until("\x1b[7mecho first-nav-history", Duration::from_secs(5)));
    pty.write(b"\x1b[A");
    transcript.push_str(&pty.read_until("\x1b[7mecho second-nav-history", Duration::from_secs(5)));
    pty.write(b"\r");
    transcript.push_str(&pty.read_until("$ echo second-nav-history", Duration::from_secs(5)));
    pty.write(b"\x15exit\r");
    let output = pty.wait(Duration::from_secs(5), &transcript);

    assert!(output.status.success(), "{transcript}");
    assert!(!transcript.contains("$ [A"), "{transcript}");
    assert!(!transcript.contains("$ [B"), "{transcript}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    let _ = std::fs::remove_dir_all(home);
}

#[test]
#[ignore = "flaky: PTY-driven, timing-sensitive; requires a controlling terminal"]
fn xshi_pty_ctrl_r_escape_restores_original_buffer() {
    let home = temp_xshi_home("xshi-history-cancel");
    let mut pty = spawn_xshi_pty_with(None, &[("HOME", home.as_path())]);

    let mut transcript = pty.read_until("$ ", Duration::from_secs(5));
    pty.write(b"echo cancel-history-marker\r");
    read_until_with_prompt(&mut pty, &mut transcript, "cancel-history-marker\r\n");
    transcript.push_str(&pty.read_until_quiet(Duration::from_millis(50), Duration::from_secs(5)));
    pty.write(b"echo preserved\x12cancel");
    transcript
        .push_str(&pty.read_until("\x1b[7mecho cancel-history-marker", Duration::from_secs(5)));
    pty.write(b"\x1b");
    transcript.push_str(&pty.read_until("$ echo preserved", Duration::from_secs(5)));
    let restored_transcript = transcript.clone();
    pty.write(b"\x15exit\r");
    let output = pty.wait(Duration::from_secs(5), &transcript);

    assert!(output.status.success(), "{transcript}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    let screen = terminal_screen(&restored_transcript);
    let prompt_row = screen
        .iter()
        .rposition(|line| line.contains("$ echo preserved"))
        .expect("restored prompt row");
    assert!(
        !screen[prompt_row].contains("preservedcancel"),
        "{screen:?}"
    );
    assert!(
        screen
            .iter()
            .skip(prompt_row + 1)
            .all(|line| !line.contains("search:") && !line.contains("cancel-history-marker")),
        "{screen:?}"
    );
    let _ = std::fs::remove_dir_all(home);
}

#[test]
#[ignore = "flaky: PTY-driven, timing-sensitive; requires a controlling terminal"]
fn xshi_pty_external_command_reads_terminal_in_cooked_mode() {
    let mut pty = spawn_xshi_pty();

    let mut transcript = pty.read_until("$ ", Duration::from_secs(5));
    pty.write(b"/bin/sh -c 'printf ready; IFS= read line; printf got-$line'\r");
    transcript.push_str(&pty.read_until("ready", Duration::from_secs(5)));
    pty.write(b"ok\r");
    transcript.push_str(&pty.read_until("got-ok", Duration::from_secs(5)));
    pty.write(b"exit\r");
    let output = pty.wait(Duration::from_secs(5), &transcript);

    assert!(output.status.success(), "{transcript}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
#[ignore = "flaky: PTY-driven, timing-sensitive; requires a controlling terminal"]
fn xshi_pty_background_job_reaps_before_later_prompt() {
    let mut pty = spawn_xshi_pty();

    let mut transcript = pty.read_until("$ ", PTY_TIMEOUT);
    pty.write(b"/bin/sh -c \"sleep 0.1\" &\r");
    let started = pty.read_until("[1] ", PTY_TIMEOUT);
    let has_prompt = started.contains("$ ");
    transcript.push_str(&started);
    if !has_prompt {
        transcript.push_str(&pty.read_until("$ ", PTY_TIMEOUT));
    }
    std::thread::sleep(Duration::from_millis(800));
    pty.write(b"\r");
    let completed = pty.read_until("xshi: completed:", PTY_TIMEOUT);
    let has_prompt = completed.contains("$ ");
    transcript.push_str(&completed);
    if !has_prompt {
        transcript.push_str(&pty.read_until("$ ", PTY_TIMEOUT));
    }
    pty.write(b"exit\r");
    let output = pty.wait(PTY_TIMEOUT, &transcript);

    assert!(output.status.success(), "{transcript}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
#[ignore = "flaky: PTY-driven, timing-sensitive; requires a controlling terminal"]
fn xshi_pty_rejects_second_background_job_until_first_reaps() {
    let mut pty = spawn_xshi_pty();

    let mut transcript = pty.read_until("$ ", Duration::from_secs(5));
    pty.write(b"/bin/sh -c \"sleep 0.2\" &\r");
    let started = pty.read_until("[1] ", Duration::from_secs(5));
    let has_prompt = started.contains("$ ");
    transcript.push_str(&started);
    if !has_prompt {
        transcript.push_str(&pty.read_until("$ ", Duration::from_secs(5)));
    }
    pty.write(b"/bin/sh -c \"sleep 5\" &\r");
    let rejected = pty.read_until("background job already exists", Duration::from_secs(5));
    let has_prompt = rejected.contains("! ");
    transcript.push_str(&rejected);
    if !has_prompt {
        transcript.push_str(&pty.read_until("! ", Duration::from_secs(5)));
    }
    std::thread::sleep(Duration::from_millis(300));
    pty.write(b"\r");
    transcript.push_str(&pty.read_until("xshi: completed:", Duration::from_secs(5)));
    pty.write(b"exit 0\r");
    let output = pty.wait(Duration::from_secs(5), &transcript);

    assert!(output.status.success(), "{transcript}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
#[ignore = "flaky: PTY-driven, timing-sensitive; requires a controlling terminal"]
fn xshi_pty_background_job_can_fg_and_ctrl_c() {
    let mut pty = spawn_xshi_pty();

    let mut transcript = pty.read_until("$ ", PTY_TIMEOUT);
    pty.write(b"/bin/sh -c \"sleep 5\" &\r");
    let mut started = pty.read_until("[1] ", PTY_TIMEOUT);
    if background_job_pgid(&started).is_none() {
        started.push_str(&pty.read_until("$ ", PTY_TIMEOUT));
    }
    let job_pgid = background_job_pgid(&started).expect("background job pid in transcript");
    let has_prompt = started.contains("$ ");
    transcript.push_str(&started);
    if !has_prompt {
        transcript.push_str(&pty.read_until("$ ", PTY_TIMEOUT));
    }
    pty.write(b"fg\r");
    wait_for_foreground_pgrp(&pty.master, job_pgid, PTY_TIMEOUT);
    transcript.push_str(&pty.read_until_quiet(Duration::from_millis(50), Duration::from_secs(1)));
    pty.write(b"\x03");
    transcript.push_str(&pty.read_until("! ", PTY_TIMEOUT));
    pty.write(b"fg\r");
    let no_job = pty.read_until("no background job", PTY_TIMEOUT);
    let has_prompt = no_job.contains("! ");
    transcript.push_str(&no_job);
    if !has_prompt {
        transcript.push_str(&pty.read_until("! ", PTY_TIMEOUT));
    }
    pty.write(b"exit 0\r");
    let output = pty.wait(PTY_TIMEOUT, &transcript);

    assert!(output.status.success(), "{transcript}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
#[ignore = "flaky: PTY-driven, timing-sensitive; requires a controlling terminal"]
fn xshi_pty_rejects_unsupported_background_shapes() {
    let mut pty = spawn_xshi_pty();

    let mut transcript = pty.read_until("$ ", Duration::from_secs(5));
    let cases = [
        (
            b"/bin/echo ok | /usr/bin/wc -c &\r".as_slice(),
            "background pipelines are not supported",
        ),
        (
            b"/bin/true && /bin/echo ok &\r".as_slice(),
            "background jobs require one simple external command",
        ),
        (
            b"cd /tmp &\r".as_slice(),
            "session builtins cannot run in the background",
        ),
    ];

    for (input, expected) in cases {
        pty.write(input);
        let rejected = pty.read_until(expected, Duration::from_secs(5));
        let has_prompt = rejected.contains("! ");
        transcript.push_str(&rejected);
        if !has_prompt {
            transcript.push_str(&pty.read_until("! ", Duration::from_secs(5)));
        }
    }
    pty.write(b"exit 0\r");
    let output = pty.wait(Duration::from_secs(5), &transcript);

    assert!(output.status.success(), "{transcript}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
#[ignore = "flaky: PTY-driven, timing-sensitive; requires a controlling terminal"]
fn xshi_pty_ctrl_z_auto_backgrounds_foreground_job() {
    let mut pty = spawn_xshi_pty();

    let mut transcript = pty.read_until("$ ", Duration::from_secs(5));
    pty.write(b"/bin/sh -c \"printf xshi-; printf ready; sleep 5\"\r");
    transcript.push_str(&pty.read_until("xshi-ready", Duration::from_secs(5)));
    std::thread::sleep(Duration::from_millis(100));
    pty.write(b"\x1a");
    let backgrounded = pty.read_until("xshi: backgrounded:", Duration::from_secs(5));
    let has_prompt = backgrounded.contains("! ");
    transcript.push_str(&backgrounded);
    if !has_prompt {
        transcript.push_str(&pty.read_until("! ", Duration::from_secs(5)));
    }
    pty.write(b"bg\r");
    let running = pty.read_until("job already running", Duration::from_secs(5));
    let has_prompt = running.contains("! ");
    transcript.push_str(&running);
    if !has_prompt {
        transcript.push_str(&pty.read_until("! ", Duration::from_secs(5)));
    }
    pty.write(b"fg\r");
    transcript.push_str(&pty.read_until_quiet(Duration::from_millis(100), Duration::from_secs(5)));
    pty.write(b"\x03");
    transcript.push_str(&pty.read_until("! ", Duration::from_secs(5)));
    pty.write(b"fg\r");
    let no_job = pty.read_until("no background job", Duration::from_secs(5));
    let has_prompt = no_job.contains("! ");
    transcript.push_str(&no_job);
    if !has_prompt {
        transcript.push_str(&pty.read_until("! ", Duration::from_secs(5)));
    }
    pty.write(b"bg\r");
    let no_job = pty.read_until("bg: no background job", Duration::from_secs(5));
    let has_prompt = no_job.contains("! ");
    transcript.push_str(&no_job);
    if !has_prompt {
        transcript.push_str(&pty.read_until("! ", Duration::from_secs(5)));
    }
    std::thread::sleep(Duration::from_millis(100));
    pty.write(b"exit 0\r");
    let output = pty.wait(Duration::from_secs(5), &transcript);

    assert!(output.status.success(), "{transcript}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

struct PtyXshi {
    master: std::fs::File,
    child: Child,
    temp_home: Option<PathBuf>,
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl PtyXshi {
    fn write(&mut self, bytes: &[u8]) {
        self.master.write_all(bytes).expect("write pty input");
        self.master.flush().expect("flush pty input");
    }

    fn read_until(&mut self, needle: &str, timeout: Duration) -> String {
        read_pty_until(&mut self.master, &mut self.child, needle, timeout)
    }

    fn read_until_quiet(&mut self, quiet: Duration, timeout: Duration) -> String {
        read_pty_until_quiet(&mut self.master, &mut self.child, quiet, timeout)
    }

    fn wait(mut self, timeout: Duration, transcript: &str) -> std::process::Output {
        let status =
            wait_child_with_timeout(&mut self.master, &mut self.child, timeout, transcript);
        assert!(status.code().is_some(), "{transcript}");
        let output = self.child.wait_with_output().expect("collect xshi output");
        if let Some(home) = &self.temp_home {
            let _ = std::fs::remove_dir_all(home);
        }
        output
    }
}

fn spawn_xshi_pty() -> PtyXshi {
    spawn_xshi_pty_with(None, &[])
}

fn temp_xshi_home(label: &str) -> PathBuf {
    static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let home = std::env::temp_dir().join(format!("{label}-{}-{id}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("create temporary xshi home");
    home
}

fn spawn_xshi_pty_with(cwd: Option<&Path>, envs: &[(&str, &Path)]) -> PtyXshi {
    let temp_home =
        (!envs.iter().any(|(name, _)| *name == "HOME")).then(|| temp_xshi_home("xshi-pty-home"));
    let mut env_paths: Vec<(&str, &Path)> = envs.to_vec();
    if let Some(home) = &temp_home {
        env_paths.push(("HOME", home.as_path()));
    }
    spawn_xshi_pty_with_temp_home(cwd, &env_paths, temp_home.clone())
}

fn spawn_xshi_pty_with_temp_home(
    cwd: Option<&Path>,
    envs: &[(&str, &Path)],
    temp_home: Option<PathBuf>,
) -> PtyXshi {
    let guard = pty_test_guard();
    let mut master = 0;
    let mut slave = 0;
    let opened = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    assert_eq!(opened, 0);
    let mut attrs = unsafe { std::mem::zeroed::<libc::termios>() };
    let got_attrs = unsafe { libc::tcgetattr(slave, &mut attrs) };
    assert_eq!(got_attrs, 0);
    attrs.c_lflag |= libc::ICANON | libc::ECHO | libc::ISIG;
    attrs.c_iflag |= libc::ICRNL;
    attrs.c_oflag |= libc::OPOST | libc::ONLCR;
    attrs.c_cc[libc::VINTR] = 0x03;
    attrs.c_cc[libc::VSUSP] = 0x1a;
    let set_attrs = unsafe { libc::tcsetattr(slave, libc::TCSANOW, &attrs) };
    assert_eq!(set_attrs, 0);

    set_fd_nonblocking(master);
    let stdin_fd = unsafe { libc::dup(slave) };
    let stdout_fd = unsafe { libc::dup(slave) };
    let stderr_fd = unsafe { libc::dup(slave) };
    assert!(stdin_fd >= 0);
    assert!(stdout_fd >= 0);
    assert!(stderr_fd >= 0);

    let master = unsafe { std::fs::File::from_raw_fd(master) };
    let stdin = unsafe { std::fs::File::from_raw_fd(stdin_fd) };
    let stdout = unsafe { std::fs::File::from_raw_fd(stdout_fd) };
    let stderr = unsafe { std::fs::File::from_raw_fd(stderr_fd) };
    unsafe {
        libc::close(slave);
    }

    let mut command = Command::new(env!("CARGO_BIN_EXE_xshi"));
    command
        .arg("--no-config")
        .stdin(Stdio::from(stdin))
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::ioctl(libc::STDIN_FILENO, libc::TIOCSCTTY as _, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    for (name, value) in envs {
        command.env(name, value);
    }
    let child = command.spawn().expect("run xshi on pty");

    PtyXshi {
        master,
        child,
        temp_home,
        _guard: guard,
    }
}

fn terminal_screen(transcript: &str) -> Vec<String> {
    let mut rows = vec![String::new()];
    let mut row = 0_usize;
    let mut col = 0_usize;
    let chars = transcript.chars().collect::<Vec<_>>();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\r' => {
                col = 0;
                i += 1;
            }
            '\n' => {
                row += 1;
                col = 0;
                if row == rows.len() {
                    rows.push(String::new());
                }
                i += 1;
            }
            '\u{1b}' if chars.get(i + 1) == Some(&'[') => {
                i += 2;
                let mut arg = String::new();
                while i < chars.len() && !('@'..='~').contains(&chars[i]) {
                    arg.push(chars[i]);
                    i += 1;
                }
                if i == chars.len() {
                    break;
                }
                let final_ch = chars[i];
                i += 1;
                let n = arg.trim_start_matches('?').parse::<usize>().unwrap_or(1);
                match final_ch {
                    'A' => row = row.saturating_sub(n),
                    'B' => {
                        row += n;
                        while row >= rows.len() {
                            rows.push(String::new());
                        }
                    }
                    'C' => col += n,
                    'D' => col = col.saturating_sub(n),
                    'H' => {
                        row = 0;
                        col = 0;
                    }
                    'J' => {
                        rows.truncate(row + 1);
                        let len = rows[row].len();
                        rows[row].truncate(col.min(len));
                    }
                    'K' => {
                        let len = rows[row].len();
                        rows[row].truncate(col.min(len));
                    }
                    _ => {}
                }
            }
            ch => {
                while row >= rows.len() {
                    rows.push(String::new());
                }
                let line = &mut rows[row];
                while line.len() < col {
                    line.push(' ');
                }
                if col < line.len() {
                    line.replace_range(col..col + 1, &ch.to_string());
                } else {
                    line.push(ch);
                }
                col += 1;
                i += 1;
            }
        }
    }
    rows
}

fn background_job_pgid(started: &str) -> Option<libc::pid_t> {
    let start = started.rfind("[1] ")? + 4;
    let pid = started[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    (!pid.is_empty()).then(|| pid.parse().ok()).flatten()
}

fn wait_for_foreground_pgrp(master: &std::fs::File, pgid: libc::pid_t, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        let foreground = unsafe { libc::tcgetpgrp(master.as_raw_fd()) };
        if foreground == pgid {
            return;
        }
        assert!(foreground >= 0, "tcgetpgrp failed");
        if Instant::now() >= deadline {
            panic!("timed out waiting for foreground pgrp {pgid}; current={foreground}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn set_fd_nonblocking(fd: i32) {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL, 0) };
    assert!(flags >= 0, "fcntl(F_GETFL) failed");
    let updated = unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };
    assert!(updated >= 0, "fcntl(F_SETFL) failed");
}

fn read_pty_until(
    master: &mut std::fs::File,
    child: &mut Child,
    needle: &str,
    timeout: Duration,
) -> String {
    let deadline = Instant::now() + timeout;
    let mut transcript = Vec::new();
    let mut buf = [0_u8; 4096];
    loop {
        match master.read(&mut buf) {
            Ok(0) => {}
            Ok(read) => {
                transcript.extend_from_slice(&buf[..read]);
                let text = String::from_utf8_lossy(&transcript);
                if text.contains(needle) {
                    return text.into_owned();
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
            Err(err) => panic!("read PTY master: {err}"),
        }

        if let Some(status) = child.try_wait().expect("poll xshi child") {
            panic!(
                "xshi exited before PTY output contained {needle:?}: status={status}, transcript={}",
                String::from_utf8_lossy(&transcript)
            );
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "timed out waiting for PTY output {needle:?}; transcript={}",
                String::from_utf8_lossy(&transcript)
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn read_pty_until_quiet(
    master: &mut std::fs::File,
    child: &mut Child,
    quiet: Duration,
    timeout: Duration,
) -> String {
    let deadline = Instant::now() + timeout;
    let mut quiet_since = Instant::now();
    let mut transcript = Vec::new();
    let mut buf = [0_u8; 4096];
    loop {
        match master.read(&mut buf) {
            Ok(0) => {}
            Ok(read) => {
                transcript.extend_from_slice(&buf[..read]);
                quiet_since = Instant::now();
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
            Err(err) => panic!("read PTY master: {err}"),
        }

        if let Some(status) = child.try_wait().expect("poll xshi child") {
            panic!(
                "xshi exited while draining PTY output: status={status}, transcript={}",
                String::from_utf8_lossy(&transcript)
            );
        }
        if !transcript.is_empty() && quiet_since.elapsed() >= quiet {
            return String::from_utf8_lossy(&transcript).into_owned();
        }
        if Instant::now() >= deadline {
            return String::from_utf8_lossy(&transcript).into_owned();
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn wait_child_with_timeout(
    master: &mut std::fs::File,
    child: &mut Child,
    timeout: Duration,
    transcript: &str,
) -> std::process::ExitStatus {
    let deadline = Instant::now() + timeout;
    let mut tail = Vec::new();
    let mut buf = [0_u8; 4096];
    loop {
        match master.read(&mut buf) {
            Ok(0) => {}
            Ok(read) => tail.extend_from_slice(&buf[..read]),
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
            Err(err) if err.raw_os_error() == Some(libc::EIO) => {}
            Err(err) => panic!("read PTY master while waiting for xshi exit: {err}"),
        }
        if let Some(status) = child.try_wait().expect("poll xshi child") {
            return status;
        }
        if Instant::now() >= deadline {
            let pid = child.id() as libc::pid_t;
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
            let _ = child.kill();
            let reap_deadline = Instant::now() + Duration::from_secs(1);
            while Instant::now() < reap_deadline {
                if let Some(status) = child.try_wait().expect("reap xshi child") {
                    return status;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            panic!(
                "timed out waiting for xshi exit; transcript={transcript}{}",
                String::from_utf8_lossy(&tail)
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn prompt_cursor_measurement(transcript: &str) -> (usize, usize) {
    let legacy_marker = "\r\u{1b}[2K";
    let render_marker = "\u{1b}[K\r";
    let marker = if transcript.contains(legacy_marker) {
        legacy_marker
    } else if transcript.contains("\u{1b}[s") {
        "\u{1b}[s"
    } else {
        render_marker
    };
    let prompt_start = transcript
        .rfind(marker)
        .map(|index| index + marker.len())
        .expect("prompt clear marker");
    let rest = &transcript[prompt_start..];
    let prompt_end = rest.find('\r').expect("prompt cursor return");
    let prompt = &rest[..prompt_end];
    let cursor_tail = rest[prompt_end + 1..]
        .strip_suffix("\u{1b}[?25h")
        .unwrap_or(&rest[prompt_end + 1..]);
    let cursor = cursor_tail
        .strip_prefix("\u{1b}[")
        .and_then(|rest| rest.strip_suffix('C'))
        .and_then(|n| n.parse::<usize>().ok())
        .expect("cursor movement");
    (visible_terminal_width(prompt), cursor)
}

fn visible_terminal_width(text: &str) -> usize {
    let mut width = 0;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for ch in chars.by_ref() {
                if ('@'..='~').contains(&ch) {
                    break;
                }
            }
        } else {
            width += 1;
        }
    }
    width
}

#[test]
fn xshi_rejects_script_paths_and_arguments() {
    let output = Command::new(env!("CARGO_BIN_EXE_xshi"))
        .args(["script.xsh", "arg"])
        .output()
        .expect("run xshi");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("script paths"), "{stderr}");
}

fn run_xshi_input(
    args: &[&str],
    input: &str,
    home: Option<&std::path::Path>,
) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_xshi"));
    command
        .args(args)
        .env("XSHI_ALLOW_NON_TTY_FOR_TESTS", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(home) = home {
        command.env("HOME", home);
    }
    command
        .spawn()
        .and_then(|mut child| {
            child
                .stdin
                .take()
                .expect("child stdin")
                .write_all(input.as_bytes())?;
            child.wait_with_output()
        })
        .expect("run interactive xsh")
}

fn write_test_executable(path: &Path, source: &str) {
    std::fs::write(path, source).expect("write executable");
    let mut permissions = std::fs::metadata(path).expect("metadata").permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).expect("set executable permissions");
}

#[test]
fn xshi_interactive_loads_profile_before_no_config() {
    let root = temp_path("interactive-login-profile");
    let _ = std::fs::remove_dir_all(&root);
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).expect("create profile bin");
    let probe = bin.join("xshi-profile-probe");
    write_test_executable(&probe, "#!/bin/sh\nprintf 'from-profile\\n'\n");
    let profile = root.join("profile");
    std::fs::write(&profile, format!("export PATH={}\n", bin.display())).expect("write profile");

    let output = Command::new(env!("CARGO_BIN_EXE_xshi"))
        .arg("--no-config")
        .env("XSHI_ALLOW_NON_TTY_FOR_TESTS", "1")
        .env("XSHI_PROFILE_PATH", &profile)
        .env("PATH", "")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child
                .stdin
                .take()
                .expect("child stdin")
                .write_all(b"xshi-profile-probe\nexit\n")?;
            child.wait_with_output()
        })
        .expect("run login xshi");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("from-profile\n"), "{stdout}");

    let output = Command::new(env!("CARGO_BIN_EXE_xshi"))
        .args(["--no-config", "-c", "xshi-profile-probe"])
        .env("XSHI_ALLOW_NON_TTY_FOR_TESTS", "1")
        .env("XSHI_PROFILE_PATH", &profile)
        .env("PATH", "")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run non-login xshi -c");

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains("from-profile\n"), "{stdout}");
}

#[test]
fn interactive_false_exit_uses_last_builtin_status() {
    let output = Command::new(env!("CARGO_BIN_EXE_xshi"))
        .arg("--no-config")
        .env("XSHI_ALLOW_NON_TTY_FOR_TESTS", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child
                .stdin
                .take()
                .expect("child stdin")
                .write_all(b"false\nexit\n")?;
            child.wait_with_output()
        })
        .expect("run interactive xsh");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn interactive_invalid_assignment_prefix_is_usage_error() {
    let home = temp_path("interactive-invalid-assignment");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("create home");

    let output = run_xshi_input(
        &["--no-config"],
        "BAD-NAME=value\nexit\n",
        Some(home.as_path()),
    );

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("invalid environment assignment"),
        "{stderr}"
    );

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_rejects_deferred_shell_comment_syntax() {
    let home = temp_path("interactive-shell-comment");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("create home");

    let output = run_xshi_input(
        &["--no-config"],
        "echo hi # later\nexit\n",
        Some(home.as_path()),
    );

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("shell comments"), "{stderr}");

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_plain_core_name_resolves_from_path() {
    let home = temp_path("interactive-plain-compat-in-process");
    let bin = home.join("bin");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&bin).expect("create bin");
    std::fs::write(home.join("marker.txt"), "").expect("write marker");
    write_test_executable(&bin.join("ls"), "#!/bin/sh\nprintf 'external-ls\\n'\n");

    let input = format!(
        "set PATH {}\ncd {}\nls\nexit\n",
        bin.display(),
        home.display()
    );
    let output = run_xshi_input(&["--no-config"], &input, Some(home.as_path()));

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("external-ls"), "{stdout}");
    assert!(!stdout.contains("marker.txt"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_rm_resolves_as_path_command() {
    let home = temp_path("interactive-rm-compat");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("create temp dir");
    let target = home.join("foo");
    std::fs::write(&target, "remove me").expect("write target");

    let input = format!("cd {}\nrm foo\nexit\n", home.display());
    let output = run_xshi_input(&["--no-config"], &input, Some(home.as_path()));

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    assert!(!target.exists());

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_sudo_core_command_is_not_rewritten_through_shim() {
    let home = temp_path("interactive-sudo-compat-shim");
    let bin = home.join("bin");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&bin).expect("create bin");
    write_test_executable(&bin.join("sudo"), "#!/bin/sh\nprintf '%s\\n' \"$@\"\n");

    let input = format!("set PATH {}\nsudo ls -l\nexit\n", bin.display());
    let output = run_xshi_input(&["--no-config"], &input, Some(home.as_path()));

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("ls\n-l\n"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_sudo_non_builtin_commands_are_not_rewritten() {
    let home = temp_path("interactive-sudo-non-builtin");
    let bin = home.join("bin");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&bin).expect("create bin");
    write_test_executable(&bin.join("sudo"), "#!/bin/sh\nprintf '%s\\n' \"$@\"\n");

    let input = format!(
        "set PATH {}\nsudo /bin/ls\nsudo command-not-builtin\nexit\n",
        bin.display()
    );
    let output = run_xshi_input(&["--no-config"], &input, Some(home.as_path()));

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("/bin/ls\n"), "{stdout}");
    assert!(stdout.contains("command-not-builtin\n"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_cd_persists_for_later_lines() {
    let root = temp_path("interactive-cd");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create temp dir");
    let input = format!("cd {}\npwd\nexit\n", root.display());
    let output = Command::new(env!("CARGO_BIN_EXE_xshi"))
        .arg("--no-config")
        .env("XSHI_ALLOW_NON_TTY_FOR_TESTS", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child
                .stdin
                .take()
                .expect("child stdin")
                .write_all(input.as_bytes())?;
            child.wait_with_output()
        })
        .expect("run interactive xsh");

    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains(&format!("{}\n", root.display()))
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn interactive_cd_dash_uses_oldpwd() {
    let root = temp_path("interactive-cd-dash");
    let one = root.join("one");
    let two = root.join("two");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&one).expect("create first dir");
    std::fs::create_dir_all(&two).expect("create second dir");

    let input = format!("cd {}\ncd ../two\ncd -\npwd\nexit\n", one.display());
    let output = run_xshi_input(&["--no-config"], &input, Some(root.as_path()));

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains(&format!("{}\n", one.display())), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn interactive_bare_external_uses_shell_status() {
    let output = Command::new(env!("CARGO_BIN_EXE_xshi"))
        .arg("--no-config")
        .env("XSHI_ALLOW_NON_TTY_FOR_TESTS", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child
                .stdin
                .take()
                .expect("child stdin")
                .write_all(b"/bin/sh -c \"exit 7\"\nexit\n")?;
            child.wait_with_output()
        })
        .expect("run interactive xsh");

    assert_eq!(output.status.code(), Some(7));
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn interactive_shell_chains_use_previous_status() {
    let home = temp_path("interactive-shell-chains");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("create home");

    let output = run_xshi_input(
        &["--no-config"],
        "/bin/sh -c \"exit 5\" || echo fallback\n/bin/sh -c \"exit 0\" && echo ok\nexit\n",
        Some(home.as_path()),
    );

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("fallback\n"), "{stdout}");
    assert!(stdout.contains("ok\n"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_true_false_shell_forms_do_not_force_xsh_parse() {
    let home = temp_path("interactive-bool-shell-forms");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("create home");

    let output = run_xshi_input(
        &["--no-config"],
        "true && echo ok\nfalse || echo fallback\nexit\n",
        Some(home.as_path()),
    );

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("ok\n"), "{stdout}");
    assert!(stdout.contains("fallback\n"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_colon_noop_supports_shell_link_rules() {
    let home = temp_path("interactive-colon-noop");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("create home");

    let output = Command::new(env!("CARGO_BIN_EXE_xshi"))
        .args(["--no-config", "-c", ": && echo ok && :"])
        .env("HOME", &home)
        .output()
        .expect("run xshi");

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "ok\n");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_which_resolves_commands_without_hiding_type_defs() {
    let home = temp_path("interactive-which-resolution");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("create home");

    let output = run_xshi_input(
        &["--no-config"],
        "which echo\nw echo\ntype Level = Info | Warn\nexit\n",
        Some(home.as_path()),
    );

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("echo"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_pipeline_status_uses_pipefail() {
    let home = temp_path("interactive-pipefail");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("create home");

    let output = run_xshi_input(
        &["--no-config"],
        "/bin/sh -c \"exit 1\" | /bin/sh -c \"exit 0\"\nexit\n",
        Some(home.as_path()),
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_session_builtins_are_rejected_in_pipelines() {
    let home = temp_path("interactive-session-pipeline");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("create home");

    let output = run_xshi_input(&["--no-config"], "cd . | pwd\nexit\n", Some(home.as_path()));

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("session builtins cannot be used in pipelines"),
        "{stderr}"
    );

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_shell_redirections_use_session_cwd() {
    let home = temp_path("interactive-redirections");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("create home");

    let input = format!(
        "cd {}\n/usr/bin/printf hi > out\ncat < out\nexit\n",
        home.display()
    );
    let output = run_xshi_input(&["--no-config"], &input, Some(home.as_path()));

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("hi"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_no_config_skips_config_aliases() {
    let home = temp_path("interactive-no-config");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".config/xshi")).expect("create config dir");
    std::fs::write(
        home.join(".config/xshi/config.ini"),
        "[aliases]\nxshi_probe_no_config = echo from-config\n",
    )
    .expect("write config");

    let output = run_xshi_input(
        &["--no-config"],
        "xshi_probe_no_config\nexit\n",
        Some(home.as_path()),
    );

    assert_eq!(output.status.code(), Some(127));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains("from-config"), "{stdout}");

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_loads_data_config_aliases() {
    let home = temp_path("interactive-loads-config");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".config/xshi")).expect("create config dir");
    std::fs::write(
        home.join(".config/xshi/config.ini"),
        "[aliases]\nxshi_probe_config = echo from-config\n",
    )
    .expect("write config");

    let output = run_xshi_input(&[], "xshi_probe_config\nexit\n", Some(home.as_path()));

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("from-config\n"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_z_jumps_to_history_directory() {
    let home = temp_path("interactive-z");
    let target = home.join("work/project-alpha");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".local/share/xshi")).expect("create history dir");
    std::fs::create_dir_all(&target).expect("create target");
    std::fs::write(
        home.join(".local/share/xshi/history"),
        format!("cd {}\n", target.display()),
    )
    .expect("write history");

    let output = run_xshi_input(
        &["--no-config"],
        "z alpha\npwd\nexit\n",
        Some(home.as_path()),
    );

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains(&format!("{}\n", target.display())),
        "{stdout}"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_denv_allow_applies_and_unloads_dotenv() {
    let home = temp_path("interactive-denv");
    let project = home.join("project");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&project).expect("create project");
    std::fs::create_dir(project.join(".git")).expect("create git marker");
    std::fs::write(project.join(".env"), "XSHI_DENV_PROBE=loaded\n").expect("write dotenv");

    let input = format!(
        "cd {}\ndenv allow\nprintenv XSHI_DENV_PROBE\ncd ..\nprintenv XSHI_DENV_PROBE || echo unloaded\nexit\n",
        project.display()
    );
    let output = run_xshi_input(&["--no-config"], &input, Some(home.as_path()));

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("denv: +XSHI_DENV_PROBE"), "{stdout}");
    assert!(stdout.contains("loaded\n"), "{stdout}");
    assert!(stdout.contains("unloaded\n"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_denv_allow_reports_missing_sources() {
    let home = temp_path("interactive-denv-missing");
    let project = home.join("project");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(project.join(".git")).expect("create git marker");
    std::fs::create_dir_all(&project).expect("create project");

    let input = format!("cd {}\ndenv allow\nexit\n", project.display());
    let output = run_xshi_input(&["--no-config"], &input, Some(home.as_path()));

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("denv: no .env/.envrc found"), "{stderr}");

    let _ = std::fs::remove_dir_all(home);
}

#[test]
#[ignore = "flaky: depends on filesystem source-appearance timing"]
fn interactive_denv_refreshes_dirty_marker_when_sources_appear() {
    let home = temp_path("interactive-denv-refresh");
    let repo = home.join("project");
    let work = repo.join("work");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&work).expect("create workdir");
    std::fs::create_dir(repo.join(".git")).expect("create git marker");

    let mut pty = spawn_xshi_pty_with(Some(&work), &[("HOME", home.as_path())]);
    let mut transcript = pty.read_until("$ ", PTY_TIMEOUT);
    pty.write(b"touch ../.env; echo denv-source-ready\r");
    let sentinel = "denv-source-ready\r\n";
    transcript.push_str(&pty.read_until(sentinel, PTY_TIMEOUT));
    let prompt_after_sentinel = transcript
        .rsplit_once(sentinel)
        .is_some_and(|(_, tail)| tail.contains("$ "));
    if !prompt_after_sentinel {
        transcript.push_str(&pty.read_until("$ ", PTY_TIMEOUT));
    }
    pty.write(b"exit\r");
    let output = pty.wait(PTY_TIMEOUT, &transcript);

    assert!(output.status.success(), "{transcript}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    let screen = terminal_screen(&transcript);
    assert!(
        screen.iter().any(|line| line.contains(" * $ ")),
        "{screen:?}\ntranscript={transcript:?}"
    );

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_l_lists_hidden_entries_without_external_ls() {
    let home = temp_path("interactive-listing");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("create home");
    std::fs::write(home.join("visible.txt"), "x").expect("write visible");
    std::fs::write(home.join(".hidden"), "x").expect("write hidden");

    let input = format!("cd {}\nl\nexit\n", home.display());
    let output = run_xshi_input(&["--no-config"], &input, Some(home.as_path()));

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("visible.txt"), "{stdout}");
    assert!(stdout.contains(".hidden"), "{stdout}");
    let visible_line = stdout
        .lines()
        .find(|line| line.contains("visible.txt"))
        .expect("visible entry");
    let hidden_line = stdout
        .lines()
        .find(|line| line.contains(".hidden"))
        .expect("hidden entry");
    let visible_line = visible_line
        .rsplit_once("$ ")
        .map(|(_, line)| line)
        .unwrap_or(visible_line);
    let hidden_line = hidden_line
        .rsplit_once("$ ")
        .map(|(_, line)| line)
        .unwrap_or(hidden_line);
    let visible_col = visible_line.find("visible.txt").expect("visible column");
    let hidden_col = hidden_line.find(".hidden").expect("hidden column");
    assert_eq!(visible_col, hidden_col, "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_l_refreshes_after_compat_command_may_mutate_cwd() {
    let home = temp_path("interactive-listing-invalidate");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("create home");
    std::fs::write(home.join("before.txt"), "x").expect("write initial file");

    let input = format!("cd {}\ntouch after.txt\nl\nexit\n", home.display());
    let output = run_xshi_input(&["--no-config"], &input, Some(home.as_path()));

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("before.txt"), "{stdout}");
    assert!(stdout.contains("after.txt"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_expands_words_for_session_and_compat_commands() {
    let home = temp_path("interactive-word-expansion");
    let subdir = home.join("subdir");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&subdir).expect("create home subdir");

    let output = run_xshi_input(
        &["--no-config"],
        "set XSHI_WORD world\necho hello-$XSHI_WORD\ncd ~/subdir\npwd\nexit\n",
        Some(home.as_path()),
    );

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("hello-world\n"), "{stdout}");
    assert!(
        stdout.contains(&format!("{}\n", subdir.display())),
        "{stdout}"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_expands_env_assignment_values() {
    let home = temp_path("interactive-env-expansion");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("create home");

    let output = run_xshi_input(
        &["--no-config"],
        "set XSHI_SOURCE bar\nXSHI_DEST=$XSHI_SOURCE\nprintenv XSHI_DEST\nexit\n",
        Some(home.as_path()),
    );

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("bar\n"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_aliases_expand_in_pipelines() {
    let home = temp_path("interactive-pipeline-alias");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("create home");

    let output = run_xshi_input(
        &["--no-config"],
        "alias count=\"wc -c\"\n/usr/bin/printf hi | count\nexit\n",
        Some(home.as_path()),
    );

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("2\n"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_quotes_control_expansion() {
    let home = temp_path("interactive-quotes");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("create home");

    let output = run_xshi_input(
        &["--no-config"],
        "set XSHI_QUOTE hi\necho '$XSHI_QUOTE' \"$XSHI_QUOTE\" \\$XSHI_QUOTE \"\\$XSHI_QUOTE\"\nexit\n",
        Some(home.as_path()),
    );

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("$XSHI_QUOTE hi $XSHI_QUOTE $XSHI_QUOTE\n"),
        "{stdout}"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_globs_are_sorted_and_skip_dotfiles_by_default() {
    let home = temp_path("interactive-globs");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("create home");
    std::fs::write(home.join("b.txt"), "").expect("write b");
    std::fs::write(home.join("a.txt"), "").expect("write a");
    std::fs::write(home.join(".hidden.txt"), "").expect("write hidden");

    let input = format!("cd {}\necho *.txt\necho .*.txt\nexit\n", home.display());
    let output = run_xshi_input(&["--no-config"], &input, Some(home.as_path()));

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("a.txt b.txt\n"), "{stdout}");
    assert!(stdout.contains(".hidden.txt\n"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_globstar_crosses_directories() {
    let home = temp_path("interactive-globstar");
    let nested = home.join("one/two");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&nested).expect("create nested");
    std::fs::write(home.join("root.log"), "").expect("write root");
    std::fs::write(nested.join("deep.log"), "").expect("write deep");

    let input = format!("cd {}\necho **/*.log\nexit\n", home.display());
    let output = run_xshi_input(&["--no-config"], &input, Some(home.as_path()));

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("one/two/deep.log"), "{stdout}");
    assert!(stdout.contains("root.log"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_no_match_glob_is_usage_error() {
    let home = temp_path("interactive-no-match-glob");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("create home");

    let input = format!("cd {}\necho *.missing\nexit\n", home.display());
    let output = run_xshi_input(&["--no-config"], &input, Some(home.as_path()));

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("glob pattern matched no paths"), "{stderr}");

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_command_substitution_expands_stdout() {
    let home = temp_path("interactive-command-substitution");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("create home");

    let output = run_xshi_input(
        &["--no-config"],
        "echo before-$(printf sub)-after\necho `printf tick`\nexit\n",
        Some(home.as_path()),
    );

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("subbefore--after\n"), "{stdout}");
    assert!(stdout.contains("tick\n"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_command_substitution_status_stops_outer_command() {
    let home = temp_path("interactive-command-substitution-status");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("create home");

    let output = run_xshi_input(
        &["--no-config"],
        "echo before $(/bin/sh -c \"exit 7\") after\nexit\n",
        Some(home.as_path()),
    );

    assert_eq!(output.status.code(), Some(7));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains("before"), "{stdout}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("command substitution failed"), "{stderr}");

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_quoted_command_substitution_does_not_glob() {
    let home = temp_path("interactive-command-substitution-quoted");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("create home");
    std::fs::write(home.join("match.txt"), "").expect("write match");

    let input = format!(
        "cd {}\necho \"$(printf '*.txt')\"\necho $(printf '*.txt')\nexit\n",
        home.display()
    );
    let output = run_xshi_input(&["--no-config"], &input, Some(home.as_path()));

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("*.txt\n"), "{stdout}");
    assert_eq!(stdout.matches("*.txt\n").count(), 2, "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn interactive_shell_chains_do_not_fall_back_for_xsh_reserved_input() {
    let output = Command::new(env!("CARGO_BIN_EXE_xshi"))
        .arg("--no-config")
        .env("XSHI_ALLOW_NON_TTY_FOR_TESTS", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child
                .stdin
                .take()
                .expect("child stdin")
                .write_all(b"let nope =\necho after\nexit\n")?;
            child.wait_with_output()
        })
        .expect("run interactive xsh");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("after\n"), "{stdout}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("parse."), "{stderr}");
}

#[test]
fn interactive_cat_without_operands_rejects_repl_stdin() {
    let output = Command::new(env!("CARGO_BIN_EXE_xshi"))
        .arg("--no-config")
        .env("XSHI_ALLOW_NON_TTY_FOR_TESTS", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child
                .stdin
                .take()
                .expect("child stdin")
                .write_all(b"cat\nexit\n")?;
            child.wait_with_output()
        })
        .expect("run interactive xsh");

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let output = Command::new(env!("CARGO_BIN_EXE_xshi"))
        .arg("--no-config")
        .env("XSHI_ALLOW_NON_TTY_FOR_TESTS", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child
                .stdin
                .take()
                .expect("child stdin")
                .write_all(b"cat\necho after\nexit\n")?;
            child.wait_with_output()
        })
        .expect("run interactive xsh");

    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("after\n")
    );
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
    let home = temp_path("xsh-ignores-xshi-config");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".config/xshi")).expect("create config dir");
    std::fs::write(
        home.join(".config/xshi/config.ini"),
        "[aliases]\necho = print\n",
    )
    .expect("write config");

    let path = temp_xsh_path("xsh-ignores-interactive-config");
    std::fs::write(&path, "echo hi\n").expect("write temp script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .env("HOME", &home)
        .arg(path.to_str().unwrap())
        .output()
        .expect("run xsh");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("err[check.unresolved-proc-command]"));
    assert!(!home.join(".local/share/xshi/history").exists());

    std::fs::remove_file(path).expect("remove temp script");
    let _ = std::fs::remove_dir_all(home);
}
