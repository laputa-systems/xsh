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

#[test]
fn final_top_level_int_sets_script_exit_status() {
    let output = run_temp_script(
        "int-exit-status",
        "\
proc main(value = 7) -> UInt {
  return value
}

main(@args)
",
    );

    assert_eq!(output.status.code(), Some(7));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn abort_sets_script_exit_status_and_runs_defers_by_default() {
    let output = run_temp_script(
        "abort-runs-defers",
        "\
defer run printf \"%s\\n\" top ?

proc main() -> Result[Unit] {
  defer run printf \"%s\\n\" proc ?
  abort(9)
  return Ok()
}

main()?
",
    );

    assert_eq!(output.status.code(), Some(9));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "proc\ntop\n");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn forced_abort_skips_defers() {
    let output = run_temp_script(
        "abort-force",
        "\
defer run printf \"%s\\n\" top ?

proc main() -> Result[Unit] {
  defer run printf \"%s\\n\" proc ?
  abort(11, force: true)
  return Ok()
}

main()?
",
    );

    assert_eq!(output.status.code(), Some(11));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn run_printf_preserves_one_data_argument() {
    let output = xsh(["tests/fixtures/runtime/run-printf.xsh"]);

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "hello world\n");
}

#[test]
fn plain_run_false_propagates_run_error_status() {
    let output = xsh(["tests/fixtures/runtime/run-false.xsh"]);

    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("nonzero-exit"));
}

#[test]
fn run_error_diagnostic_includes_cwd_and_argv() {
    let output = run_temp_script("run-failure-details", "run false \"two words\" ?\n");

    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("nonzero-exit"), "{stderr}");
    assert!(stderr.contains("cwd: "), "{stderr}");
    assert!(stderr.contains("argv: false 'two words'"), "{stderr}");
}

#[test]
fn run_status_false_returns_status_value() {
    let output = xsh(["tests/fixtures/runtime/run-status-false.xsh"]);

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "true\n");
}

#[test]
fn auto_main_returned_err_is_script_failure() {
    let output = run_temp_script(
        "auto-main-returned-err",
        r#"
error AppError = usage(message: Str)
proc main(...argv: List[Str]) [error] {
  let _ = argv
  return Err(AppError.usage(message: "bad args"))
}
"#,
    );

    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("usage"));
    assert!(stderr.contains("bad args"));
}

#[test]
fn signaled_status_exit_code_is_structured_error() {
    let output = xsh(["tests/fixtures/runtime/run-signaled-exit-code.xsh"]);

    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("status-kind"));
}

#[test]
fn missing_run_target_is_exec_failure_not_exit_127() {
    let output = xsh(["tests/fixtures/runtime/run-missing.xsh"]);

    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("not-found"));
    assert!(!stderr.contains("127"));
}

#[test]
fn run_capture_text_returns_str() {
    let output = xsh(["tests/fixtures/runtime/run-capture-text.xsh"]);

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "hello\n");
}

#[test]
fn run_capture_bytes_does_not_decode_utf8() {
    let output = xsh(["tests/fixtures/runtime/run-capture-bytes.xsh"]);

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "ok\n");
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
fn failed_left_pipeline_segment_is_diagnostic_and_trace_visible() {
    let plain = run_temp_script("pipeline-left-failure", "run false | run true ?\n");
    let late = run_temp_script("pipeline-late-failure", "run true | run false ?\n");
    let traced = run_temp_script_with_args(
        "pipeline-left-failure-trace",
        "run false | run true ?\n",
        ["--trace", "--raw"],
    );
    let json = run_temp_script_with_args(
        "pipeline-left-failure-json",
        "run false | run true ?\n",
        ["--trace", "--raw", "--trace-format", "jsonl"],
    );

    assert_eq!(plain.status.code(), Some(3));
    let stderr = String::from_utf8(plain.stderr).unwrap();
    assert!(stderr.contains("pipeline segment 0"));
    assert!(stderr.contains("false"));

    assert_eq!(late.status.code(), Some(3));
    assert!(
        String::from_utf8(late.stderr)
            .unwrap()
            .contains("pipeline segment 1")
    );

    assert_eq!(traced.status.code(), Some(3));
    let trace = String::from_utf8(traced.stderr).unwrap();
    assert!(trace.contains("kind=pipeline.enter"));
    assert!(trace.contains("kind=pipeline.segment.end"));
    assert!(trace.contains("index=0"));
    assert!(trace.contains("success:false"));

    assert_eq!(json.status.code(), Some(3));
    let json_trace = String::from_utf8(json.stderr).unwrap();
    assert!(json_trace.contains("\"kind\":\"pipeline.segment.end\""));
    assert!(json_trace.contains("\"index\":0"));
    assert!(json_trace.contains("\"success\":false"));
}

