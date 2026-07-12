type RunRow = {name: Str}

error TailError = tail_error(message: Str)

pure show_run_row(row: RunRow, prefix: Str) -> Str {
  return f"${prefix} ${row.name}"
}

pure run_object_path(src: Path) -> Path {
  src.with_ext("o")
}

proc run_wrap_tail(value: Str) [error] -> Result[Str] {
  Ok(f"${value}.ok")
}

proc run_command_tail(value: Str) [error] -> Result[Str] {
  run_wrap_tail(value)
}

proc run_marker_tail() [error] -> Result[Str] {
  "proc-tail"
}

proc run_choose_tail(label: Str) [error] -> Result[Str] {
  let _ = label
  run_marker_tail()?
}

pure run_result_unit_tail_error() -> Result[Unit] {
  Err(TailError.tail_error(message: "bad"))
}

proc test_command_proc_args_resolve_bare_value_references() [error] {
  let rows = [{name: "alpha"}]
  let prefix = "item"
  test.eq(show_run_row(rows[0], prefix), "item alpha")?
  test.eq(show_run_row(rows[0], "prefix"), "prefix alpha")?
}

proc test_grouped_multiline_run_invocation_executes() [process, error] {
  test.eq(
    run.text (
      printf
      "%s %s\n"
      "grouped"
      "run"
    )?,
    """grouped run
""",
  )?
}

proc test_run_status_can_drive_conditions() [process, error] {
  var seen: List[Str] = []

  if ! run.status false {
    seen = seen.push("missing")
  }

  if run.status true {
    seen = seen.push("ok")
  }

  test.eq(seen, ["missing", "ok"])?
}

proc test_path_absolute_uses_current_runtime_cwd_without_existing_path() [fs, error] {
  let cwd = fs.cwd()?
  let p = path.absolute(p"target/../target/lang-absolute-demo")?
  test.eq(p, fp"${cwd}/target/lang-absolute-demo")?
}

proc test_boolean_operators_short_circuit() [error] {
  let items = [1]
  var seen: List[Str] = []

  if false and items[9] == 0 {
    seen = seen.push("bad-and")
  }

  if true or items[9] == 0 {
    seen = seen.push("ok-or")
  }

  if items.len() > 0 and items[0] == 1 {
    seen = seen.push("ok-and")
  }

  test.eq(seen, ["ok-or", "ok-and"])?
}

proc test_result_unit_statements_propagate_by_default() [time, error] {
  time.sleep(1ms)
}

proc test_script_stdout_can_emit_invalid_utf8_bytes() [error, io] {
  io.write_stdout_bytes(b"\xff\0a")?
}

proc test_run_capture_record_captures_status_stdout_and_stderr() [process, error] {
  let text_capture = run.capture --text sh -c "printf out; printf err >&2; exit 7" ?
  test.ok(text_capture.status.exited_with(7))?
  test.eq(text_capture.stdout, "out")?
  test.eq(text_capture.stderr, "err")?
  let byte_capture = run.capture --bytes sh -c "head -c 1 /dev/zero >&2; printf ok" ?
  test.eq(byte_capture.stdout.len(), 2)?
  test.eq(byte_capture.stderr.len(), 1)?
}

proc test_run_text_captures_stdout_and_inherits_stderr(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    r"""
let out = run.text sh -c "printf out; printf err >&2" ?
print ${out}
""",
  )?

  test.eq(
    output.stdout,
    """out
""",
  )?

  test.eq(output.stderr, "err")?
}

proc test_run_builtin_forms_execute_like_plain_run_forms() [process, error] {
  let status = run.builtin.status false
  test.ok(status.exited_with(1))?
  let text = run.builtin.text echo hello ?
  test.eq(text.trim(), "hello")?
  let capture = run.builtin.capture --text printf "out" ?
  test.ok(capture.status.ok)?
  test.eq(capture.stdout, "out")?
}

proc test_run_builtin_unknown_name_returns_process_error() [process, env, error] {
  env PATH="/bin:/usr/bin" {
    let missing = run.builtin.text command-not-builtin
    test.error_kind(missing, "not-found")?
  }
}

proc test_modules_are_not_command_namespaces(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    """use fs
fs read
""",
  )?

  test.eq(output.status, 2)?
  test.contains(output.stderr, "check.unresolved-proc-command")?
}

