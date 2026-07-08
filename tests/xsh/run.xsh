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
  return Ok("proc-tail")
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
    "grouped run\n",
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
  let p = path.absolute(Path("target/../target/lang-absolute-demo"))?

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

proc test_script_stdout_can_emit_invalid_utf8_bytes() [io, error] {
  io.write_stdout_bytes(b"\xff\x00a")?
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

proc test_function_tail_values_return_declared_values() [error] {
  let obj = run_object_path(Path("main.c"))
  let values = ["ok"] |> map { |value| run_command_tail(value)? }
  let marker_text = run_choose_tail("ignored")?

  test.eq(marker_text, "proc-tail")?
  test.eq(obj.name(), "main.o")?
  test.eq(values[0], "ok.ok")?
  test.error_kind(run_result_unit_tail_error(), "TailError.tail_error")?
}

proc test_byte_pipeline_executes_without_shell_and_redirects_stdout(ctx: TestContext) [fs, process, error] {
  let out = test.temp_path(ctx)
  run printf "%s\n" "hello" | run tr a-z A-Z > (out) ?

  test.eq(out.read_bytes()?, b"HELLO\n")?
}

proc test_acceptance_tar_gzip_pipeline_writes_archive(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "tar-gzip")?
  let src = fp"${root}/src"
  let tarball = fp"${root}/archive.tar.gz"
  src.mkdir()?
  fp"${src}/file.txt".write("contents\n")?

  cd root {
    run tar cf - src | run gzip -9 > (tarball) ?
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
  let lined = fp"${root}/line\nname"
  let dashed = fp"${root}/-leading"
  let errlog = fp"${root}/err log"

  run printf "a" > (spaced) ?
  run printf "b" >> (spaced) ?
  run cat < (spaced) > (lined) ?
  run cat < (lined) > (dashed) ?
  run sh -c "printf err >&2" 2> (errlog) ?
  run sh -c "printf more >&2" 2>> (errlog) ?
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
  let broken = run yes | run head -n 1 > (sink)
  test.ok(broken.segments[0].kind == "signal")?
  test.eq(sink.read_text()?, "y\n")?
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
