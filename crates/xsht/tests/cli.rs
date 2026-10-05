#![allow(clippy::single_call_fn)]

use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn xsht_reports_non_utf8_argument_without_panicking() {
    let raw_path = std::ffi::OsString::from_vec(b"raw\xffpath.xsh".to_vec());
    let output = Command::new(release_bin!("xsht"))
        .arg("check")
        .arg(raw_path)
        .output()
        .expect("run xsht");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "xsht: argument 2 is not valid UTF-8\n"
    );
}

#[test]
fn copied_xsht_formats_and_lints_script_backed_calls_in_static_and_loaded_modules() {
    let root = TempDir::new().expect("create isolated source root");
    let xsht = root.path().join("xsht");
    fs::copy(release_bin!("xsht"), &xsht).expect("copy xsht");
    fs::write(
        root.path().join("helper.xsh"),
        "##! Static helper.\n## Return a terminal sequence.\nexport pure color() -> Str { tui.red() }\n",
    )
    .expect("write static module");
    fs::write(
        root.path().join("dynamic.xsh"),
        "##! Dynamic helper.\n## Return a terminal sequence.\nexport pure color() -> Str { tui.bold() }\n",
    )
    .expect("write loaded module");
    fs::write(
        root.path().join("main.xsh"),
        "use helper\ntype Loaded = module { export pure color() -> Str }\nproc main() [fs, io, error] {\n  let loaded = module.load(p\"dynamic.xsh\")?.require(Loaded)?\n  let both = helper.color() + loaded.color()\n  print $both\n}\n",
    )
    .expect("write entry script");

    for args in [
        vec!["check", "."],
        vec!["fmt", "."],
        vec!["fmt", "--check", "."],
        vec!["lint", "."],
    ] {
        let output = Command::new(&xsht)
            .args(&args)
            .current_dir(root.path())
            .env_remove("XSH_MODULE_PATH")
            .output()
            .expect("run copied xsht");
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty(), "{}", args.join(" "));
        // `check` and `lint` close stderr with their timing line; `fmt` prints nothing.
        let stderr = match args[0] {
            "fmt" => std::str::from_utf8(&output.stderr).expect("UTF-8 stderr"),
            command => crate::stderr_before_timing_line(command, &output.stderr),
        };
        assert!(stderr.is_empty(), "{}: {stderr}", args.join(" "));
    }
}

#[test]
fn test_reports_lazy_default_runtime_failure_without_panicking() {
    let root = TempDir::new().expect("create temp root");
    let tests = root.path().join("tests");
    fs::create_dir(&tests).expect("create tests directory");
    fs::write(
        tests.join("lowering.xsh"),
        "pure helper(x: Int = 1 / 0) -> Int {\n  return x\n}\n\ntest test_lowering {\n  let _ = helper()\n}\n",
    )
    .expect("write test script");

    for filter in ["tests/lowering.xsh", "test_lowering"] {
        let output = Command::new(release_bin!("xsht"))
            .args(["test", filter])
            .current_dir(root.path())
            .output()
            .expect("run xsht test");

        assert_eq!(
            output.status.code(),
            Some(1),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("division by zero"), "stdout: {stdout}");
        assert!(
            !stdout.contains("compact.indexed-build"),
            "stdout: {stdout}"
        );
    }
}

fn processes_with_marker(marker: &str) -> Vec<String> {
    let listing = Command::new("ps")
        .args(["-A", "-o", "pid=,args="])
        .output()
        .expect("list processes");
    String::from_utf8_lossy(&listing.stdout)
        .lines()
        .filter(|line| line.contains(marker))
        .map(str::to_owned)
        .collect()
}