#[test]
fn redirection_setup_failure_is_traced() {
    let missing = temp_path("missing-redirection-input");
    let source = format!(
        "let missing = Path({})\nrun cat < (missing) ?\n",
        xsh_string_literal(missing.to_str().unwrap())
    );
    let output =
        run_temp_script_with_args("redirection-failure-trace", &source, ["--trace", "--raw"]);

    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("kind=redirection.setup"));
    assert!(stderr.contains("error={kind:b\"redirection\""));
}

#[test]
fn concise_language_sugar_runs_with_edge_cases() {
    let source = r#"
pure label(value: Str) -> Result[Str] {
  value
}

pure returned(value: Str) -> Result[Str] {
  return value
}

let root = ./target/runtime-sugar
root.remove(missing_ok: true)?
root.mkdir(parents: true)?
let file = fp"${root}/note.txt"
file.write("""alpha
beta
""")?
let content = file.read_text()?
let raw = r"\n ${literal}"
let nested = f"""${{name: "demo"}.name}:${if true { "x}" } else { "y" }}:${f"${1}"}"""
let escaped = f"\${not_interp}:${"ok"}:\x63\u{61}"
let names = fs.children(root)
|> map {
  .name
}
print ${label("ok")?} ${returned("return")?}
print ${content == """alpha
beta
"""} ${raw == r"\n ${literal}"} ${nested} ${escaped}
print ${names[0]}
"#;

    let output = run_temp_script("concise-language-sugar", source);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "ok return\ntrue true demo:x}:1 ${not_interp}:ok:ca\nnote.txt\n"
    );
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
fn xsht_trace_jsonl_includes_method_events_and_api_ids() {
    let output = run_temp_script_with_args(
        "method-trace-jsonl",
        "let demo_path = Path(\"demo.txt\")\nprint ${demo_path.display()}\n",
        ["--trace", "--raw", "--trace-format", "jsonl"],
    );

    assert!(output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("\"kind\":\"method.call\""), "{stderr}");
    assert!(stderr.contains("\"kind\":\"method.result\""), "{stderr}");
    assert!(
        stderr.contains("\"api_id\":\"method.Path.display\""),
        "{stderr}"
    );
    assert!(stderr.contains("\"api_id\":\"core.print\""), "{stderr}");
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

#[test]
fn trace_output_covers_baseline_event_kinds() {
    let success = run_temp_script_with_args(
        "trace-success",
        "\
let _term = process.signal(\"TERM\")?

pure decorate(value: Str) -> Str {
  return value
}

proc say(value: Str) -> Result[Unit] {
  let rendered = decorate(value)
  print ${rendered}
  return Ok()
}

proc main(args: List[Str]) -> Result[Unit] {
  say(\"traced\")?
  cd tests {
    run true ?
  } ?
  return Ok()
}

main(args)?
",
        ["--trace", "--raw"],
    );
    let propagated = xsht(["trace", "--raw", "examples/trace-error.xsh"]);
    let runtime_error = run_temp_script_with_args(
        "trace-runtime-error",
        "\
proc main(args: List[Str]) -> Result[Unit] {
  let values = [\"only\"]
  let missing = values[1]
  return Ok()
}

main(args)?
",
        ["--trace", "--raw"],
    );

    assert!(success.status.success());
    let success_trace = String::from_utf8(success.stderr).unwrap();
    for kind in [
        "kind=proc.enter",
        "kind=proc.exit",
        "kind=pure.enter",
        "kind=pure.exit",
        "kind=core.call",
        "kind=core.result",
        "kind=module.call",
        "kind=module.result",
        "kind=run.start",
        "kind=run.end",
        "kind=cwd.enter",
        "kind=cwd.exit",
    ] {
        assert!(success_trace.contains(kind), "{kind}: {success_trace}");
    }

    assert_eq!(propagated.status.code(), Some(3));
    assert!(
        String::from_utf8(propagated.stderr)
            .unwrap()
            .contains("kind=result.propagate")
    );

    assert_eq!(runtime_error.status.code(), Some(3));
    assert!(
        String::from_utf8(runtime_error.stderr)
            .unwrap()
            .contains("kind=runtime.error")
    );
}

#[test]
fn traceback_includes_nested_user_procs_and_pure_functions() {
    let output = run_temp_script(
        "nested-traceback",
        "\
pure leaf() -> Result[Unit] {
  let _ = Path.parse_bytes(b\"bad\\0path\")?
  return Ok()
}
pure middle() -> Result[Unit] {
  let _ = leaf()?
  return Ok()
}

proc outer() -> Result[Unit] {
  let _ = middle()?
  return Ok()
}

proc main(args: List[Str]) -> Result[Unit] {
  outer()?
  return Ok()
}

main(args)?
",
    );

    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("call path:"));
    assert!(stderr.contains("proc main"));
    assert!(stderr.contains("proc outer"));
    assert!(stderr.contains("pure middle"));
    assert!(stderr.contains("pure leaf"));
    assert!(stderr.contains("nul-path"));
}

#[test]
fn foundation_literals_defer_context_streams_timeout_and_builders_run() {
    let root = temp_path("foundation-root");
    let marker = temp_path("foundation-marker");
    let source = format!(
        r###"
let root = Path({})
let _made = fs.mkdir(root, parents: true)?
let marker = Path({})
defer fs.remove(root, missing_ok: true)?
defer fs.write(marker, "cleaned")?
let mode = 0o755
let label = f"mode ${{mode}}"
let raw_lines = run.stream --text printf "%s\n" alpha beta gamma
let lines = raw_lines
  |> drop(1)
  |> take(1)
let total = [1, 2, 3] |> sum()
let unique = [1, 1, 2, 3] |> unique-by {{ . }}
let command = process.command {{
  timeout = 2s
  run --timeout=1s echo ok
}}
print ${{mode == 493}} ${{"493" in label}} ${{lines[0]}} ${{total}} ${{unique[2]}}
"###,
        xsh_string_literal(root.to_str().unwrap()),
        xsh_string_literal(marker.to_str().unwrap())
    );

    let output = run_temp_script("foundations", &source);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "true true beta 6 3\n"
    );
    assert_eq!(std::fs::read_to_string(&marker).unwrap(), "cleaned");
    let _ = std::fs::remove_file(marker);
}

