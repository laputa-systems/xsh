//! Editor scenarios ported from ish's `src/editor.rs`: undo/redo, suggestion
//! word accept, the recent-directory picker, word kills, and the OSC 133
//! prompt/command markers. Goldens are recorded from ish like every scenario.

use super::scenarios::parity;
use super::{
    Cols, Fixture, Launch, Live, PINNED_HOST, ScratchHome, ShellKind, USER, first_difference,
    golden_path, host_for, ish_binary, key, materialize, prompt_width_at_home, run_scenario,
};
use ptytest::{CommandSpec, ProtocolProfile, PtyTest, Scenario, Size, TestEnv};
use std::path::Path;

const CTRL_UNDERSCORE: &[u8] = b"\x1f";
const ALT_R: &[u8] = b"\x1br";

parity!(
    editor_undo_redo_typing_paste_and_cursor,
    Fixture::new(),
    |sh: &mut Live| {
        sh.text("echo hi");
        sh.frame("typed");
        sh.keys(CTRL_UNDERSCORE);
        sh.frame("undo_typing_group");
        sh.keys(ALT_R);
        sh.frame("redo_typing_group");
        sh.keys(&crate::paste(" there"));
        sh.frame("pasted");
        sh.keys(CTRL_UNDERSCORE);
        sh.frame("undo_paste");
        sh.keys(ALT_R);
        sh.frame("redo_paste");
        sh.repeat(key::LEFT, 2);
        sh.text("X");
        sh.frame("typed_mid_line");
        sh.keys(CTRL_UNDERSCORE);
        sh.frame("undo_restores_cursor");
        sh.keys(CTRL_UNDERSCORE);
        sh.frame("undo_previous_step");
        sh.keys(ALT_R);
        sh.frame("redo_previous_step");
        sh.keys(key::CTRL_W);
        sh.frame("killed_word");
        sh.keys(CTRL_UNDERSCORE);
        sh.frame("undo_kill");
    }
);

parity!(
    editor_ctrl_right_accepts_suggestion_words,
    Fixture::new().history(&["git checkout feature-branch"]),
    |sh: &mut Live| {
        sh.text("git ch");
        sh.frame("suggested");
        sh.keys(key::CTRL_RIGHT);
        sh.frame("first_word");
        sh.keys(key::CTRL_RIGHT);
        sh.frame("second_word");
        sh.keys(key::CTRL_RIGHT);
        sh.frame("at_end");
        sh.keys(key::CTRL_U);
        sh.text("git ch");
        sh.keys(key::RIGHT);
        sh.frame("whole_suggestion");
        sh.keys(key::CTRL_U);
        sh.text("echo gi");
        sh.keys(key::CTRL_RIGHT);
        sh.frame("no_suggestion");
    }
);

parity!(
    editor_ctrl_backspace_directory_picker_accepts,
    Fixture::new()
        .file("one/.keep", "")
        .file("two/.keep", "")
        .file("three/.keep", ""),
    |sh: &mut Live| {
        sh.line("cd one");
        sh.line("cd ../two");
        sh.line("cd ../three");
        sh.keys(key::CTRL_BACKSPACE);
        sh.frame("picker");
        sh.keys(key::DOWN);
        sh.frame("second_entry");
        sh.keys(key::UP);
        sh.keys(key::ENTER);
        sh.frame("accepted");
        sh.line("pwd");
        sh.frame("after_pwd");
    }
);

parity!(
    editor_ctrl_delete_and_alt_d_word_kills,
    Fixture::new(),
    |sh: &mut Live| {
        sh.text("echo alpha beta");
        sh.keys(key::CTRL_DELETE);
        sh.frame("ctrl_delete");
        sh.keys_to_prompt(key::ENTER);
        sh.frame("after_enter");
        sh.text("one two three");
        sh.keys(key::CTRL_A);
        sh.keys(key::ALT_D);
        sh.frame("alt_d_at_start");
        sh.keys(key::CTRL_Y);
        sh.frame("yanked");
        sh.keys(key::CTRL_U);
        sh.text("alpha beta gamma");
        sh.repeat(key::ALT_B, 2);
        sh.keys(key::ALT_D);
        sh.frame("alt_d_middle");
        sh.keys(CTRL_UNDERSCORE);
        sh.frame("undo_kill");
        sh.keys(key::CTRL_W);
        sh.frame("ctrl_w_after_undo");
    }
);

