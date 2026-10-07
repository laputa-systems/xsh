type Ran = {status: Int, stdout: Bytes, stderr: Str}

# Runs core/tee.xsh by its real path inside `root`, capturing both streams.
proc tee_run(ctx: TestContext, root: Path, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/.out"
  let err = fp"{root}/.err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/tee.xsh".display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_tee_copies_stdin_to_stdout_and_every_file { |ctx|
  let root = test.temp_dir(ctx, name: "tee")?
  let result = tee_run(ctx, root, ["one", "two", "-"], b"hello\0\xff\n")?

  assert result.status == 0
  assert result.stdout == b"hello\0\xff\n"
  assert fp"{root}/one".read_bytes()? == b"hello\0\xff\n"
  assert fp"{root}/two".read_bytes()? == b"hello\0\xff\n"
  assert fp"{root}/-".read_bytes()? == b"hello\0\xff\n", "- is a file name for tee"
}

test test_tee_truncates_by_default_and_appends_with_a { |ctx|
  let root = test.temp_dir(ctx, name: "tee")?
  fp"{root}/log1".write(b"old1\n")
  fp"{root}/log2".write(b"old2\n")

  let _ = tee_run(ctx, root, ["log1"], b"new\n")?
  assert fp"{root}/log1".read_bytes()? == b"new\n"

  let _ = tee_run(ctx, root, ["-a", "log1", "-a", "log2"], b"\xfe\n")?
  assert fp"{root}/log1".read_bytes()? == b"new\n\xfe\n"
  assert fp"{root}/log2".read_bytes()? == b"old2\n\xfe\n"

  let _ = tee_run(ctx, root, ["--append", "fresh"], b"x")?
  assert fp"{root}/fresh".read_bytes()? == b"x"
}

test test_tee_without_input_creates_empty_files { |ctx|
  let root = test.temp_dir(ctx, name: "tee")?

  assert tee_run(ctx, root, ["empty"])?.stdout == b""
  assert fp"{root}/empty".read_bytes()? == b""
}

test test_tee_write_errors_follow_the_output_error_mode { |ctx|
  let root = test.temp_dir(ctx, name: "tee")?
  fp"{root}/dir".mkdir()

  let default = tee_run(ctx, root, ["dir", "good"], b"data")?
  assert default.status == 1
  assert default.stdout == b"data"
  assert fp"{root}/good".read_bytes()? == b"data"
  assert default.stderr.starts_with("tee: dir: "), default.stderr

  let exits = tee_run(ctx, root, ["--output-error=exit", "dir", "late"], b"data")?
  assert exits.status == 1
  assert ! fp"{root}/late".exists()?, "exit mode stops at the first failure"

  let full = tee_run(ctx, root, ["-p", "/dev/full", "kept"], b"data")?
  assert full.status == 1
  assert full.stderr == "tee: /dev/full: No space left on device\n", full.stderr
  assert fp"{root}/kept".read_bytes()? == b"data"
}

test test_tee_output_error_values_use_argmatch { |ctx|
  let root = test.temp_dir(ctx, name: "tee")?

  for mode in ["warn", "warn-nopipe", "exit", "exit-nopipe", "warn-", "exit-nop", "--output-error"] {
    let args = if mode == "--output-error" { [mode, "out"] } else { [f"--output-error={mode}", "out"] }
    assert tee_run(ctx, root, args, b"x")?.status == 0, mode
  }

  let bad = tee_run(ctx, root, ["--output-error=bogus"], b"")?
  assert bad.status == 1
  assert bad.stderr == "tee: invalid argument 'bogus' for '--output-error'\nValid arguments are:\n  - 'warn'\n  - 'warn-nopipe'\n  - 'exit'\n  - 'exit-nopipe'\nTry 'tee --help' for more information.\n", bad.stderr

  let ambiguous = tee_run(ctx, root, ["--output-error=w"], b"")?
  assert "ambiguous argument 'w'" in ambiguous.stderr
}

test test_tee_rejects_ignore_interrupts_explicitly { |ctx|
  let root = test.temp_dir(ctx, name: "tee")?
  let result = tee_run(ctx, root, ["-i"], b"x")?

  assert result.status == 1
  assert result.stdout == b""
  assert result.stderr == "tee: option '-i' is not supported: interrupts cannot be ignored\nTry 'tee --help' for more information.\n", result.stderr
}

test test_tee_reports_a_stdin_read_error { |ctx|
  let root = test.temp_dir(ctx, name: "tee")?
  let script = fp"{ctx.core_dir}/tee.xsh"
  let err = fp"{root}/.err"

  cd $root {
    let status = run.status ${ctx.xsh_bin} $script < $root 2> $err
    assert status.exited_with(1)
  }

  assert err.read_text()? == "tee: read error: Is a directory\n"
}

test test_tee_getopt_diagnostics_and_help { |ctx|
  let root = test.temp_dir(ctx, name: "tee")?

  let bad = tee_run(ctx, root, ["--definitely-invalid"])?
  assert bad.status == 1
  assert bad.stderr == "tee: unrecognized option '--definitely-invalid'\nTry 'tee --help' for more information.\n", bad.stderr

  assert "Usage: tee [OPTION]... [FILE]..." in tee_run(ctx, root, ["--help"])?.stdout as Str
  assert tee_run(ctx, root, ["--version"])?.stdout.starts_with(b"tee")
}

test test_tee_short_help_matches_long_help { |ctx|
  let root = test.temp_dir(ctx, name: "tee")?
  let short = tee_run(ctx, root, ["-h"])?
  let long = tee_run(ctx, root, ["--help"])?

  assert short.status == 0
  assert short.stderr == ""
  assert short.stdout == long.stdout
}

test test_tee_stdout_write_failure_preserves_other_outputs { |ctx|
  if ! p"/dev/full".exists() { test.skip("/dev/full is not available") }
  assert p"/dev/full".metadata()?.mode / 4096 % 16 == 2, "/dev/full must be a character device"
  let root = test.temp_dir(ctx, name: "tee-stdout-failure")?
  let input = test.temp_file(ctx, name: "tee-input", contents: bytes.concat([b"data\0\xff" for _ in range(20000)]))?
  let script = fp"{ctx.core_dir}/tee.xsh"
  let out = fp"{root}/copied"
  let err = fp"{root}/error"
  for mode in ["warn", "warn-nopipe", "default"] {
    let option = if mode == "default" { "--" } else { f"--output-error={mode}" }
    let status = run.status sh -c "exec \"$0\" \"$1\" \"$2\" \"$3\" < \"$4\" > /dev/full 2> \"$5\"" ${ctx.xsh_bin} $script $option $out $input $err
    assert status.exited_with(1), mode
    assert out.read_bytes()? == input.read_bytes()?, mode
    assert err.read_text()? == "tee: 'standard output': No space left on device\n", err.read_text()?
  }
}

test test_tee_streams_multiple_chunks_without_reopening_outputs { |ctx|
  let root = test.temp_dir(ctx, name: "tee-chunks")?
  let input = bytes.concat([b"\0\xffline\n" for _ in range(20000)])
  fp"{root}/out".write(b"prefix")
  let result = tee_run(ctx, root, ["-a", "out"], input)?
  assert result.status == 0, result.stderr
  assert result.stdout == input
  assert fp"{root}/out".read_bytes()? == bytes.concat([b"prefix", input])
}

test test_tee_broken_stdout_pipe_modes_keep_file_writes { |ctx|
  let root = test.temp_dir(ctx, name: "tee-pipe-error")?
  let input = test.temp_file(ctx, name: "tee-pipe-input", contents: bytes.concat([b"data\0\xff" for _ in range(20000)]))?
  let script = fp"{ctx.core_dir}/tee.xsh"
  let out = fp"{root}/copied"
  let err = fp"{root}/error"
  let code = fp"{root}/status"
  for mode in ["default", "pipe", "warn", "warn-nopipe", "exit", "exit-nopipe"] {
    let option = if mode == "default" { "--" } else if mode == "pipe" { "-p" } else { f"--output-error={mode}" }
    let status = run.status sh -c "(\"$0\" \"$1\" \"$2\" \"$3\" < \"$4\" 2> \"$6\"; printf '%s' \"$?\" > \"$5\") | head -c 0" ${ctx.xsh_bin} $script $option $out $input $code $err
    assert status.exited_with(0)
    let tee_status = if mode == "default" { "141" } else if mode in ["warn", "exit"] { "1" } else { "0" }
    assert code.read_text()? == tee_status, code.read_text()?
    if mode in ["default", "exit"] {
      assert out.read_bytes()?.len() < input.read_bytes()?.len(), mode
    } else {
      assert out.read_bytes()? == input.read_bytes()?, mode
    }
    assert err.read_text()? == (if mode in ["warn", "exit"] { "tee: 'standard output': Broken pipe\n" } else { "" }), err.read_text()?
  }
}

test test_tee_exit_mode_finishes_the_current_chunk_after_stdout_failure { |ctx|
  if ! p"/dev/full".exists() { test.skip("/dev/full is not available") }
  assert p"/dev/full".metadata()?.mode / 4096 % 16 == 2, "/dev/full must be a character device"
  let root = test.temp_dir(ctx, name: "tee-stdout-exit")?
  let input = test.temp_file(ctx, name: "tee-exit-input", contents: b"data")?
  let out = fp"{root}/copied"
  let err = fp"{root}/error"
  let script = fp"{ctx.core_dir}/tee.xsh"
  let status = run.status sh -c "exec \"$0\" \"$1\" --output-error=exit \"$2\" < \"$3\" > /dev/full 2> \"$4\"" ${ctx.xsh_bin} $script $out $input $err
  assert status.exited_with(1)
  assert out.read_bytes()? == b"data"
  assert err.read_text()? == "tee: 'standard output': No space left on device\n"
}

test test_tee_exit_mode_reports_each_failed_output_in_the_current_chunk { |ctx|
  if ! p"/dev/full".exists() { test.skip("/dev/full is not available") }
  assert p"/dev/full".metadata()?.mode / 4096 % 16 == 2, "/dev/full must be a character device"
  let root = test.temp_dir(ctx, name: "tee-exit-errors")?
  let input = test.temp_file(ctx, name: "tee-exit-errors-input", contents: bytes.concat([b"data\0\xff" for _ in range(20000)]))?
  let script = fp"{ctx.core_dir}/tee.xsh"
  let out = fp"{root}/copied"
  let err = fp"{root}/error"
  let code = fp"{root}/status"
  let status = run.status sh -c "(\"$0\" \"$1\" --output-error=exit \"$2\" /dev/full < \"$3\" 2> \"$5\"; printf '%s' \"$?\" > \"$4\") | head -c 0" ${ctx.xsh_bin} $script $out $input $code $err

  assert status.exited_with(0)
  assert code.read_text()? == "1"
  assert err.read_text()? == "tee: 'standard output': Broken pipe\ntee: /dev/full: No space left on device\n", err.read_text()?
  assert out.read_bytes()?.len() > 0
  assert out.read_bytes()?.len() < input.read_bytes()?.len()
}

test test_tee_nopipe_mode_does_not_wait_for_idle_input_when_stdout_is_broken { |ctx|
  let root = test.temp_dir(ctx, name: "tee-idle-pipe")?
  let input = fp"{root}/input"
  let output = fp"{root}/output"
  let script = fp"{ctx.core_dir}/tee.xsh"
  let status = run.status timeout -s KILL 2 sh -c "mkfifo \"$2\" \"$3\"; exec 3<> \"$2\"; (exec 4< \"$3\") & reader=$!; exec 4> \"$3\"; wait \"\$reader\"; exec \"$0\" \"$1\" -p < \"$2\" >&4" ${ctx.xsh_bin} $script $input $output
  assert status.exited_with(0), "tee blocked on idle input after its only output pipe broke"
}

test test_tee_nopipe_mode_stops_when_stdout_and_file_fifo_readers_are_gone { |ctx|
  let root = test.temp_dir(ctx, name: "tee-idle-file-pipe")?
  let input = fp"{root}/input"
  let output = fp"{root}/stdout"
  let file = fp"{root}/file"
  let script = fp"{ctx.core_dir}/tee.xsh"
  let status = run.status timeout -s KILL 2 sh -c "mkfifo \"$2\" \"$3\" \"$4\"; exec 3<> \"$2\"; (exec 4< \"$3\") & reader=$!; exec 4> \"$3\"; wait \"\$reader\"; (exec 5< \"$4\") & exec \"$0\" \"$1\" -p \"$4\" < \"$2\" >&4" ${ctx.xsh_bin} $script $input $output $file
  assert status.exited_with(0), "tee blocked on idle input after all output pipe readers closed"
}