#[test]
fn test_runner_cancellation_stops_run_script_descendants() {
    for signal in [libc::SIGTERM, libc::SIGINT] {
        let root = TempDir::new().expect("temporary descendant fixture");
        let marker = format!("xsht-descendant-{}-{signal}", std::process::id());
        fs::create_dir(root.path().join("tests")).expect("create test root");
        fs::write(
            root.path().join("tests/spawn.xsh"),
            format!(
                "\
test spawns_descendants {{ |ctx|
  let output = test.run_script(ctx, \"\"\"
run sh -c \"sleep 300; : {marker}-grandchild\"
\"\"\", [\"{marker}-child\"])?
  assert output.success
}}
"
            ),
        )
        .expect("write descendant fixture");
        let mut runner = Command::new(release_bin!("xsht"))
            .args(["test", "--jobs", "1"])
            .current_dir(root.path())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("start xsht test");
        let started = std::time::Instant::now();
        loop {
            let found = processes_with_marker(&marker);
            if found.iter().any(|line| line.contains("-child"))
                && found.iter().any(|line| line.contains("-grandchild"))
            {
                break;
            }
            if started.elapsed() > std::time::Duration::from_secs(120)
                || runner.try_wait().expect("poll runner").is_some()
            {
                let _ = runner.kill();
                panic!("descendants never started: {found:?}");
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert_eq!(unsafe { libc::kill(runner.id() as libc::pid_t, signal) }, 0);
        let status = runner.wait().expect("wait for runner");
        assert_eq!(
            std::os::unix::process::ExitStatusExt::signal(&status),
            None,
            "{status:?}"
        );
        assert_eq!(status.code(), Some(128 + signal));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !processes_with_marker(&marker).is_empty() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert_eq!(processes_with_marker(&marker), Vec::<String>::new());
    }
}

/// A signal during a long `xsht lint --fix` ends the run between files or
/// fix rounds. Fixed files are written only after every file is done, so an
/// interrupted run leaves each file exactly as it was.
///
/// The signal goes to a spawned `xsht`. A cancellation request is state of
/// the whole process, and the tests of this target run as threads of one
/// process that call the same commands, so a request raised here would
/// interrupt whichever of them was running.
#[test]
fn lint_fix_cancellation_writes_no_file() {
    let mut functions = String::new();
    for index in 0..1000 {
        functions.push_str(&format!(
            "pure pick_{index}(n: Int) -> Int {{\n  let chosen = match n {{\n    1 => 10,\n    _ => 20,\n  }}\n  chosen + 1\n}}\n\n"
        ));
    }
    functions.push_str("print ${pick_0(1)}\n");
    for signal in [libc::SIGTERM, libc::SIGINT] {
        let root = TempDir::new().expect("temporary lint fixture");
        for file in 0..96 {
            fs::write(root.path().join(format!("module_{file}.xsh")), &functions)
                .expect("write lint fixture");
        }
        let mut lint = Command::new(release_bin!("xsht"))
            .args(["lint", "--fix", "."])
            .current_dir(root.path())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("start xsht lint");
        // Long enough for the handlers to be installed, far shorter than the
        // run: every file has a thousand fixes.
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(
            lint.try_wait().expect("poll lint").is_none(),
            "the lint run ended before it could be interrupted; the fixture is too small"
        );
        assert_eq!(unsafe { libc::kill(lint.id() as libc::pid_t, signal) }, 0);
        let started = std::time::Instant::now();
        let output = lint.wait_with_output().expect("wait for lint");
        assert_eq!(
            output.status.code(),
            Some(128 + signal),
            "{:?}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "");
        let name = if signal == libc::SIGINT { "SIGINT" } else { "SIGTERM" };
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(&format!("interrupted by {name}")),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        // One file's check and one fix round at most separate two looks at
        // the signal.
        assert!(
            started.elapsed() < std::time::Duration::from_secs(10),
            "{:?}",
            started.elapsed()
        );
        for file in 0..96 {
            let path = root.path().join(format!("module_{file}.xsh"));
            assert!(
                fs::read_to_string(&path).expect("read lint fixture") == functions,
                "{} changed",
                path.display()
            );
        }
    }
}

#[test]
fn test_runner_times_out_hung_tests_and_stops_their_descendants() {
    let root = TempDir::new().expect("temporary timeout fixture");
    let marker = format!("xsht-timeout-{}", std::process::id());
    fs::create_dir(root.path().join("tests")).expect("create test root");
    fs::write(
        root.path().join("tests/hang.xsh"),
        format!(
            "\
test spins {{ |ctx|
  defer {{ print \"spins cleanup ran\" }}
  var n = 0
  while true {{ n += 1 }}
}}

test sleeping_script {{ |ctx|
  let output = test.run_script(ctx, \"\"\"
run sh -c \"sleep 300; : {marker}-grandchild\"
\"\"\", [\"{marker}-child\"])?
  assert output.success
}}

test sleeping_command {{ |ctx|
  run sh -c \"sleep 300; : {marker}-command\"
}}

test own_shorter_limit {{ |ctx|
  test.timeout(ctx, 200ms)
  time.sleep(30s)
}}

test own_longer_limit {{ |ctx|
  test.timeout(ctx, 30s)
  time.sleep(1500ms)
}}

test passes {{ |ctx|
  assert 1 == 1
}}
"
        ),
    )
    .expect("write timeout fixture");
    let started = std::time::Instant::now();
    let mut runner = Command::new(release_bin!("xsht"))
        .args(["test", "--jobs", "2", "--timeout", "1s"])
        .current_dir(root.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("start xsht test");
    let status = loop {
        if let Some(status) = runner.try_wait().expect("poll runner") {
            break status;
        }
        if started.elapsed() > std::time::Duration::from_secs(60) {
            let _ = runner.kill();
            panic!("hung tests did not time out");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    let mut stdout = String::new();
    std::io::Read::read_to_string(
        &mut runner.stdout.take().expect("runner stdout"),
        &mut stdout,
    )
    .expect("read runner stdout");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(30),
        "{stdout}"
    );
    assert_eq!(status.code(), Some(1), "{stdout}");
    for name in [
        "spins",
        "sleeping_script",
        "sleeping_command",
        "own_shorter_limit",
    ] {
        assert!(
            stdout.contains(&format!("tests/hang.xsh::{name} ... TIMEOUT")),
            "{name}: {stdout}"
        );
    }
    assert!(
        stdout.contains(
            "---- tests/hang.xsh::spins ----\nstdout:\nspins cleanup ran\nTIMEOUT after 1s\n"
        ),
        "{stdout}"
    );
    assert!(stdout.contains("TIMEOUT after 200ms"), "{stdout}");
    assert!(
        stdout.contains("tests/hang.xsh::own_longer_limit ... ok"),
        "{stdout}"
    );
    assert!(stdout.contains("tests/hang.xsh::passes ... ok"), "{stdout}");
    assert!(
        stdout.contains("test result: FAILED. 2 passed; 4 failed; 0 skipped"),
        "{stdout}"
    );
    assert_eq!(processes_with_marker(&marker), Vec::<String>::new());

    let invalid = Command::new(release_bin!("xsht"))
        .args(["test", "--timeout", "soon"])
        .current_dir(root.path())
        .output()
        .expect("run xsht test with an invalid timeout");
    assert_eq!(invalid.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("`--timeout` expects a duration"));
}

#[test]
fn native_test_declaration_discovery_preserves_names_and_runs_each_once() {
    let root = TempDir::new().expect("temporary native declaration fixture");
    fs::create_dir(root.path().join("tests")).expect("create test root");
    fs::write(
        root.path().join("tests/explicit.xsh"),
        "\
pure helper() -> Int { 2 }
proc ordinary_helper() { print helper() }
proc test_named_helper(value: Int) -> Int { value }
test test_old_name { assert test_named_helper(helper()) == 2 }
test no_prefix { |ctx| assert \"no_prefix\" in ctx.name }
test discarded { |_| }
",
    )
    .expect("write native declaration fixture");
    let list = Command::new(release_bin!("xsht"))
        .args(["test", "--list"])
        .current_dir(root.path())
        .output()
        .expect("list declarations");
    assert!(list.status.success());
    assert_eq!(
        String::from_utf8(list.stdout).unwrap(),
        "tests/explicit.xsh::discarded\ntests/explicit.xsh::no_prefix\ntests/explicit.xsh::test_old_name\n"
    );
    let run = Command::new(release_bin!("xsht"))
        .args(["test", "--jobs", "1"])
        .current_dir(root.path())
        .output()
        .expect("run declarations");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stdout)
    );
    assert!(String::from_utf8_lossy(&run.stdout).contains("3 passed; 0 failed"));
}

#[test]
fn native_test_declaration_legacy_proc_has_actionable_failure() {
    let root = TempDir::new().expect("temporary legacy fixture");
    fs::create_dir(root.path().join("tests")).expect("create test root");
    fs::write(
        root.path().join("tests/legacy.xsh"),
        "proc test_old(ctx: TestContext) -> Result[Unit] {}\n",
    )
    .expect("write legacy fixture");
    let run = Command::new(release_bin!("xsht"))
        .args(["test", "--jobs", "1"])
        .current_dir(root.path())
        .output()
        .expect("run legacy fixture");
    assert_eq!(run.status.code(), Some(1));
    let output = String::from_utf8_lossy(&run.stdout);
    assert!(output.contains("check.legacy-test-proc"), "{output}");
    assert!(output.contains("keep the exact declared name"), "{output}");
    assert!(!output.contains("0 passed; 0 failed"), "{output}");
    let filtered = Command::new(release_bin!("xsht"))
        .args(["test", "--exact", "tests/legacy.xsh::test_old"])
        .current_dir(root.path())
        .output()
        .expect("run exact legacy filter");
    assert_eq!(filtered.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&filtered.stdout).contains("check.legacy-test-proc"));
}

#[test]
fn native_test_declaration_import_registers_without_execution_or_discovery() {
    let root = TempDir::new().expect("temporary imported declaration fixture");
    fs::create_dir(root.path().join("tests")).expect("create test root");
    fs::write(root.path().join("tests/helper.xsh"), "##! Import registration fixture.\n## Returns the fixture value.\nexport pure value() -> Int { 7 }\ntest imported { assert false }\n")
        .expect("write imported module");
    fs::write(
        root.path().join("tests/entry.xsh"),
        "use helper\ntest entry { assert helper.value() == 7 }\n",
    )
    .expect("write entry fixture");
    let run = Command::new(release_bin!("xsht"))
        .args(["test", "--jobs", "1", "tests/entry.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run imported fixture");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stdout)
    );
    let output = String::from_utf8_lossy(&run.stdout);
    assert!(output.contains("1 passed; 0 failed"), "{output}");
    assert!(!output.contains("::imported"), "{output}");
}