proc test_nul_run_targets_proc_splice_and_match_diagnostics(ctx: TestContext) [error] {
  let nul_target = test.run_script(
    ctx,
    """
proc main(args: List[Str]) -> Result[Unit] {
  run ("\0") ?
  return Ok()
}

main(args)?
""",
  )?

  test.eq(nul_target.status, 3)?
  test.contains(nul_target.stderr, "nul")?

  let nul_path = test.run_script(
    ctx,
    """let _ = Path("bad\\0path")
""",
  )?

  test.eq(nul_path.status, 3)?
  test.contains(nul_path.stderr, "nul")?

  let nul_argv = test.run_script(
    ctx,
    """run printf ("bad\\0arg") ?
""",
  )?

  test.eq(nul_argv.status, 3)?
  test.contains(nul_argv.stderr, "nul")?

  let spliced = test.run_script(
    ctx,
    r"""
proc pair(a: Str, b: Str) -> Result[Unit] {
  print ${a} ${b}
  return Ok()
}
let parts = ["left", "right"]
pair(@parts)?
""",
  )?

  test.ok(spliced.success, spliced.stderr)?

  test.eq(
    spliced.stdout,
    """left right
""",
  )?

  let no_arm = test.run_script(
    ctx,
    """let value = 1
match value {
  2 => print "two"
}
""",
  )?

  test.eq(no_arm.status, 3)?
  test.contains(no_arm.stderr, "match did not match any arm")?
}

proc test_legacy_test_and_getopt_spellings_are_not_command_aliases(ctx: TestContext) [error] {
  for source in [
    """test -f file
""",
    """[ -f file ]
""",
    """[[ name == value ]]
""",
    """getopt -- --root dest
""",
  ] {
    let output = test.run_script(ctx, source)?
    test.ok(! output.success, source)?

    test.ok(
      "check.unresolved-proc-command" in output.stderr or "check.unresolved-name" in output.stderr or "parse" in output.stderr or "lex" in output.stderr,
      output.stderr,
    )?
  }
}

proc test_function_tail_values_return_declared_values() [error] {
  let obj = run_object_path(p"main.c")

  let values = ["ok"]
    |> map { |value|
      run_command_tail(value)?
    }

  let marker_text = run_choose_tail("ignored")?
  test.eq(marker_text, "proc-tail")?
  test.eq(obj.name(), "main.o")?
  test.eq(values[0], "ok.ok")?
  test.error_kind(run_result_unit_tail_error(), "TailError.tail_error")?
}

proc test_byte_pipeline_executes_without_shell_and_redirects_stdout(ctx: TestContext) [fs, process, error] {
  let out = test.temp_path(ctx)
  run printf "%s\n" "hello" | run tr a-z A-Z > $out ?
  test.eq(out.read_bytes()?, b"HELLO\n")?
}

proc test_acceptance_tar_gzip_pipeline_writes_archive(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "tar-gzip")?
  let src = fp"${root}/src"
  let tarball = fp"${root}/archive.tar.gz"
  src.mkdir()?

  fp"${src}/file.txt".write("""contents
""")?

  cd root {
    run tar cf - src | run gzip -9 > $tarball ?
  } ?

  test.ok(tarball.metadata()?.size > 0)?
}

proc test_plain_run_updates_last_status_and_direct_binding() [process, error] {
  run.status false
  let last = $?
  test.ok(last.exited_with(1))?
  let bound = run sh -c "exit 7"
  test.ok(bound.segments[0].code == 7)?
  test.eq(bound.ok, false)?
}

proc test_redirection_paths_and_fd_duplication_use_typed_boundaries(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "redir")?
  let spaced = fp"${root}/space name"

  let lined = fp"""${root}/line
name"""

  let dashed = fp"${root}/-leading"
  let errlog = fp"${root}/err log"
  run printf "a" > $spaced ?
  run printf "b" >> $spaced ?
  run cat < $spaced > $lined ?
  run cat < $lined > $dashed ?
  run sh -c "printf err >&2" 2> $errlog ?
  run sh -c "printf more >&2" 2>> $errlog ?
  run true <& 0 ?
  test.eq(spaced.read_bytes()?, b"ab")?
  test.eq(lined.read_bytes()?, b"ab")?
  test.eq(dashed.read_bytes()?, b"ab")?
  test.eq(errlog.read_bytes()?, b"errmore")?
}

proc test_pipeline_status_preserves_exec_failure_and_broken_pipe_segments(ctx: TestContext) [fs, process, env, error] {
  env PATH="/bin:/usr/bin" {
    let status = run xsh-definitely-missing-command | run true
    test.eq(status.segments[0].kind, "exec")?
    test.eq(status.segments[0].error_kind, "not-found")?
  }

  let sink = test.temp_path(ctx)
  let broken = run yes | run head -n 1 > $sink
  test.ok(broken.segments[0].kind == "signal")?

  test.eq(
    sink.read_text()?,
    """y
""",
  )?
}

proc test_signaled_status_exposes_total_signal_helpers() [process, error] {
  let status = run sh -c "kill -TERM $$"
  test.ok(status.signaled())?
  test.ok(status.signal_number()? > 0)?
}

