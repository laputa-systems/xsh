#![allow(clippy::single_call_fn)]

use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn xsht_reports_non_utf8_argument_without_panicking() {
    let raw_path = std::ffi::OsString::from_vec(b"raw\xffpath.xsh".to_vec());
    let output = Command::new(test_bin!("xsht"))
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
fn test_list_returns_test_and_invalid_file_ids_in_sorted_order() {
    let root = TempDir::new().expect("temporary native test project");
    let tests = root.path().join("tests");
    fs::create_dir(&tests).expect("create native test root");
    fs::write(tests.join("z.xsh"), "test zeta { |_| }\n").expect("write z test");
    fs::write(tests.join("a.xsh"), "test alpha { |_| }\n").expect("write a test");
    fs::write(tests.join("m.xsh"), "test mu { |_| }\n").expect("write m test");
    fs::write(tests.join("bad.xsh"), "test broken { |ctx| let value: Int = \"wrong\" }\n")
        .expect("write invalid test");

    let output = Command::new(test_bin!("xsht"))
        .args(["test", "--list"])
        .current_dir(root.path())
        .output()
        .expect("list native tests");

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).expect("UTF-8 test list"),
        "tests/a.xsh::alpha\ntests/bad.xsh\ntests/m.xsh::mu\ntests/z.xsh::zeta\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn test_roots_share_imported_modules_during_parallel_discovery() {
    let root = TempDir::new().expect("temporary native test project");
    let tests = root.path().join("tests");
    fs::create_dir(&tests).expect("create native test root");
    fs::write(
        tests.join("shared.xsh"),
        "##! Shared native test value.\n## Return its fixed value.\nexport pure answer() -> Int { 42 }\n",
    )
    .expect("write shared module");
    fs::write(
        tests.join("a.xsh"),
        "use shared\ntest first_root { |_| assert shared.answer() == 42 }\n",
    )
    .expect("write first test root");
    fs::write(
        tests.join("b.xsh"),
        "use shared\ntest second_root { |_| assert shared.answer() == 42 }\n",
    )
    .expect("write second test root");

    let output = Command::new(test_bin!("xsht"))
        .args(["test", "--jobs", "4"])
        .current_dir(root.path())
        .output()
        .expect("run test roots that share an imported module");

    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(output.stderr.is_empty());
    assert!(String::from_utf8_lossy(&output.stdout).contains("2 passed; 0 failed; 0 skipped"));
}

#[test]
fn test_roots_resolve_shared_modules_in_their_nearest_config_context() {
    let root = TempDir::new().expect("temporary native test project");
    let left_tests = root.path().join("tests/left");
    let right_tests = root.path().join("tests/right");
    let common_modules = root.path().join("common");
    let left_modules = root.path().join("left_modules");
    let right_modules = root.path().join("right_modules");
    for directory in [
        &left_tests,
        &right_tests,
        &common_modules,
        &left_modules,
        &right_modules,
    ] {
        fs::create_dir_all(directory).expect("create native test fixture directory");
    }
    fs::write(
        root.path().join("xsht-config.ini"),
        "test_roots = tests\n",
    )
    .expect("write root test configuration");
    fs::write(
        left_tests.join("xsht-config.ini"),
        "module_path = ../../left_modules\n",
    )
    .expect("write left module configuration");
    fs::write(
        right_tests.join("xsht-config.ini"),
        "module_path = ../../right_modules\n",
    )
    .expect("write right module configuration");
    fs::write(
        common_modules.join("shared.xsh"),
        "##! Shared native test module.\n##! Resolve its dependency from the importing test's project.\nuse nested\n## Return the value from the context-specific dependency.\nexport pure answer() -> Int { nested.value() }\n",
    )
    .expect("write shared module");
    fs::write(
        left_modules.join("nested.xsh"),
        "##! Left native test dependency.\n## Return the left context's value.\nexport pure value() -> Int { 11 }\n",
    )
    .expect("write left nested dependency");
    fs::write(
        right_modules.join("nested.xsh"),
        "##! Right native test dependency.\n## Return the right context's value.\nexport pure value() -> Int { 22 }\n",
    )
    .expect("write right nested dependency");
    fs::write(
        left_tests.join("first.xsh"),
        "use shared\ntest left_context { |_| assert shared.answer() == 11 }\n",
    )
    .expect("write left test root");
    fs::write(
        right_tests.join("second.xsh"),
        "use shared\ntest right_context { |_| assert shared.answer() == 22 }\n",
    )
    .expect("write right test root");

    let output = Command::new(test_bin!("xsht"))
        .args(["test", "--jobs", "4"])
        .current_dir(root.path())
        .env("XSH_MODULE_PATH", &common_modules)
        .output()
        .expect("run test roots with different module search contexts");

    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(output.stderr.is_empty());
    assert!(String::from_utf8_lossy(&output.stdout).contains("2 passed; 0 failed; 0 skipped"));
}

