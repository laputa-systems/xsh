type Ran = {status: Int, stdout: Bytes, stderr: Str}

# Runs core/rev.xsh by its real path inside `root`, capturing both streams.
proc rev_run(ctx: TestContext, root: Path, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/.out"
  let err = fp"{root}/.err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/rev.xsh".display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_rev_reverses_characters_of_files_and_stdin { |ctx|
  let root = test.temp_dir(ctx, name: "rev")?
  fp"{root}/in".write(b"abc\ncaf\xc3\xa9")

  assert rev_run(ctx, root, ["in"])?.stdout == b"cba\n\xc3\xa9fac"
  assert rev_run(ctx, root, [], b"one\ntwo\n")?.stdout == b"eno\nowt\n"
  assert rev_run(ctx, root, ["in", "-"], b"xy\n")?.stdout == b"cba\n\xc3\xa9facyx\n"
  assert rev_run(ctx, root, [], b"")?.stdout == b""
}

test test_rev_keeps_crlf_bytes_and_invalid_utf8_characters { |ctx|
  let root = test.temp_dir(ctx, name: "rev")?

  assert rev_run(ctx, root, [], b"ab\r\n")?.stdout == b"\rba\n"
  assert rev_run(ctx, root, [], b"a\xffb\n")?.stdout == b"b\xffa\n"
  assert rev_run(ctx, root, [], b"\xc3\xa9\xff\xe2\x82\xac\n")?.stdout == b"\xe2\x82\xac\xff\xc3\xa9\n"
}

test test_rev_zero_option_uses_nul_lines { |ctx|
  let root = test.temp_dir(ctx, name: "rev")?

  assert rev_run(ctx, root, ["-0"], b"ab\0cd\n\0")?.stdout == b"ba\0\ndc\0"
  assert rev_run(ctx, root, ["--zero"], b"ab\0cd")?.stdout == b"ba\0dc"
  assert rev_run(ctx, root, ["--ze"], b"ab\0")?.stdout == b"ba\0"
}

test test_rev_reports_unreadable_operands_and_continues { |ctx|
  let root = test.temp_dir(ctx, name: "rev")?
  fp"{root}/ok".write(b"ab\n")

  let result = rev_run(ctx, root, ["missing", "ok"])?
  assert result.status == 1
  assert result.stdout == b"ba\n"
  assert result.stderr == "rev: cannot open missing: No such file or directory\n", result.stderr
}

test test_rev_getopt_diagnostics_help_and_version { |ctx|
  let root = test.temp_dir(ctx, name: "rev")?

  let bad = rev_run(ctx, root, ["-z"])?
  assert bad.status == 1
  assert bad.stderr == "rev: invalid option -- 'z'\nTry 'rev --help' for more information.\n", bad.stderr

  let help = rev_run(ctx, root, ["-h"])?
  assert help.status == 0
  assert "rev [options] [file ...]" in help.stdout.utf8()?
  assert rev_run(ctx, root, ["-V"])?.stdout.starts_with(b"rev")
}