proc test_large_stdout_capture_drains_and_limit_is_error() [process, error] {
  let out = run.bytes head -c 131072 /dev/zero ?
  test.eq(out.len(), 131072)?
  let too_large = run.bytes head -c 16777217 /dev/zero
  test.error_kind(too_large, "capture-limit")?
}

proc test_invalid_utf8_text_capture_is_a_run_error() [process, error] {
  let invalid = run.text sh -c "printf '\\377'"
  test.error_kind(invalid, "invalid-utf8")?
}

proc test_whole_script_exit_status_and_abort_behavior(ctx: TestContext) [error] {
  let int_status = test.run_script(
    ctx,
    """
proc main(value = 7) -> UInt {
  return value
}

main(@args)
""",
  )?

  test.eq(int_status.status, 7)?
  test.eq(int_status.stdout, "")?
  test.eq(int_status.stderr, "")?

  let abort_with_defers = test.run_script(
    ctx,
    """
defer run printf "%s\\n" top ?

proc main() -> Result[Unit] {
  defer run printf "%s\\n" proc ?
  abort(9)
  return Ok()
}

main()?
""",
  )?

  test.eq(abort_with_defers.status, 9, abort_with_defers.stderr)?

  test.eq(
    abort_with_defers.stdout,
    """proc
top
""",
  )?

  test.eq(abort_with_defers.stderr, "")?

  let forced = test.run_script(
    ctx,
    """
defer run printf "%s\\n" top ?

proc main() -> Result[Unit] {
  defer run printf "%s\\n" proc ?
  abort(11, force: true)
  return Ok()
}

main()?
""",
  )?

  test.eq(forced.status, 11)?
  test.eq(forced.stdout, "")?
  test.eq(forced.stderr, "")?
}

proc test_whole_script_cli_usage_and_auto_main_errors(ctx: TestContext) [error] {
  let help = test.run_script(
    ctx,
    """
type Opts = {verbose: Bool, paths: List[Str]}

let opts: Opts = cli.parse(
  ARGV,
  {
    verbose: {form: "-v --verbose", default: false, help: "show extra output"},
    paths: {form: "...PATH", repeated: true},
  },
)?

print \${opts.paths.len()}
""",
    ["--help"],
    {},
    b"",
    "cli-help.xsh",
  )?

  test.ok(help.success, help.stderr)?
  test.eq(help.stderr, "")?
  test.contains(help.stdout, "usage: ")?
  test.contains(help.stdout, "cli-help")?
  test.not_contains(help.stdout, "usage: command ")?
  test.contains(help.stdout, "[...PATH] [OPTIONS]")?
  test.contains(help.stdout, "-v, --verbose")?
  test.contains(help.stdout, "-h, --help")?

  test.not_contains(
    help.stdout,
    """
0
""",
  )?

  let usage_error = test.run_script(
    ctx,
    """
type Opts = {path: Str}
let opts: Opts = cli.parse(ARGV, {path: {form: "PATH"}})?
print \${opts.path}
""",
  )?

  test.eq(usage_error.status, 2)?
  test.eq(usage_error.stdout, "")?
  test.contains(usage_error.stderr, "missing required argument PATH")?
  test.contains(usage_error.stderr, "usage:")?
  test.not_contains(usage_error.stderr, "traceback")?

  let auto_main = test.run_script(
    ctx,
    """
error AppError = usage(message: Str)
proc main(...argv: List[Str]) [error] {
  let _ = argv
  return Err(AppError.usage(message: "bad args"))
}
""",
  )?

  test.eq(auto_main.status, 3)?
  test.contains(auto_main.stderr, "usage")?
  test.contains(auto_main.stderr, "bad args")?
}

proc test_whole_script_run_error_diagnostics(ctx: TestContext) [error] {
  let details = test.run_script(
    ctx,
    """run false "two words" ?
""",
  )?

  test.eq(details.status, 3)?
  test.contains(details.stderr, "nonzero-exit")?
  test.contains(details.stderr, "cwd: ")?
  test.contains(details.stderr, "argv: false 'two words'")?

  let missing = test.run_script(
    ctx,
    """run xsh-definitely-missing-command ?
""",
    [],
    {PATH: "/bin:/usr/bin"},
  )?

  test.eq(missing.status, 3)?
  test.contains(missing.stderr, "not-found")?
  test.not_contains(missing.stderr, "127")?
}