#[test]
fn test_roots_lower_contextual_nominal_imports_in_the_selected_scope() {
    let root = TempDir::new().expect("temporary native test project");
    let left_tests = root.path().join("tests/left");
    let right_tests = root.path().join("tests/right");
    let common_modules = root.path().join("common");
    let left_modules = root.path().join("left_modules");
    let right_modules = root.path().join("right_modules");
    for directory in [
        &left_tests,
        &right_tests,
        &common_modules,
        &left_modules,
        &right_modules,
    ] {
        fs::create_dir_all(directory).expect("create native test fixture directory");
    }
    fs::write(
        root.path().join("xsht-config.ini"),
        "test_roots = tests\n",
    )
    .expect("write root test configuration");
    fs::write(
        left_tests.join("xsht-config.ini"),
        "module_path = ../../left_modules\n",
    )
    .expect("write left module configuration");
    fs::write(
        right_tests.join("xsht-config.ini"),
        "module_path = ../../right_modules\n",
    )
    .expect("write right module configuration");
    fs::write(
        common_modules.join("shared.xsh"),
        "##! Shared test helpers over a context-specific nominal module.\nuse nested as n\n## Build this context's tagged value.\nexport pure state() -> n.State { n.Right }\n## Match this context's tag.\nexport pure is_right(state: n.State) -> Bool { match state { n.Right => true; _ => false } }\n## Build this context's error family variant.\nexport pure failure() -> Result[Unit, n.ContextError] { Err(n.ContextError.Right(n.value())) }\n## Build its error value directly for a checked identity match.\nexport pure failure_value() -> n.ContextError { n.ContextError.Right(n.value()) }\n## Match this context's error family variant and payload.\nexport pure is_right_failure(failure: Result[Unit, n.ContextError]) -> Bool { match failure { Err(n.ContextError.Right { value }) => value == n.value(); _ => false } }\n",
    )
    .expect("write shared module");
    fs::write(
        left_modules.join("nested.xsh"),
        "##! Left context nominal types.\n## The state tags resolved in the left context.\nexport enum State { Left, Right }\n## The error family resolved in the left context.\nexport error ContextError = Left | Right(value: Int)\n## Supply the left error variant's declared payload.\nexport pure value() -> Int { 11 }\n",
    )
    .expect("write left nested dependency");
    fs::write(
        right_modules.join("nested.xsh"),
        "##! Right context nominal types.\n## The state tags resolved in the right context.\nexport enum State { Left, Right }\n## The error family resolved in the right context.\nexport error ContextError = Left | Right(value: Str)\n## Supply the right error variant's declared payload.\nexport pure value() -> Str { \"twenty-two\" }\n",
    )
    .expect("write right nested dependency");
    fs::write(
        common_modules.join("other.xsh"),
        "##! A separate error declaration with the same visible family name.\n## Keep its same-shaped variant distinct from nested.ContextError.\nexport error ContextError = Left | Right(value: Int)\n",
    )
    .expect("write distinct error family");
    fs::write(
        common_modules.join("other_right.xsh"),
        "##! A separate right-shaped error declaration with the same visible family name.\n## Keep its same-shaped variant distinct from nested.ContextError.\nexport error ContextError = Left | Right(value: Str)\n",
    )
    .expect("write distinct right error family");
    fs::write(
        left_tests.join("first.xsh"),
        "use shared\nuse nested as n\nuse other as o\ntest left_root_context { |_| let state: n.State = n.Right; assert state == n.Right; let failure: Result[Unit, n.ContextError] = Err(n.ContextError.Right(n.value())); assert match failure { Err(n.ContextError.Right { value }) => value == 11; _ => false }; let direct_error: n.ContextError = n.ContextError.Right(n.value()); let direct: Any = direct_error; assert match direct { o.ContextError.Right { .. } => false; _ => true } }\ntest left_transitive_context { |_| assert shared.is_right(shared.state()); assert shared.is_right_failure(shared.failure()); let transitive: Any = shared.failure_value(); assert match transitive { n.ContextError.Right { value } => value == 11; _ => false }; assert match transitive { o.ContextError.Right { .. } => false; _ => true } }\n",
    )
    .expect("write left test root");
    fs::write(
        right_tests.join("second.xsh"),
        "use shared\nuse nested as n\nuse other_right as o\ntest right_root_context { |_| let state: n.State = n.Right; assert state == n.Right; let failure: Result[Unit, n.ContextError] = Err(n.ContextError.Right(n.value())); assert match failure { Err(n.ContextError.Right { value }) => value == \"twenty-two\"; _ => false }; let direct_error: n.ContextError = n.ContextError.Right(n.value()); let direct: Any = direct_error; assert match direct { o.ContextError.Right { .. } => false; _ => true } }\ntest right_transitive_context { |_| assert shared.is_right(shared.state()); assert shared.is_right_failure(shared.failure()); let transitive: Any = shared.failure_value(); assert match transitive { n.ContextError.Right { value } => value == \"twenty-two\"; _ => false }; assert match transitive { o.ContextError.Right { .. } => false; _ => true } }\n",
    )
    .expect("write right test root");

    let output = Command::new(test_bin!("xsht"))
        .args(["test", "--jobs", "4"])
        .current_dir(root.path())
        .env("XSH_MODULE_PATH", &common_modules)
        .output()
        .expect("run nominal imports in distinct module contexts");

    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(output.stderr.is_empty());
    assert!(String::from_utf8_lossy(&output.stdout).contains("4 passed; 0 failed; 0 skipped"));
}