#[test]
fn hash_module_hashes_parses_and_verifies_files() {
    let file = temp_path("hash-input");
    let source = format!(
        r###"
let file = Path({})
let _ = fs.write(file, b"abc")?
defer fs.remove(file, missing_ok: true)?
let digest = hash.sha256(b"abc")
let file_digest = hash.sha256(file)?
hash.verify_file(file, sha256: digest.hex())?
let parsed = hash.parse_check_line(f"${{digest.hex()}} *input.bin")?
let mismatch = hash.verify_file(file, sha256: "0000000000000000000000000000000000000000000000000000000000000000")
match mismatch {{
  Err(e) => print ${{digest.hex()}} ${{digest.base64()}} ${{file_digest == digest}} ${{parsed.path}} ${{parsed.binary}} ${{e.kind}}
}}
"###,
        xsh_string_literal(file.to_str().unwrap())
    );

    let output = run_temp_script("hash-module", &source);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0= true input.bin true checksum-mismatch\n"
    );
}

#[test]
fn bytes_module_decodes_base64_base32_utf8_and_reports_compare_offsets() {
    let output = run_temp_script(
        "bytes-module",
        r#"
let encoded = b"\0hello\xff".base64()
let roundtrip = encoded.base64_decode()?
let spaced = "Y\nWJj".base64_decode()?
let unpadded = "Zm9v".base64_decode()?
let b32 = b"foobar".base32()
let b32_roundtrip = "mzxw6ytboi======".base32_decode()?
let b32_unpadded = "mzxw6ytboi".base32_decode()?
let decoded = b"alpha\nbeta".utf8()?
let lines = decoded |> text.lines()
let same = b"abc".compare(b"abc")
let comparison = b"abc\nxyz".compare(b"abc\nxqz")
let eof = b"abc".compare(b"abcd")
let invalid = b"\xff".utf8()
let invalid_base64 = "%%%".base64_decode()
let invalid_base32 = "M!".base32_decode()
match invalid {
  Err(e) => {
    match invalid_base64 {
      Err(b64) => {
        match invalid_base32 {
          Err(b32_err) => {
            print ${encoded} ${roundtrip == b"\0hello\xff"} ${spaced == b"abc"} ${unpadded == b"foo"}
            print ${b32} ${b32_roundtrip == b"foobar"} ${b32_unpadded == b"foobar"}
            print ${lines[1]} ${same.equal} ${comparison.equal} ${comparison.byte} ${comparison.line} ${comparison.left} ${comparison.right} ${eof.byte} ${eof.left} ${eof.right} ${e.kind} ${b64.kind} ${b32_err.kind}
          }
        }
      }
    }
  }
}
"#,
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "AGhlbGxv/w== true true true\nMZXW6YTBOI====== true true\nbeta true false 6 2 121 113 4 -1 100 invalid-utf8 invalid-base64 invalid-base32\n"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn bytes_module_slices_dumps_strings_and_copies_blocks() {
    let source = temp_path("bytes-copy-source");
    let dest = temp_path("bytes-copy-dest");
    let _ = std::fs::remove_file(&source);
    let _ = std::fs::remove_file(&dest);
    std::fs::write(&source, b"0123456789abcdef").expect("write byte copy source");
    let source = xsh_string_literal(&source.to_string_lossy());
    let dest = xsh_string_literal(&dest.to_string_lossy());
    let script = format!(
        r#"
let data = b"\0hello marker-one\0xx marker-two!!\xff"
let header = data.slice(offset: 1, length: 5)
let markers = data.strings(min_len: 7)
let hex = header.dump(format: "hex-u8")
let octal = header.dump("octal-u8")
let copied = bytes.copy(Path({source}), Path({dest}), block_size: 3, count: 2, skip: 1, seek: 0, overwrite: false)?
let copied_file = bytes.copy_file(Path({source}), Path({dest}), source_offset: 9, dest_offset: 2, length: 3, create: false, truncate: false)?
let out = Path({dest}).read_bytes()?
let exists = bytes.copy(Path({source}), Path({dest}), block_size: 3)
match exists {{
  Err(e) => {{
    print ${{data.len()}} ${{header == b"hello"}} ${{markers[0]}} ${{markers[1]}}
    print ${{hex}}
    print ${{octal}}
    print ${{copied.bytes}} ${{copied.blocks}} ${{copied_file.bytes}} ${{copied_file.blocks}} ${{out == b"349ab8"}} ${{e.kind}}
  }}
}}
"#
    );
    let output = run_temp_script("bytes-copy-dump", &script);
    let _ = std::fs::remove_file(temp_path("bytes-copy-source"));
    let _ = std::fs::remove_file(temp_path("bytes-copy-dest"));

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "34 true hello marker-one xx marker-two!!\n0000000 68 65 6c 6c 6f\n0000000 150 145 154 154 157\n6 2 3 1 true bytes-copy\n"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn run_timeout_returns_run_error() {
    let output = run_temp_script(
        "run-timeout",
        "let _ = run --timeout=10ms sh -c \"sleep 1\" ?\n",
    );

    assert_eq!(output.status.code(), Some(3));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("timeout")
    );
}
