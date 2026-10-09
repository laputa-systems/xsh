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

# Runs tee with stdout connected to a pipe whose reader exits without consuming
# data. A large stdin makes the broken pipe deterministic before tee finishes.
proc tee_broken_pipe_run(ctx: TestContext, root: Path, args: List[Str]) [fs, process, error] -> Result[Ran] {
  const pipeline = """head -c 1000000 /dev/zero | {
  xsh=$1; shift
  applet=$1; shift
  status_file=$1; shift
  "$xsh" "$applet" "$@"
  rc=$?
  printf '%s\\n' "$rc" > "$status_file"
} | head -c 0 >/dev/null"""
  let out = fp"{root}/.pipe-out"
  let err = fp"{root}/.pipe-err"
  let status_file = fp"{root}/.pipe-status"
  let argv = [
    "sh", "-c", pipeline, "sh", ctx.xsh_bin.display(),
    fp"{ctx.core_dir}/tee.xsh".display(), status_file.display(),
  ].extend(args)
  let plan = process.command_argv("sh", argv, root, {LC_ALL: "C"}, b"", out, err)
  let _ = process.run(plan)?
  let status = status_file.read_text()?.trim().parse_int() ?? -1
  Ok({status: status, stdout: out.read_bytes()?, stderr: err.read_text()?})
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

test test_tee_broken_pipe_modes { |ctx|
  let root = test.temp_dir(ctx, name: "tee-pipe")?
  let all = bytes.zero(1000000)?

  let default = tee_broken_pipe_run(ctx, root, ["default"])?
  assert default.status == 141
  assert default.stderr == ""
  let default_file = fp"{root}/default"
  assert default_file.exists()?
  let default_data = default_file.read_bytes()?
  assert default_data.len() < all.len()
  assert all.starts_with(default_data)

  for options in [["-p"], ["--output-error=warn-nopipe"], ["--output-error=exit-nopipe"]] {
    let file = f"continue-{options[0]}"
    let result = tee_broken_pipe_run(ctx, root, options.extend([file]))?
    assert result.status == 0, options[0]
    assert result.stderr == "", options[0]
    assert fp"{root}/{file}".read_bytes()? == all, options[0]
  }

  let warned = tee_broken_pipe_run(ctx, root, ["--output-error=warn", "warned"])?
  assert warned.status == 1
  assert warned.stderr == "tee: 'standard output': Broken pipe\n", warned.stderr
  assert fp"{root}/warned".read_bytes()? == all

  let exited = tee_broken_pipe_run(ctx, root, ["--output-error=exit", "exited"])?
  assert exited.status == 1
  assert exited.stderr == "tee: 'standard output': Broken pipe\n", exited.stderr
  let exited_data = fp"{root}/exited".read_bytes()?
  assert exited_data.len() < all.len()
  assert all.starts_with(exited_data)

  let full = tee_broken_pipe_run(ctx, root, ["-p", "kept", "/dev/full"])?
  assert full.status == 1
  assert full.stderr == "tee: /dev/full: No space left on device\n", full.stderr
  assert fp"{root}/kept".read_bytes()? == all
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

  let help = tee_run(ctx, root, ["--help"])?.stdout
  let short_help = tee_run(ctx, root, ["-h"])?.stdout
  assert help == short_help
  assert "Usage: tee [OPTION]... [FILE]..." in help.utf8()?
  assert tee_run(ctx, root, ["--version"])?.stdout.starts_with(b"tee")
}