#[test]
fn test_help_describes_native_test_concurrency_bounds() {
    let output = Command::new(test_bin!("xsht"))
        .args(["test", "--help"])
        .output()
        .expect("show native test help");

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    assert!(String::from_utf8(output.stdout)
        .expect("UTF-8 test help")
        .contains("default: all CPUs capped at 32; max 8 per file"));
}

#[test]
fn test_runner_admits_another_file_while_one_file_is_at_its_active_limit() {
    let root = TempDir::new().expect("temporary native test project");
    let tests = root.path().join("tests");
    let markers = root.path().join("markers");
    fs::create_dir(&tests).expect("create native test root");
    fs::create_dir(&markers).expect("create marker root");

    let mut hot_tests = String::new();
    for index in 1..=12 {
        hot_tests.push_str(&format!(
            "test hot_{index:02} [fs, time, error] {{\n  p\"markers/hot-{index:02}\".write(\"\")?\n  var checks = 0\n  while checks < 500 and ! p\"markers/release\".exists()? {{\n    time.sleep(10ms)?\n    checks += 1\n  }}\n  assert p\"markers/release\".exists()?, \"the cold file must release hot tests\"\n}}\n"
        ));
    }
    fs::write(tests.join("hot.xsh"), hot_tests).expect("write hot test file");
    fs::write(
        tests.join("cold.xsh"),
        "test cold_file_progresses_while_hot_file_runs [fs, time, error] {\n  var checks = 0\n  while checks < 500 and ! p\"markers/hot-08\".exists()? {\n    time.sleep(10ms)?\n    checks += 1\n  }\n  assert p\"markers/hot-08\".exists()?, \"the hot file must reach its eighth active test\"\n  checks = 0\n  while checks < 200 and ! p\"markers/hot-09\".exists()? {\n    time.sleep(10ms)?\n    checks += 1\n  }\n  let exceeded_active_limit = p\"markers/hot-09\".exists()?\n  p\"markers/release\".write(\"\")?\n  assert ! exceeded_active_limit, \"the hot file must not exceed its active limit\"\n}\n",
    )
    .expect("write cold test file");

    let output = Command::new(test_bin!("xsht"))
        .args(["test", "--jobs", "16", "--timeout", "10s"])
        .current_dir(root.path())
        .output()
        .expect("run native tests with the file-level active limit");

    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(output.stderr.is_empty());
    assert!(String::from_utf8_lossy(&output.stdout)
        .contains("13 passed; 0 failed; 0 skipped"));
    assert!(markers.join("hot-12").exists());
}