fn osc133_markers(sh: &Live) -> Vec<String> {
    let raw = sh.term.raw_output();
    let mut markers = Vec::new();
    let mut start = 0;
    while let Some(offset) = raw[start..].windows(2).position(|w| w == b"\x1b]") {
        let at = start + offset;
        let rest = &raw[at..];
        if !rest.starts_with(b"\x1b]133;") {
            start = at + 2;
            continue;
        }
        let Some(end) = rest.iter().position(|byte| *byte == 0x07) else {
            break;
        };
        markers.push(String::from_utf8_lossy(&rest[6..end]).into_owned());
        start = at + end + 1;
    }
    markers
}

fn record_osc133(sh: &mut Live, name: &str) {
    let markers = osc133_markers(sh);
    sh.effects.push((name.to_owned(), markers.join("\n")));
}

/// The parity driver's profile rejects OSC 133, so the marker scenario spawns
/// its own PTY with protocol validation disabled, as ish's OSC tests do.
fn spawn_violation_free(kind: ShellKind, binary: &Path, fixture: &Fixture) -> Live {
    let scratch = ScratchHome::new();
    let home = scratch.home.clone();
    materialize(&home, fixture, kind);
    let cwd = fixture
        .cwd
        .as_deref()
        .map_or_else(|| home.clone(), |relative| home.join(relative));
    let host = host_for(kind);
    let cols = match fixture.cols {
        Cols::Absolute(cols) => cols,
        Cols::PromptPlus(extra) => prompt_width_at_home(&host) + extra,
    };
    let launch = Launch {
        binary: binary.to_path_buf(),
        cwd,
        size: (cols, fixture.rows),
        env: fixture.env.clone(),
        args: fixture.args.clone(),
        launcher: None,
    };
    let environment = if cfg!(target_os = "linux") {
        TestEnv::hermetic_ascii()
    } else {
        TestEnv::hermetic_utf8("C.UTF-8")
    }
    .expect("a hermetic locale must be available on PTY test platforms");
    let mut command = CommandSpec::new(&launch.binary)
        .current_dir(&launch.cwd)
        .env("HOME", &home)
        .env("USER", USER)
        .env("PWD", &launch.cwd)
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .env("XSHI_PROFILE_PATH", "/dev/null")
        .env("XSHI_HOSTNAME", PINNED_HOST)
        .env("ISH_TEST_SUPPRESS_OSC133", "1");
    for arg in &launch.args {
        command = command.arg(arg);
    }
    for (name, value) in &launch.env {
        command = command.env(name, value);
    }
    let mut environment = environment
        .env("HOME", &home)
        .env("USER", USER)
        .env("PWD", &launch.cwd)
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .env("XSHI_PROFILE_PATH", "/dev/null")
        .env("XSHI_HOSTNAME", PINNED_HOST)
        .env("ISH_TEST_SUPPRESS_OSC133", "1");
    for (name, value) in &launch.env {
        environment = environment.env(name, value);
    }
    let scenario = Scenario::new(format!("parity osc133 {}", kind.label()))
        .expect("valid scenario label")
        .command(command)
        .size(Size::new(launch.size.0, launch.size.1).expect("non-zero terminal size"))
        .environment(environment)
        .protocol_profile(ProtocolProfile::disabled());
    let term = PtyTest::spawn(scenario).expect("spawn shell through ptytest");
    let baseline = term.terminal_baseline();
    let mut live = Live {
        kind,
        term,
        baseline,
        launch,
        _scratch: Some(scratch),
        home,
        host,
        prompts_seen: 0,
        frames: Vec::new(),
        effects: Vec::new(),
        exited: None,
    };
    live.wait_prompt();
    live
}

fn run_violation_free(kind: ShellKind, binary: &Path, fixture: &Fixture) -> String {
    let mut live = spawn_violation_free(kind, binary, fixture);
    osc133_script(&mut live);
    live.finish().render()
}

fn run_violation_free_with(
    kind: ShellKind,
    binary: &Path,
    fixture: &Fixture,
    script: fn(&mut Live),
) -> String {
    let mut live = spawn_violation_free(kind, binary, fixture);
    script(&mut live);
    live.finish().render()
}

