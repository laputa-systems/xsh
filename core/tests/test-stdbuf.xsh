type Ran = {status: Int, stdout: Str, stderr: Str}

proc stdbuf_run(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "stdbuf")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/stdbuf.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let status = process.run(
    process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", out, err),
  )?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_stdbuf_passes_command_status_and_arguments { |ctx|
  let echoed = stdbuf_run(ctx, ["-o0", "echo", "-n", "buffered"])?
  assert echoed.status == 0
  assert echoed.stdout == "buffered"
  assert echoed.stderr == ""

  assert stdbuf_run(ctx, ["-e0", "sh", "-c", "exit 7"])?.status == 7
}

test test_stdbuf_accepts_line_and_size_modes { |ctx|
  assert stdbuf_run(ctx, ["--output=L", "true"])?.status == 0
  assert stdbuf_run(ctx, ["-o1K", "true"])?.status == 0
  assert stdbuf_run(ctx, ["-i", "1024", "-e0", "true"])?.status == 0
}

test test_stdbuf_line_buffering_flushes_while_command_runs { |ctx|
  if let Err(_) = process.which("awk") { test.skip("awk is required to check stdio buffering") }

  let root = test.temp_dir(ctx, name: "stdbuf-live")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/stdbuf.xsh"
  let awk = "BEGIN { printf \"line\\n\"; while (1) value += 1 }"
  let argv = [ctx.xsh_bin.display(), script.display(), "-oL", "awk", awk]
  let child = spawn process.command_argv(
    ctx.xsh_bin,
    argv,
    root,
    {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""},
    b"",
    out,
    err,
    new_session: true,
  )?

  time.sleep(75ms)
  let finished = process.wait_timeout([child], time.millis(5))?
  let output = out.read_bytes()?.utf8() ?? ""
  if finished == null {
    let _ = process.kill_group(child.pid, "KILL")
    let _ = process.wait_any([child])?
  }

  assert finished == null, "the buffering probe should still be running"
  assert output == "line\n", output
}

test test_stdbuf_rejects_bad_modes_and_stdin_line_buffering { |ctx|
  let bad = stdbuf_run(ctx, ["-o1024R", "true"])?
  assert bad.status == 125
  assert bad.stderr == "stdbuf: invalid mode '1024R': Value too large for defined data type\n", bad.stderr

  let unknown = stdbuf_run(ctx, ["-o6pq", "true"])?
  assert unknown.status == 125
  assert unknown.stderr == "stdbuf: invalid mode '6pq'\n", unknown.stderr

  let line_stdin = stdbuf_run(ctx, ["-iL", "true"])?
  assert line_stdin.status == 125
  assert line_stdin.stderr == "stdbuf: line buffering stdin is meaningless\n", line_stdin.stderr
}

test test_stdbuf_requires_mode_and_command { |ctx|
  let no_mode = stdbuf_run(ctx, ["true"])?
  assert no_mode.status == 125
  assert no_mode.stderr == "stdbuf: missing operand\nTry 'stdbuf --help' for more information.\n", no_mode.stderr

  let no_command = stdbuf_run(ctx, ["-o0"])?
  assert no_command.status == 125
}

test test_stdbuf_missing_command_is_127 { |ctx|
  let result = stdbuf_run(ctx, ["-o0", "xsh-no-such-command-compat"])?
  assert result.status == 127
  assert "failed to execute 'xsh-no-such-command-compat'" in result.stderr
}

test test_stdbuf_rejects_directories_as_commands { |ctx|
  let result = stdbuf_run(ctx, ["-o0", "/"])?
  assert result.status == 126
  assert "Permission denied" in result.stderr
}

test test_stdbuf_help_and_version { |ctx|
  let help = stdbuf_run(ctx, ["--help"])?
  assert help.status == 0
  assert help.stdout.starts_with("Usage: stdbuf OPTION... COMMAND")

  let version = stdbuf_run(ctx, ["--version"])?
  assert version.status == 0
  assert version.stdout.starts_with("stdbuf ")
}
