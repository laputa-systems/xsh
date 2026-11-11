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