/// OSC 133 is outside the driver's protocol profile, so these scenarios use
/// the same profile-disabled PTY as ish's own marker tests.
fn run_osc_scenario(name: &str, fixture: &Fixture, script: fn(&mut Live)) {
    let golden = golden_path(name);
    let record = std::env::var_os("XSHI_PARITY_RECORD").is_some();
    if let Some(ish) = ish_binary() {
        let from_ish = run_violation_free_with(ShellKind::Ish, &ish, fixture, script);
        if record {
            std::fs::create_dir_all(golden.parent().expect("golden parent"))
                .expect("golden dir");
            std::fs::write(&golden, &from_ish).expect("write golden");
        } else {
            let expected = std::fs::read_to_string(&golden)
                .unwrap_or_else(|error| panic!("read golden {}: {error}", golden.display()));
            assert!(
                expected == from_ish,
                "{name}: ish no longer matches its recorded golden\n{}",
                first_difference(&expected, &from_ish)
            );
        }
    }
    let expected = std::fs::read_to_string(&golden).unwrap_or_else(|error| {
        panic!(
            "{name}: read golden {}: {error}; record it with XSHI_PARITY_ISH_BIN and XSHI_PARITY_RECORD=1",
            golden.display()
        )
    });
    let xshi = std::path::PathBuf::from(env!("CARGO_BIN_EXE_xshi"));
    let actual = run_violation_free_with(ShellKind::Xshi, &xshi, fixture, script);
    if super::known_gap(name) {
        assert!(
            expected != actual,
            "{name}: xshi now matches the ish golden; remove it from {}",
            super::known_gaps_path().display()
        );
        return;
    }
    assert!(
        expected == actual,
        "{name}: xshi differs from the ish golden\n{}",
        first_difference(&expected, &actual)
    );
}

macro_rules! osc_parity {
    ($name:ident, $fixture:expr, $script:expr) => {
        #[test]
        fn $name() {
            let fixture: Fixture = $fixture;
            run_osc_scenario(stringify!($name), &fixture, $script);
        }
    };
}

fn osc52_payload(sh: &Live) -> String {
    let raw = sh.term.raw_output();
    let marker = b"\x1b]52;c;";
    let Some(position) = raw.windows(marker.len()).position(|word| word == marker) else {
        return "<missing>".to_owned();
    };
    let rest = &raw[position + marker.len()..];
    let end = rest
        .iter()
        .position(|byte| *byte == 0x07)
        .unwrap_or(rest.len());
    String::from_utf8_lossy(&rest[..end]).into_owned()
}

const READY_SLEEP: &str = "#!/bin/sh\necho ready\nsleep 60\n";

osc_parity!(
    osc133_marks_prompt_input_execution_and_completion,
    Fixture::new().env("ISH_TEST_SUPPRESS_OSC133", "0"),
    |sh: &mut Live| {
        sh.line("true");
        record_osc133(sh, "markers");
    }
);

osc_parity!(
    osc133_reports_command_status,
    Fixture::new().env("ISH_TEST_SUPPRESS_OSC133", "0"),
    |sh: &mut Live| {
        sh.line("/bin/sh -c 'exit 7'");
        record_osc133(sh, "markers");
    }
);

osc_parity!(
    osc133_cancelled_input_has_no_execution_marker,
    Fixture::new().env("ISH_TEST_SUPPRESS_OSC133", "0"),
    |sh: &mut Live| {
        sh.text("echo nope");
        sh.keys_to_prompt(key::CTRL_C);
        sh.line("echo after");
        record_osc133(sh, "markers");
    }
);

osc_parity!(
    osc133_refused_input_has_no_execution_marker,
    Fixture::new().env("ISH_TEST_SUPPRESS_OSC133", "0"),
    |sh: &mut Live| {
        sh.line("source /etc/profile");
        record_osc133(sh, "markers");
    }
);

osc_parity!(
    osc133_resize_does_not_duplicate_markers,
    Fixture::new().env("ISH_TEST_SUPPRESS_OSC133", "0"),
    |sh: &mut Live| {
        sh.line("true");
        sh.resize(30, 100);
        sh.line("true");
        record_osc133(sh, "markers");
    }
);

osc_parity!(
    osc133_redirected_output_keeps_markers_off_command_file,
    Fixture::new().env("ISH_TEST_SUPPRESS_OSC133", "0"),
    |sh: &mut Live| {
        sh.line("echo hi >out.txt");
        sh.effect_file("out_file", "out.txt");
        record_osc133(sh, "markers");
    }
);

