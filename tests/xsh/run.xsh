type RunRow = {name: Str}

pure show_run_row(row: RunRow, prefix: Str) -> Str {
  f"${prefix} ${row.name}"
}

test test_command_proc_args_resolve_bare_value_references {
  let rows = [{name: "alpha"}]
  let prefix = "item"
  assert show_run_row(rows[0], prefix) == "item alpha"
  assert show_run_row(rows[0], "prefix") == "prefix alpha"
}

test test_mutable_string_accumulator_uses_string_addition_in_loop { |ctx|
  let output = test.run_script(
    ctx,
    """proc main() {
  var stack = ""
  for segment in ["a", "b", "c"] {
    stack = stack + segment
  }
  print $stack
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """abc
"""
  assert output.stderr == ""
}

test test_reassigning_let_names_mutable_binding { |ctx|
  let output = test.run_script(
    ctx,
    """let x = 1
x = 2
""",
  )?
  assert output.status == 2
  assert "check.assign-let" in output.stderr
  assert "declare with `var`" in output.stderr
}

test test_checker_errors_prevent_execution { |ctx|
  let output = test.run_script(
    ctx,
    """print "before"
let value = "abc"
print $value.length()
""",
  )?
  assert output.status == 2
  assert output.stdout == ""
  assert "check.unknown-method" in output.stderr
}

test test_runtime_unknown_method_names_receiver_and_candidate { |ctx|
  let output = test.run_script(
    ctx,
    """let value: Any = "abc"
print $value.length()
""",
  )?
  assert output.status == 3
  assert "unknown method `length` on Str" in output.stderr
  assert "count_chars" in output.stderr
}

test test_grouped_multiline_run_invocation_executes {
  assert (run.text (
    printf
    "%s %s\n"
    "grouped"
    "run"
  )?) == """grouped run
"""
}

test test_run_status_can_drive_conditions {
  var seen = []

  if ! (run.status false) {
    seen += ["missing"]
  }

  if run.status true {
    seen += ["ok"]
  }

  assert seen == ["missing", "ok"]
}

test test_path_absolute_uses_current_runtime_cwd_without_existing_path {
  let cwd = fs.cwd()?
  let p = path.absolute(p"target/../target/lang-absolute-demo")?
  assert p == fp"${cwd}/target/lang-absolute-demo"
}

test test_boolean_operators_short_circuit {
  let items = [1]
  var seen = []

  if false and items[9] == 0 {
    seen += ["bad-and"]
  }

  if true or items[9] == 0 {
    seen += ["ok-or"]
  }

  if items.len() > 0 and items[0] == 1 {
    seen += ["ok-and"]
  }

  assert seen == ["ok-or", "ok-and"]
}

test test_result_unit_statements_propagate_by_default {
  time.sleep(1ms)
}

test test_script_stdout_can_emit_invalid_utf8_bytes {
  io.write_stdout_bytes(b"\xff\0a")?
}

test test_run_capture_record_captures_status_stdout_and_stderr {
  let text_capture = run.capture --text sh -c "printf out; printf err >&2; exit 7" ?
  assert text_capture.status.exited_with(7)
  assert text_capture.stdout == "out"
  assert text_capture.stderr == "err"
  let byte_capture = run.capture --bytes sh -c "head -c 1 /dev/zero >&2; printf ok" ?
  assert byte_capture.stdout.len() == 2
  assert byte_capture.stderr.len() == 1
}

test test_run_text_captures_stdout_and_inherits_stderr { |ctx|
  let output = test.run_script(
    ctx,
    r"""
let out = run.text sh -c "printf out; printf err >&2" ?
print ${out}
""",
  )?

  assert output.stdout == """out
"""

  assert output.stderr == "err"
}

test test_run_forms_preserve_status_text_and_capture {
  let status = run.status false
  assert status.exited_with(1)
  let text = run.text echo hello ?
  assert text.trim() == "hello"
  let capture = run.capture --text printf "out" ?
  assert capture.status.ok
  assert capture.stdout == "out"
}

test test_dynamic_module_proc_preserves_bareword_run_arguments { |ctx|
  let root = test.temp_dir(ctx, name: "module-run-arguments")?
  let module_path = fp"${root}/package.xsh"
  module_path.write(r"""
##! Bareword run argument fixture.
## Runs representative bareword forms from a loaded module.
export proc build() [process, error] {
  run echo -a json ?
  run echo -ab json ?
  run echo -- json ?
  run echo --f json ?
  run echo "-C" "build" samu ?
  run echo --- json ?
  run echo --format json ?
  run echo apples ?
}
""")?

  let output = test.run_script(
    ctx,
    f"""let build_fn: Proc = module.load(p"${module_path.display()}")?.get("build")?
build_fn.call()?
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  for expected in [
    "-a json",
    "-ab json",
    "-- json",
    "--f json",
    "-C build samu",
    "--- json",
    "--format json",
    "apples",
  ] {
    assert expected in output.stdout
  }
}

test test_run_unknown_name_returns_process_error {
  env PATH="/bin:/usr/bin" {
    let missing = run.text command-not-builtin
    test.error_kind(missing, "not-found")?
  }
}

test test_modules_are_not_command_namespaces { |ctx|
  let output = test.run_script(
    ctx,
    """use fs
fs read
""",
  )?

  assert output.status == 2
  assert "check.unresolved-proc-command" in output.stderr
}

test test_call_splices_preserve_shared_and_constant_lists { |ctx|
  let result = test.run_script(
    ctx,
    r"""
const prepared = ["constant", "backing"]
proc pair(a: Str, b: Str) -> Result[Unit] {
  print ${a} ${b}
  Ok()
}
let parts = ["left", "right"]
pair(@parts)?
pair(@prepared)?
pair(@parts)?
pair(@prepared)?
var order = 0
pair(@{ order = order * 10 + 1; ["first"] }, @{ order = order * 10 + 2; ["second"] })?
print $order
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = result
    assert assertion_condition, assertion_message
  }
  assert result.stdout == """left right
constant backing
left right
constant backing
first second
12
"""
}

test test_nul_run_targets_proc_splice_and_match_diagnostics { |ctx|
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

  assert nul_target.status == 3
  assert "nul" in nul_target.stderr

  let nul_path = test.run_script(
    ctx,
    """let _ = Path("bad\\0path")
""",
  )?

  assert nul_path.status == 3
  assert "nul" in nul_path.stderr

  let nul_argv = test.run_script(
    ctx,
    """run printf ("bad\\0arg") ?
""",
  )?

  assert nul_argv.status == 3
  assert "nul" in nul_argv.stderr

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

  {
    let {success: assertion_condition, stderr: assertion_message, ..} = spliced
    assert assertion_condition, assertion_message
  }

  assert spliced.stdout == """left right
"""

  let no_arm = test.run_script(
    ctx,
    """let value = 1
match value {
  2 => print "two"
}
""",
  )?

  assert no_arm.status == 3
  assert "match did not match any arm" in no_arm.stderr
}

test test_legacy_test_and_getopt_spellings_are_not_command_aliases { |ctx|
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
    {
      let assertion_condition = ! output.success
      let assertion_message = source
      assert assertion_condition, assertion_message
    }

    {
      let assertion_condition = "check.unresolved-proc-command" in output.stderr or "check.unresolved-name" in output.stderr or "parse" in output.stderr or "lex" in output.stderr
      let assertion_message = output.stderr
      assert assertion_condition, assertion_message
    }
  }
}

test test_function_tail_values_return_declared_values { |ctx|
  let output = test.run_script(
    ctx,
    r"""pure run_object_path(src: Path) -> Path { src.with_ext("o") }
proc run_wrap_tail(value: Str) [error] -> Result[Str] { Ok(f"${value}.ok") }
proc run_command_tail(value: Str) [error] -> Result[Str] {
  run_wrap_tail(value)
}

proc run_marker_tail() [error] -> Result[Str] { "proc-tail" }
proc run_choose_tail(label: Str) [error] -> Result[Str] { let _ = label; run_marker_tail()? }
error TailError = tail_error(message: Str)
pure run_result_unit_tail_error() -> Result[Unit] { Err(TailError.tail_error(message: "bad")) }
proc witness() [error] {
  let obj = run_object_path(p"main.c")

  let values = ["ok"]
    |> map { |value|
      run_command_tail(value)?
    }

  let marker_text = run_choose_tail("ignored")?
  assert marker_text == "proc-tail"
  assert obj.name() == "main.o"
  assert values[0] == "ok.ok"
  test.error_kind(run_result_unit_tail_error(), "TailError.tail_error")?
}
witness()
""",
  )?
  let {success: assertion_condition, stderr: assertion_message, ..} = output
  assert assertion_condition, assertion_message
  assert output.stdout == ""
}

test test_byte_pipeline_executes_without_shell_and_redirects_stdout { |ctx|
  let out = test.temp_path(ctx)
  run printf "%s\n" "hello" | run tr a-z A-Z > $out ?
  assert out.read_bytes()? == b"HELLO\n"
}

test test_acceptance_tar_gzip_pipeline_writes_archive { |ctx|
  let root = test.temp_dir(ctx, name: "tar-gzip")?
  let src = fp"${root}/src"
  let tarball = fp"${root}/archive.tar.gz"
  src.mkdir()?

  fp"${src}/file.txt".write("""contents
""")?

  cd root {
    run tar cf - src | run gzip -9 > $tarball ?
  } ?

  assert tarball.metadata()?.size > 0
}

test test_plain_run_updates_last_status_and_direct_binding {
  run.status false
  let last = $?
  assert last.exited_with(1)
  let bound = run sh -c "exit 7"
  assert bound.segments[0].code == 7
  assert bound.ok == false
}

test test_redirection_paths_and_fd_duplication_use_typed_boundaries { |ctx|
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
  assert spaced.read_bytes()? == b"ab"
  assert lined.read_bytes()? == b"ab"
  assert dashed.read_bytes()? == b"ab"
  assert errlog.read_bytes()? == b"errmore"
}

test test_pipeline_status_preserves_exec_failure_and_broken_pipe_segments { |ctx|
  env PATH="/bin:/usr/bin" {
    let status = run xsh-definitely-missing-command | run true
    assert status.segments[0].kind == "exec"
    assert status.segments[0].error_kind == "not-found"
  }

  let sink = test.temp_path(ctx)
  let broken = run yes | run head -n 1 > $sink
  assert broken.segments[0].kind == "signal"

  assert sink.read_text()? == """y
"""
}

test test_signaled_status_exposes_total_signal_helpers {
  let status = run sh -c "kill -TERM $$"
  assert status.signaled()
  assert status.signal_number()? > 0
}

test test_large_stdout_capture_drains_and_limit_is_error {
  let out = run.bytes head -c 131072 /dev/zero ?
  assert out.len() == 131072
  let too_large = run.bytes head -c 16777217 /dev/zero
  test.error_kind(too_large, "capture-limit")?
}

test test_invalid_utf8_text_capture_is_a_run_error {
  let invalid = run.text sh -c "printf '\\377'"
  test.error_kind(invalid, "invalid-utf8")?
}

test test_whole_script_exit_status_and_abort_behavior { |ctx|
  let int_status = test.run_script(
    ctx,
    """
proc main(value = 7) -> UInt {
  return value
}

main(@args)
""",
  )?

  assert int_status.status == 7
  assert int_status.stdout == ""
  assert int_status.stderr == ""

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

  {
    let assertion_actual = abort_with_defers.status
    let assertion_expected = 9
    let assertion_message = abort_with_defers.stderr
    assert assertion_actual == assertion_expected, assertion_message
  }

  assert abort_with_defers.stdout == """proc
top
"""

  assert abort_with_defers.stderr == ""

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

  assert forced.status == 11
  assert forced.stdout == ""
  assert forced.stderr == ""

  let quiet_validation_failure = test.run_script(
    ctx,
    """
print "escape"
abort(17)
print "unreachable"
""",
  )?

  assert quiet_validation_failure.status == 17
  assert quiet_validation_failure.stdout == """escape
"""
  assert quiet_validation_failure.stderr == ""
}

test test_whole_script_cli_usage_and_auto_main_errors { |ctx|
  let help = test.run_script(
    ctx,
    """
type Opts = {verbose: Bool, paths: List[Str]}

let opts: Opts = cli.parse(
  args,
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

  {
    let {success: assertion_condition, stderr: assertion_message, ..} = help
    assert assertion_condition, assertion_message
  }
  assert help.stderr == ""
  assert "usage: " in help.stdout
  assert "cli-help" in help.stdout
  assert "usage: command " not in help.stdout
  assert "[...PATH] [OPTIONS]" in help.stdout
  assert "-v, --verbose" in help.stdout
  assert "-h, --help" in help.stdout

  assert """
0
""" not in help.stdout

  let usage_error = test.run_script(
    ctx,
    """
type Opts = {path: Str}
let opts: Opts = cli.parse(args, {path: {form: "PATH"}})?
print \${opts.path}
""",
  )?

  assert usage_error.status == 2
  assert usage_error.stdout == ""
  assert "missing required argument PATH" in usage_error.stderr
  assert "usage:" in usage_error.stderr
  assert "traceback" not in usage_error.stderr

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

  assert auto_main.status == 3
  assert "usage" in auto_main.stderr
  assert "bad args" in auto_main.stderr
}

test test_explicit_zero_arg_main_runs_once { |ctx|
  let output = test.run_script(
    ctx,
    """
proc main() [error] -> Result[Unit] { print 5 }
main()?
""",
  )?

  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """5
"""
  assert output.stderr == ""
}

test test_whole_script_run_error_diagnostics { |ctx|
  let details = test.run_script(
    ctx,
    """run false "two words" ?
""",
  )?

  assert details.status == 3
  assert "nonzero-exit" in details.stderr
  assert "cwd: " in details.stderr
  assert "argv: false 'two words'" in details.stderr

  let missing = test.run_script(
    ctx,
    """run xsh-definitely-missing-command ?
""",
    [],
    {PATH: "/bin:/usr/bin"},
  )?

  assert missing.status == 3
  assert "not-found" in missing.stderr
  assert "exited with status" not in missing.stderr
}

test test_pipeline_failures_and_trace_are_visible { |ctx|
  let plain = test.run_script(
    ctx,
    """run false | run true ?
""",
  )?

  assert plain.status == 3
  assert "pipeline segment 0" in plain.stderr
  assert "false" in plain.stderr

  let late = test.run_script(
    ctx,
    """run true | run false ?
""",
  )?

  assert late.status == 3
  assert "pipeline segment 1" in late.stderr

  let traced = test.run_xsht_trace(
    ctx,
    """run false | run true ?
""",
    ["--raw"],
  )?

  assert traced.status == 3
  assert "kind=pipeline.enter" in traced.stderr
  assert "kind=pipeline.segment.end" in traced.stderr
  assert "index=0" in traced.stderr
  assert "success:false" in traced.stderr

  let json_trace = test.run_xsht_trace(
    ctx,
    """run false | run true ?
""",
    ["--raw", "--trace-format", "jsonl"],
  )?

  assert json_trace.status == 3
  assert "\"kind\":\"pipeline.segment.end\"" in json_trace.stderr
  assert "\"index\":0" in json_trace.stderr
  assert "\"success\":false" in json_trace.stderr
}

test test_statement_pipeline_fails_on_any_failed_segment { |ctx|
  let late = test.run_script(
    ctx,
    """run true | run false
print "after"
""",
  )?
  assert late.status == 3
  assert "pipeline segment 1 `false` exited with status 1" in late.stderr
  assert "after" not in late.stdout

  let signaled = test.run_script(
    ctx,
    """run sh -c "kill -TERM $$" | run true
""",
  )?
  assert signaled.status == 3
  assert "pipeline segment 0 `sh` was terminated by signal" in signaled.stderr

  let missing = test.run_script(
    ctx,
    """run true | run xsh-definitely-missing-command
print "after"
""",
  )?
  assert missing.status == 3
  assert "pipeline segment 1" in missing.stderr
  assert "after" not in missing.stdout

  let marker = test.temp_path(ctx)
  let drained = test.run_script(
    ctx,
    f"""run sh -c "exit 3" | run sh -c "cat >/dev/null; echo ran > ${marker}"
""",
  )?
  assert drained.status == 3
  assert "pipeline segment 0 `sh` exited with status 3" in drained.stderr
  assert marker.read_text()? == "ran\n"
}

test test_pipeline_sigpipe_after_successful_downstream_exit_is_not_failure { |ctx|
  let sink = test.temp_path(ctx)
  run yes | run head -n 1 | run cat > $sink
  assert sink.read_text()? == "y\n"

  let status = run yes | run head -n 1 > $sink
  assert status.success
  assert status.segments[0].kind == "signal"

  let downstream_failed = test.run_script(
    ctx,
    """run yes | run sh -c "head -n 1 >/dev/null; exit 4"
""",
  )?
  assert downstream_failed.status == 3
  assert "pipeline segment 1 `sh` exited with status 4" in downstream_failed.stderr
}

test test_pipeline_value_position_and_accept_rules { |ctx|
  let status = run true | run false
  assert !status.success
  assert status.segments[1].code == 1

  run.status false | run false
  run true | run --accept=[0, 1] false

  let rejected = test.run_script(
    ctx,
    """run --accept=[0] true | run false
print "after"
""",
  )?
  assert rejected.status == 3
  assert "pipeline segment 1" in rejected.stderr
  assert "after" not in rejected.stdout
}

test test_pipeline_capture_takes_last_stdout_and_fails_on_any_segment {
  let upper = run.text printf "%s\n" "hello" | run tr a-z A-Z ?
  assert upper == "HELLO\n"
  let raw = run.bytes printf "a\\377" | run cat | run cat ?
  assert raw == b"a\xff"
  let head = run.text yes | run head -n 2 ?
  assert head == "y\ny\n"

  let late = run.text true | run false
  test.error_kind(late, "pipeline-failure")?
  if let Err(error) = late {
    assert "pipeline segment 1 `false` exited with status 1" in error.message, error.message
  }

  let early = run.bytes sh -c "echo lost; exit 2" | run cat
  if let Err(error) = early {
    assert "pipeline segment 0 `sh` exited with status 2" in error.message, error.message
  } else {
    test.fail("expected a pipeline failure")?
  }

  let invalid = run.text printf "\\377" | run cat
  test.error_kind(invalid, "invalid-utf8")?
  let too_large = run.bytes head -c 16777217 /dev/zero | run cat
  test.error_kind(too_large, "capture-limit")?
  let accepted = run.text true | run --accept=[0, 1] sh -c "echo kept; exit 1" ?
  assert accepted == "kept\n"
}

test test_pipeline_capture_record_merges_stderr_and_reports_status {
  let record = run.capture --text sh -c "echo out0; echo err0 >&2" | run sh -c "cat; echo err1 >&2; exit 3" ?
  assert record.stdout == "out0\n"
  assert "err0\n" in record.stderr
  assert "err1\n" in record.stderr
  assert !record.status.success
  assert record.status.segments[1].code == 3

  let raw_record = run.capture --bytes printf "x" | run cat ?
  assert raw_record.stdout == b"x"
  assert raw_record.stderr == b""
  assert raw_record.status.success
}

test test_pipeline_capture_form_is_chosen_by_the_head_segment { |ctx|
  let cases = [
    "let x = run.stream --text printf a | run cat",
    "let x = run printf a | run.text cat",
    "let x = run.text printf a | run.status cat",
  ]
  for source in cases {
    let output = test.run_script(ctx, source)?
    assert output.status == 2
    assert "check.pipeline-capture" in output.stderr
  }
}

test test_run_trace_reports_redirection_method_and_env_details { |ctx|
  let redirection = test.run_xsht_trace(
    ctx,
    f"""
let missing = Path("{missing}")
run cat < (missing) ?
""",
    ["--trace", "--raw"],
  )?

  assert redirection.status == 3
  assert "kind=redirection.setup" in redirection.stderr
  assert "error={kind:b\"redirection\"" in redirection.stderr

  let method_trace = test.run_xsht_trace(
    ctx,
    """let demo_path = Path("demo.txt")
print \${demo_path.display()}
""",
    ["--trace", "--raw", "--trace-format", "jsonl"],
  )?

  {
    let {success: assertion_condition, stderr: assertion_message, ..} = method_trace
    assert assertion_condition, assertion_message
  }
  assert "\"kind\":\"method.call\"" in method_trace.stderr
  assert "\"kind\":\"method.result\"" in method_trace.stderr
  assert "\"api_id\":\"method.Path.display\"" in method_trace.stderr
  assert "\"api_id\":\"core.print\"" in method_trace.stderr

  let env_trace = test.run_xsht_trace(
    ctx,
    """run XSH_STAGE3_TRACE=value sh -c "true" ?
""",
    ["--raw"],
  )?

  {
    let {success: assertion_condition, stderr: assertion_message, ..} = env_trace
    assert assertion_condition, assertion_message
  }
  assert "env={b\"XSH_STAGE3_TRACE\":b\"value\"}" in env_trace.stderr

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

  assert cd_error.status == 3
  assert "kind=cwd.enter" in cd_error.stderr
  assert "kind=cwd.exit" in cd_error.stderr
  assert "index-out-of-range" in cd_error.stderr
}

test test_trace_output_covers_baseline_event_kinds { |ctx|
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

  {
    let {success: assertion_condition, stderr: assertion_message, ..} = success
    assert assertion_condition, assertion_message
  }

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
    assert kind in success.stderr
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

  assert runtime_error.status == 3
  assert "kind=runtime.error" in runtime_error.stderr
  assert "index-out-of-range" in runtime_error.stderr
}

test test_run_fixture_behaviors { |ctx|
  assert (run.text printf "%s\n" "hello world"?) == """hello world
"""

  let failed = test.run_script(
    ctx,
    """run false
""",
  )?

  assert failed.status == 3
  assert "nonzero-exit" in failed.stderr
  let status = run.status false
  assert status.exited_with(1)
  let text = run.text printf "%s" "hello" ?
  assert text == "hello"
  let raw = run.bytes head -c 1 /dev/zero ?
  assert raw == b"\0"
}

test test_signaled_status_exit_code_is_structured_error { |ctx|
  let output = test.run_script(
    ctx,
    """let status = run sh -c "kill -TERM $$"
let _ = status.exit_code()?
""",
  )?

  assert output.status == 3
  assert "status-kind" in output.stderr
}

test test_nested_traceback_includes_user_procs_and_pure_functions { |ctx|
  let output = test.run_script(
    ctx,
    """
pure leaf() -> Result[Unit] {
  let _ = Path.parse_bytes(b"bad\\0path")?
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
""",
  )?

  assert output.status == 3
  assert "call path:" in output.stderr
  assert "proc main" in output.stderr
  assert "proc outer" in output.stderr
  assert "pure middle" in output.stderr
  assert "pure leaf" in output.stderr
  assert "nul-path" in output.stderr
}

test test_foundation_literals_defers_streams_and_builders { |ctx|
  let root = test.temp_dir(ctx, name: "foundation")?
  let marker = fp"${root}/marker"
  defer marker.write("cleaned")?
  let file = fp"${root}/note.txt"
  file.write("""alpha
beta
""")?
  let content = file.read_text()?
  let mode = 0o755
  let label = f"mode ${mode}"
  let raw_lines = run.stream --text printf "%s\n" alpha beta gamma
  let lines = raw_lines
    |> drop(1)
    |> take(1)
  let total = [1, 2, 3] |> sum
  let unique = [1, 1, 2, 3] |> unique-by .
  let command = process.command {
    timeout = 2s
    run --timeout=1s echo ok
  }
  assert process.run(command)?.exited_with(0)
  assert mode == 493
  assert "493" in label
  assert lines[0] == "beta"
  assert total == 6
  assert unique[2] == 3
  assert content == """alpha
beta
"""
}

test test_run_timeout_error { |ctx|
  let output = test.run_script(
    ctx,
    """let _ = run --timeout=10ms sh -c "sleep 1" ?
""",
  )?
  assert output.status == 3
  assert "timeout" in output.stderr
}

proc run_capture_match_assignments(ok_cmd: Str) [process] -> Str {
  var text = ""
  var size = 0
  var stdout = ""
  var failure = ""
  match run.text printf "%s " $ok_cmd {
    Ok(body) => text = body.trim()
    Err(err) => text = f"text failed: ${err.message}"
  }

  if let Ok(body) = run.bytes printf "%s" $ok_cmd {
    size = body.len()
  }

  if let Ok(captured) = run.capture --text printf "%s " $ok_cmd {
    stdout = captured.stdout.trim()
  }

  match run.text sh -c "exit 3" {
    Ok(body) => failure = body.trim()
    Err(err) => failure = f"failed ${err.message != ""}"
  }

  let label = if let Ok(body) = run.text printf " %s " $ok_cmd { body.trim() } else { "none" }
  f"${text}|${size}|${stdout}|${failure}|${label}"
}

proc run_capture_propagated_method(value: Str) [process, error] -> Result[Str] {
  let body = run.text printf "%s " $value ?
  body.trim()
}

test test_run_capture_match_arm_bindings_keep_capture_types {
  assert run_capture_match_assignments("hi") == "hi|2|hi|failed true|hi"
  assert run_capture_propagated_method("ok")? == "ok"
}

test test_script_output_keeps_its_order_around_inheriting_children { |ctx|
  let root = test.temp_dir(ctx, name: "output-order")?
  let script = fp"${root}/order.xsh"
  script.write(r"""print "a"
run echo b
print "c"
let _ = run.status echo d
eprint "e"
run sh -c "echo f >&2"
print "g"
run echo h | run cat
let child = spawn run sh -c "sleep 0.1; echo j" ?
print "i"
let _ = wait child ?
let k = run.text echo k ?
print ${k.trim()}
""")?
  let stdout = fp"${root}/stdout.txt"
  let stderr = fp"${root}/stderr.txt"
  run "xsh" $script > $stdout 2> $stderr
  assert stdout.read_text()? == "a\nb\nc\nd\ng\nh\ni\nj\nk\n"
  assert stderr.read_text()? == "e\nf\n"
}

test test_tracebacks_locate_the_failing_command_and_current_call_path { |ctx|
  let failed_run = test.run_script(
    ctx,
    r"""proc backup(dest: Path) [process, error] {
  print "before"
  run sh -c "exit 4" $dest
}
backup(p"/nonexistent/out.tgz")?
""",
  )?
  assert failed_run.status == 3
  assert "error: nonzero-exit: `sh` exited with status 4" in failed_run.stderr, failed_run.stderr
  assert "pipeline segment" not in failed_run.stderr, failed_run.stderr
  assert ":3:3-" in failed_run.stderr, failed_run.stderr

  let pipeline = test.run_script(
    ctx,
    """run false | run true ?
""",
  )?
  assert "pipeline segment 0 `false` exited with status 1" in pipeline.stderr, pipeline.stderr

  let stale = test.run_script(
    ctx,
    r"""stream numbers() -> Stream[Int] {
  yield 1
  yield 2
}
proc reads(file: Path) [fs, error] -> Result[Str] {
  file.read_text()?
}
for _ in numbers() {
  let _ = reads(p"/nonexistent/traceback.txt")?
}
""",
  )?
  assert stale.status == 3
  assert "proc reads" in stale.stderr, stale.stderr
  assert "numbers" not in stale.stderr, stale.stderr
  assert "fs-read: /nonexistent/traceback.txt: " in stale.stderr, stale.stderr
}

test test_filesystem_errors_name_their_paths {
  let missing = p"/nonexistent/xsh-missing-dir"
  if let Err(error) = fs.metadata(missing) {
    assert "/nonexistent/xsh-missing-dir" in error.message, error.message
  } else {
    test.fail("expected a filesystem failure")?
  }
  for failure in [
    fs.mkdir(fp"${missing}/child"),
    fs.write(fp"${missing}/file.txt", "x"),
    fs.copy(fp"${missing}/a", fp"${missing}/b"),
  ] {
    if let Err(error) = failure {
      assert "/nonexistent/xsh-missing-dir" in error.message, error.message
    } else {
      test.fail("expected a filesystem failure")?
    }
  }
}
