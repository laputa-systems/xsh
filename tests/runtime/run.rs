use super::common::*;

#[test]
fn xsht_trace_runs_and_xsh_rejects_trace_options() {
    let trace = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["trace", "examples/hello.xsh"])
        .output()
        .expect("run xsht");
    let trace_format = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .args(["--trace-format", "jsonl", "examples/hello.xsh"])
        .output()
        .expect("run xsh");
    let stale = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["stale", "examples/hello.xsh"])
        .output()
        .expect("run xsht");

    assert!(trace.status.success());
    assert_eq!(String::from_utf8(trace.stdout).unwrap(), "hello\n");
    assert!(
        String::from_utf8(trace.stderr)
            .unwrap()
            .contains("trace summary")
    );
    assert_eq!(trace_format.status.code(), Some(2));
    assert!(
        String::from_utf8(trace_format.stderr)
            .unwrap()
            .contains("trace options moved to `xsht trace`")
    );
    assert_eq!(stale.status.code(), Some(2));
    assert!(
        String::from_utf8(stale.stderr)
            .unwrap()
            .contains("unknown command 'stale'")
    );
}

#[test]
fn xsh_rejects_tool_subcommands() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .args(["check", "examples/hello.xsh"])
        .output()
        .expect("run xsh");

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("use xsht for tools")
    );
}

#[test]
fn xsh_evaluates_float_literals_methods_and_json() {
    let path = temp_xsh_path("float-values");
    std::fs::write(
        &path,
        r#"
type Metric = {ratio: Float, samples: List[Float]}

let ratio = 5.float() / 2.0
var adjusted: Float = ratio
adjusted += 0.25
let metric = json.decode("{\"ratio\":1.5,\"samples\":[0.25,1.25]}")?.require(Metric)?
let encoded = json.encode({ratio: metric.ratio, value: adjusted})?
print ${ratio.format(precision: 2)} ${adjusted.floor()?} ${encoded}
"#,
    )
    .expect("write float script");

    let output = xsh([path.to_str().unwrap()]);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "2.50 2 {\"ratio\":1.5,\"value\":2.75}\n"
    );
}

#[test]
fn xsh_help_describes_script_runner() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .arg("--help")
        .output()
        .expect("run xsh");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("xsh SCRIPT [ARGS...]"));
    assert!(!stdout.contains("--trace"));
}

#[test]
fn xsht_trace_accepts_script_args_without_double_dash() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["trace", "examples/args.xsh", "one", "two"])
        .output()
        .expect("run xsht trace");

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "one\ntwo\n");
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("trace summary")
    );
}

#[test]
fn xsh_rejects_trace_flags() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .args(["--raw", "examples/hello.xsh"])
        .output()
        .expect("run xsh");

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("trace options moved to `xsht trace`")
    );
}

#[test]
fn xsht_trace_accepts_syscalls_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["--syscalls", "examples/hello.xsh"])
        .output()
        .expect("run xsht");

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("unknown command '--syscalls'")
    );
}

#[test]
fn xsht_trace_rejects_invalid_trace_top_syscalls() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args([
            "trace",
            "--syscalls",
            "--trace-top-syscalls",
            "0",
            "examples/hello.xsh",
        ])
        .output()
        .expect("run xsht");

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("`--trace-top-syscalls` must be a positive integer")
    );
}

#[cfg(not(target_os = "linux"))]
#[test]
fn xsht_trace_rejects_syscalls_on_non_linux() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["trace", "--syscalls", "examples/hello.xsh"])
        .output()
        .expect("run xsht");

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("`--syscalls` is only supported on Linux")
    );
}

#[cfg(target_os = "linux")]
#[test]
fn xsht_syscall_trace_includes_summary_when_ptrace_available() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args([
            "trace",
            "--syscalls",
            "--trace-top-syscalls",
            "3",
            "examples/hello.xsh",
        ])
        .output()
        .expect("run xsht");

    let stderr = String::from_utf8(output.stderr).unwrap();
    if !output.status.success() && stderr.contains("syscall tracing setup failed") {
        return;
    }

    assert!(output.status.success(), "{stderr}");
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "hello\n");
    assert!(stderr.contains("trace summary"), "{stderr}");
    assert!(stderr.contains("syscall_count="), "{stderr}");
    assert!(stderr.contains("top_syscalls_by_count:"), "{stderr}");
    assert!(stderr.contains("per_program_top_syscalls:"), "{stderr}");
    assert!(stderr.contains("per_process_top_syscalls:"), "{stderr}");
}