osc_parity!(
    osc133_continuation_does_not_emit_extra_execution_markers,
    Fixture::new().env("ISH_TEST_SUPPRESS_OSC133", "0"),
    |sh: &mut Live| {
        sh.text("true |");
        sh.keys(key::ENTER);
        sh.text("cat");
        sh.keys_to_prompt(key::ENTER);
        record_osc133(sh, "markers");
    }
);

osc_parity!(
    osc133_suspended_job_and_fg_emit_completion_markers,
    Fixture::new()
        .env("ISH_TEST_SUPPRESS_OSC133", "0")
        .executable("sleeper.sh", READY_SLEEP),
    |sh: &mut Live| {
        sh.line_until("./sleeper.sh", "ready");
        sh.keys_to_prompt(key::CTRL_Z);
        record_osc133(sh, "stopped");
        sh.line_until("fg", "resuming:");
        sh.keys_to_prompt(key::CTRL_C);
        record_osc133(sh, "markers");
    }
);

osc_parity!(
    resize_storm_keeps_shell_responsive,
    Fixture::new().env("ISH_TEST_SUPPRESS_OSC133", "0"),
    |sh: &mut Live| {
        for (rows, cols) in [(24, 80), (30, 100), (20, 60), (40, 120), (24, 80), (10, 40)] {
            sh.resize(rows, cols);
        }
        sh.line("echo storm-ok");
        record_osc133(sh, "markers");
    }
);

osc_parity!(
    repeated_stop_resume_preserves_command_boundaries,
    Fixture::new()
        .env("ISH_TEST_SUPPRESS_OSC133", "0")
        .executable("s1.sh", READY_SLEEP)
        .executable("s2.sh", READY_SLEEP),
    |sh: &mut Live| {
        for script in ["$HOME/s1.sh", "$HOME/s2.sh"] {
            sh.line_until(script, "ready");
            sh.keys_to_prompt(key::CTRL_Z);
            sh.line_until("fg", "resuming:");
            sh.keys_to_prompt(key::CTRL_C);
        }
        record_osc133(sh, "markers");
    }
);

osc_parity!(
    osc52_copy_scrollback_emits_base64_session_log,
    Fixture::new().env("ISH_TEST_SUPPRESS_OSC133", "0"),
    |sh: &mut Live| {
        sh.line("echo seed");
        sh.line("copy-scrollback");
        let payload = osc52_payload(sh);
        sh.effects.push(("payload".to_owned(), payload));
    }
);

fn osc133_script(sh: &mut Live) {
    sh.line("true");
    sh.line("/bin/sh -c 'exit 7'");
    sh.text("echo nope");
    sh.keys_to_prompt(key::CTRL_C);
    sh.resize(30, 100);
    sh.line("echo hi >out.txt");
    sh.effect_file("out_file", "out.txt");
    sh.text("true |");
    sh.keys(key::ENTER);
    sh.text("cat");
    sh.keys_to_prompt(key::ENTER);
    sh.text("echo hi >");
    sh.keys_to_prompt(key::ENTER);
    record_osc133(sh, "markers");
}

#[test]
fn editor_osc133_prompt_command_markers() {
    let name = "editor_osc133_prompt_command_markers";
    let fixture = Fixture::new().env("ISH_TEST_SUPPRESS_OSC133", "0");
    let golden = golden_path(name);
    let record = std::env::var_os("XSHI_PARITY_RECORD").is_some();
    if let Some(ish) = ish_binary() {
        let from_ish = run_violation_free(ShellKind::Ish, &ish, &fixture);
        if record {
            std::fs::create_dir_all(golden.parent().expect("golden parent"))
                .expect("golden dir");
            std::fs::write(&golden, &from_ish).expect("write golden");
        } else {
            let expected = std::fs::read_to_string(&golden)
                .unwrap_or_else(|error| panic!("read golden {}: {error}", golden.display()));
            assert!(
                expected == from_ish,
                "{name}: ish no longer matches its recorded golden\n{}",
                first_difference(&expected, &from_ish)
            );
        }
    }
    let expected = std::fs::read_to_string(&golden).unwrap_or_else(|error| {
        panic!(
            "{name}: read golden {}: {error}; record it with XSHI_PARITY_ISH_BIN and XSHI_PARITY_RECORD=1",
            golden.display()
        )
    });
    let xshi = std::path::PathBuf::from(env!("CARGO_BIN_EXE_xshi"));
    let actual = run_violation_free(ShellKind::Xshi, &xshi, &fixture);
    assert!(
        expected == actual,
        "{name}: xshi differs from the ish golden\n{}",
        first_difference(&expected, &actual)
    );
}