proc test_pipeline_failures_and_trace_are_visible(ctx: TestContext) [error] {
  let plain = test.run_script(
    ctx,
    """run false | run true ?
""",
  )?

  test.eq(plain.status, 3)?
  test.contains(plain.stderr, "pipeline segment 0")?
  test.contains(plain.stderr, "false")?

  let late = test.run_script(
    ctx,
    """run true | run false ?
""",
  )?

  test.eq(late.status, 3)?
  test.contains(late.stderr, "pipeline segment 1")?

  let traced = test.run_xsht_trace(
    ctx,
    """run false | run true ?
""",
    ["--raw"],
  )?

  test.eq(traced.status, 3)?
  test.contains(traced.stderr, "kind=pipeline.enter")?
  test.contains(traced.stderr, "kind=pipeline.segment.end")?
  test.contains(traced.stderr, "index=0")?
  test.contains(traced.stderr, "success:false")?

  let json_trace = test.run_xsht_trace(
    ctx,
    """run false | run true ?
""",
    ["--raw", "--trace-format", "jsonl"],
  )?

  test.eq(json_trace.status, 3)?
  test.contains(json_trace.stderr, "\"kind\":\"pipeline.segment.end\"")?
  test.contains(json_trace.stderr, "\"index\":0")?
  test.contains(json_trace.stderr, "\"success\":false")?
}

proc test_run_trace_reports_redirection_method_and_env_details(ctx: TestContext) [error] {
  let redirection = test.run_xsht_trace(
    ctx,
    f"""
let missing = Path("{missing}")
run cat < (missing) ?
""",
    ["--trace", "--raw"],
  )?

  test.eq(redirection.status, 3)?
  test.contains(redirection.stderr, "kind=redirection.setup")?
  test.contains(redirection.stderr, "error={kind:b\"redirection\"")?

  let method_trace = test.run_xsht_trace(
    ctx,
    """let demo_path = Path("demo.txt")
print \${demo_path.display()}
""",
    ["--trace", "--raw", "--trace-format", "jsonl"],
  )?

  test.ok(method_trace.success, method_trace.stderr)?
  test.contains(method_trace.stderr, "\"kind\":\"method.call\"")?
  test.contains(method_trace.stderr, "\"kind\":\"method.result\"")?
  test.contains(method_trace.stderr, "\"api_id\":\"method.Path.display\"")?
  test.contains(method_trace.stderr, "\"api_id\":\"core.print\"")?

  let env_trace = test.run_xsht_trace(
    ctx,
    """run XSH_STAGE3_TRACE=value sh -c "true" ?
""",
    ["--raw"],
  )?

  test.ok(env_trace.success, env_trace.stderr)?
  test.contains(env_trace.stderr, "env={b\"XSH_STAGE3_TRACE\":b\"value\"}")?

  let cd_error = test.run_xsht_trace(
    ctx,
    """
let before = run.text pwd ?
cd tests {
  let xs = ["x"]
  let bad = xs[1]
} ?
""",
    ["--trace", "--raw"],
  )?

  test.eq(cd_error.status, 3)?
  test.contains(cd_error.stderr, "kind=cwd.enter")?
  test.contains(cd_error.stderr, "kind=cwd.exit")?
  test.contains(cd_error.stderr, "index-out-of-range")?
}

proc test_trace_output_covers_baseline_event_kinds(ctx: TestContext) [error] {
  let success = test.run_xsht_trace(
    ctx,
    """
let _term = process.signal("TERM")?

pure decorate(value: Str) -> Str {
  return value
}

proc say(value: Str) -> Result[Unit] {
  let rendered = decorate(value)
  print \${rendered}
  return Ok()
}

proc main(args: List[Str]) -> Result[Unit] {
  say("traced")?
  cd tests {
    run true ?
  } ?
  return Ok()
}

main(args)?
""",
    ["--trace", "--raw"],
  )?

  test.ok(success.success, success.stderr)?

  for kind in [
    "kind=script.enter",
    "kind=script.exit",
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
    test.contains(success.stderr, kind)?
  }

  let runtime_error = test.run_xsht_trace(
    ctx,
    """
proc main(args: List[Str]) -> Result[Unit] {
  let values = ["only"]
  let missing = values[1]
  return Ok()
}

main(args)?
""",
    ["--trace", "--raw"],
  )?

  test.eq(runtime_error.status, 3)?
  test.contains(runtime_error.stderr, "kind=runtime.error")?
  test.contains(runtime_error.stderr, "index-out-of-range")?
}

proc test_run_fixture_behaviors(ctx: TestContext) [process, error] {
  test.eq(
    run.text printf "%s\n" "hello world"?,
    """hello world
""",
  )?

  let failed = test.run_script(
    ctx,
    """run false
""",
  )?

  test.eq(failed.status, 3)?
  test.contains(failed.stderr, "nonzero-exit")?
  let status = run.status false
  test.ok(status.exited_with(1))?
  let text = run.text printf "%s" "hello" ?
  test.eq(text, "hello")?
  let raw = run.bytes head -c 1 /dev/zero ?
  test.eq(raw, b"\0")?
}

proc test_signaled_status_exit_code_is_structured_error(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    """let status = run sh -c "kill -TERM $$"
let _ = status.exit_code()?
""",
  )?

  test.eq(output.status, 3)?
  test.contains(output.stderr, "status-kind")?
}
