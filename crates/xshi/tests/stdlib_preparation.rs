//! `xshi` prepares each submitted input through the embedded
//! standard-library preparation boundary.
//!
//! Every submitted input crosses its own preparation boundary: the input is
//! parsed, checked, and lowered — including the embedded implementations it
//! can reach — before it executes. Repeated submissions in one process must
//! keep working, and a failed submission must not leave state that breaks the
//! next one.

use xshi::{OneCommandOptions, run_one_command_with_options};

fn run(source: &str) -> i32 {
    run_one_command_with_options(
        source,
        OneCommandOptions {
            load_config: false,
            load_profile: false,
        },
    )
}

#[test]
fn each_submitted_input_prepares_and_runs_embedded_implementations() {
    // The first submission reaches an embedded implementation.
    assert_eq!(run("print shlex.quote(\"a b\")"), 0);

    // Later submissions in the same process must prepare and run again rather
    // than depend on state carried from the previous input.
    assert_eq!(run("print bytes.human(4096)"), 0);
    assert_eq!(run("print tui.left_pad(\"x\", 4)"), 0);
    assert_eq!(run("print hash.parse_check_line(\"aa  b\")?.hex"), 0);
    assert_eq!(run("print shlex.quote(\"a b\")"), 0);

    // A submission that reports a runtime failure must not poison the next
    // one. `xshi -c` reports the failure on stderr and keeps its own exit
    // status, so the sequencing is what this asserts.
    let _ = run("let x = 1 / 0");
    assert_eq!(run("print shlex.quote(\"after-failure\")"), 0);

    // Ordinary shell inputs keep their own route.
    assert_eq!(run("true"), 0);
    assert_eq!(run("print shlex.quote(\"after-shell\")"), 0);
}