#[test]
fn copied_xsht_formats_and_lints_script_backed_calls_in_static_and_loaded_modules() {
    let root = TempDir::new().expect("create isolated source root");
    let xsht = root.path().join("xsht");
    fs::copy(test_bin!("xsht"), &xsht).expect("copy xsht");
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
        let mut runner = Command::new(test_bin!("xsht"))
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
        let mut lint = Command::new(test_bin!("xsht"))
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
        let name = if signal == libc::SIGINT {
            "SIGINT"
        } else {
            "SIGTERM"
        };
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

/// One file is one unit of formatting work, so a signal that arrives while a
/// large file is being formatted is answered by the command and not by the
/// worker: `xsht fmt` leaves at once, and the file, which only the command
/// writes after every file is formatted, is untouched.
///
/// The signal goes to a spawned `xsht` for the reason given above.
#[test]
fn fmt_cancellation_during_one_large_file_is_prompt_and_writes_no_file() {
    // Every function needs formatting, and the file is large enough that
    // loading, checking, and formatting it takes seconds.
    let mut functions = String::new();
    for index in 0..100_000 {
        functions.push_str(&format!(
            "pure pick_{index}(n: Int) -> Int {{\n  let chosen = match n {{\n    1 => 10,\n    _ => 20,\n  }}\n  chosen   + 1\n}}\n\n"
        ));
    }
    functions.push_str("print ${pick_0(1)}\n");
    for signal in [libc::SIGTERM, libc::SIGINT] {
        let root = TempDir::new().expect("temporary format fixture");
        let file = root.path().join("large.xsh");
        fs::write(&file, &functions).expect("write format fixture");
        let mut format = Command::new(test_bin!("xsht"))
            .args(["fmt", "large.xsh"])
            .current_dir(root.path())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("start xsht fmt");
        // Long enough for the handlers to be installed, far shorter than the
        // run.
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(
            format.try_wait().expect("poll fmt").is_none(),
            "the format run ended before it could be interrupted; the fixture is too small"
        );
        assert_eq!(unsafe { libc::kill(format.id() as libc::pid_t, signal) }, 0);
        let started = std::time::Instant::now();
        let output = format.wait_with_output().expect("wait for fmt");
        assert_eq!(
            output.status.code(),
            Some(128 + signal),
            "{:?}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "");
        let name = if signal == libc::SIGINT {
            "SIGINT"
        } else {
            "SIGTERM"
        };
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(&format!("interrupted by {name}")),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        // The command looks at the signal many times a second while the
        // worker is inside the file; it does not wait for the file.
        assert!(
            started.elapsed() < std::time::Duration::from_secs(1),
            "{:?}",
            started.elapsed()
        );
        assert!(
            fs::read_to_string(&file).expect("read format fixture") == functions,
            "the interrupted run wrote the file"
        );
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
    let mut runner = Command::new(test_bin!("xsht"))
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

    let invalid = Command::new(test_bin!("xsht"))
        .args(["test", "--timeout", "soon"])
        .current_dir(root.path())
        .output()
        .expect("run xsht test with an invalid timeout");
    assert_eq!(invalid.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("`--timeout` expects a duration"));
}