parity!(
    comments_do_not_hold_complete_commands_in_continuation,
    Fixture::new(),
    |sh: &mut Live| {
        for (label, input) in [
            ("apostrophe", "echo okay # don't wait"),
            ("pipe", "echo okay # |"),
            ("backslash", "echo okay # trailing \\"),
            ("after_semicolon", "echo okay;# &&"),
            ("comment_line", "echo a\n# 'open\necho b"),
            ("quote_in_trailing_comment", "echo a # it's\necho b"),
            ("operator_before_comment_line", "echo a && # done\necho b"),
        ] {
            sh.keys(&crate::paste(input));
            sh.keys_to_prompt(key::ENTER);
            sh.frame(label);
        }
        for (label, input) in [
            ("hash_inside_word", "echo a#'b"),
            ("hash_inside_quotes", "echo 'a # b"),
            ("trailing_pipe", "echo a |"),
            ("operator_before_comment", "echo a && # done"),
            ("trailing_backslash", "echo a \\"),
        ] {
            sh.keys(&crate::paste(input));
            sh.enter();
            sh.frame(label);
            sh.keys_to_prompt(key::CTRL_C);
        }
    }
);

fn partial_budget_fixture() -> Fixture {
    let mut fixture = Fixture::new().size(40, 80);
    for branch in 0..70 {
        fixture = fixture.file(&format!("branch{branch:02}/item{branch:02}"), "");
    }
    fixture
}

parity!(
    partial_path_completion_through_the_editor_is_bounded_to_the_directory_budget,
    partial_budget_fixture(),
    |sh: &mut Live| {
        sh.text("cat br/item");
        sh.keys(key::TAB);
        sh.frame("grid");
    }
);

parity!(
    completion_and_history_acceptance_are_single_undoable_edits,
    Fixture::new()
        .file("alpha", "")
        .file("alpine", "")
        .file("unique", "")
        .history(&["echo recalled"]),
    |sh: &mut Live| {
        sh.keys(&crate::paste("echo un"));
        sh.keys(key::TAB);
        sh.frame("completed_single");
        sh.keys(CTRL_UNDERSCORE);
        sh.frame("undo_completion");
        sh.keys(ALT_R);
        sh.frame("redo_completion");
        sh.keys(CTRL_UNDERSCORE);
        sh.keys(CTRL_UNDERSCORE);
        sh.frame("undo_paste");
        sh.keys_to_prompt(key::CTRL_C);

        sh.keys(&crate::paste("echo al"));
        sh.keys(key::TAB);
        sh.keys(key::TAB);
        sh.keys(key::ENTER);
        sh.frame("accepted_from_grid");
        sh.keys(CTRL_UNDERSCORE);
        sh.frame("undo_accepted");
        sh.keys_to_prompt(key::CTRL_C);

        sh.keys(&crate::paste("echo al"));
        sh.keys(key::TAB);
        sh.keys(key::TAB);
        sh.keys(key::ESC);
        sh.frame("cancelled");
        sh.keys(CTRL_UNDERSCORE);
        sh.frame("undo_after_cancel");
        sh.keys_to_prompt(key::CTRL_C);

        sh.keys(&crate::paste("x"));
        sh.keys(CTRL_UNDERSCORE);
        sh.keys(key::CTRL_R);
        sh.keys(&crate::paste("recalled"));
        sh.keys(key::ENTER);
        sh.frame("history_accepted");
        sh.keys(ALT_R);
        sh.frame("redo_after_history");
        sh.keys(CTRL_UNDERSCORE);
        sh.frame("undo_history");
    }
);

parity!(
    completion_replaces_the_whole_word_around_the_cursor,
    Fixture::new().file("alpha", ""),
    |sh: &mut Live| {
        for (label, input, lefts) in [
            ("complete_word", "cat alpha", 3),
            ("closing_quote", "cat 'al'", 1),
            ("later_argument", "cat alp next", 5),
            ("escaped_suffix", "cat al\\ x next", 8),
        ] {
            sh.keys(&crate::paste(input));
            sh.repeat(key::LEFT, lefts);
            sh.keys(key::TAB);
            sh.frame(label);
            sh.keys_to_prompt(key::ENTER);
            sh.frame(&format!("{label}_submitted"));
        }
    }
);