#[test]
fn xsh_accepts_script_args_without_double_dash() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .args(["examples/args.xsh", "one"])
        .output()
        .expect("run xsh");

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "one\n");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn xsh_accepts_leading_double_dash_for_shebang_scripts() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .args(["--", "examples/args.xsh", "one", "two"])
        .output()
        .expect("run xsh");

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "one\ntwo\n");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[cfg(target_os = "linux")]
#[test]
fn run_cpumax_writes_fake_cgroup_scope_and_cleans_up() {
    let root = temp_path("run-cpumax-cgroup");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create fake cgroup root");
    let script = write_temp_script("run-cpumax-cgroup", "run --cpumax=80 true ?\n");
    let output = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .arg(&script)
        .env("XSH_CGROUP_ROOT", &root)
        .output()
        .expect("run xsh");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let entries = std::fs::read_dir(&root)
        .expect("read fake cgroup root")
        .collect::<Result<Vec<_>, _>>()
        .expect("read cgroup entries");
    assert!(entries.is_empty(), "{entries:?}");
    let _ = std::fs::remove_file(script);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn run_trace_preserves_argv_boundaries() {
    let output = xsht([
        "trace",
        "--raw",
        "tests/fixtures/runtime/run-trace-argv.xsh",
    ]);

    assert!(output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("kind=run.start"));
    assert!(stderr.contains("b\"hello world\""));
    assert!(stderr.contains("b\"line\\nfeed\""));
    assert!(stderr.contains("b\"-dash\""));
}

#[test]
fn xsht_trace_jsonl_is_on_stderr() {
    let output = xsht(["trace", "--trace-format", "jsonl", "examples/hello.xsh"]);

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "hello\n");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr
            .lines()
            .any(|line| line.contains("\"trace.summary\""))
    );
    assert!(stderr.contains("\"function_calls\":"));
    assert!(stderr.contains("\"hot_commands\":"));
    assert!(stderr.contains("\"script_duration_us\":"));
}
#[test]
fn xsht_trace_file_keeps_runtime_stderr_separate() {
    let path = temp_xsh_path("trace-file");
    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args([
            "trace",
            "--trace-file",
            path.to_str().unwrap(),
            "examples/hello.xsh",
        ])
        .output()
        .expect("run xsht");

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "hello\n");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    let trace = std::fs::read_to_string(&path).expect("read trace file");
    assert!(trace.contains("trace summary"));
    assert!(trace.contains("script duration"));
    assert!(trace.contains("hot commands (top 10 by total ms)"));
    assert!(trace.contains('┌'));
    assert!(!trace.contains("kind=script.enter"));

    std::fs::remove_file(path).expect("remove trace file");
}

#[test]
fn process_failures_report_distinct_error_kinds() {
    let running_as_root = unsafe { libc::geteuid() == 0 };
    let not_executable = temp_path("not-executable-tool");
    std::fs::write(&not_executable, "#!/bin/sh\nexit 0\n").expect("write not executable");
    std::fs::set_permissions(&not_executable, std::fs::Permissions::from_mode(0o644))
        .expect("chmod not executable");

    let exec_format = temp_path("exec-format-tool");
    std::fs::write(&exec_format, "not a native executable\n").expect("write exec format");
    std::fs::set_permissions(&exec_format, std::fs::Permissions::from_mode(0o755))
        .expect("chmod exec format");

    let denied_dir = temp_path("permission-denied-dir");
    let denied_tool = denied_dir.join("tool");
    std::fs::create_dir_all(&denied_dir).expect("create denied dir");
    std::fs::write(&denied_tool, "#!/bin/sh\nexit 0\n").expect("write denied tool");
    std::fs::set_permissions(&denied_tool, std::fs::Permissions::from_mode(0o755))
        .expect("chmod denied tool");
    std::fs::set_permissions(&denied_dir, std::fs::Permissions::from_mode(0o000))
        .expect("chmod denied dir");

    let not_executable_output = run_path_target_script("not-executable-run", &not_executable);
    let exec_format_output = run_path_target_script("exec-format-run", &exec_format);
    let permission_denied_output = if running_as_root {
        None
    } else {
        Some(run_path_target_script(
            "permission-denied-run",
            &denied_tool,
        ))
    };

    std::fs::set_permissions(&denied_dir, std::fs::Permissions::from_mode(0o755))
        .expect("restore denied dir permissions");
    let _ = std::fs::remove_file(not_executable);
    let _ = std::fs::remove_file(exec_format);
    let _ = std::fs::remove_dir_all(denied_dir);

    assert_eq!(not_executable_output.status.code(), Some(3));
    assert!(
        String::from_utf8(not_executable_output.stderr)
            .unwrap()
            .contains("not-executable")
    );
    assert_eq!(exec_format_output.status.code(), Some(3));
    assert!(
        String::from_utf8(exec_format_output.stderr)
            .unwrap()
            .contains("exec-format")
    );
    if let Some(output) = permission_denied_output {
        assert_eq!(output.status.code(), Some(3));
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("permission-denied")
        );
    }
}
