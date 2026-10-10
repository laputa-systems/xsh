type Ran = {status: Int, stdout: Str, stderr: Str, bytes: Bytes}

# Runs core/echo.xsh by its real path (so the invoked name is echo and
# `lib.gnu` resolves beside it), capturing both streams.
proc applet_run(
  ctx: TestContext,
  args: List[Str],
  vars: Record = {LC_ALL: "C"},
  stdin = b"",
) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "echo")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/echo.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, vars, stdin, out, err)
  let status = process.run(plan)?
  let raw = out.read_bytes()?

  Ok({status: status.exit_code()?, stdout: raw.utf8() ?? "", stderr: err.read_text()?, bytes: raw})
}

# Like applet_run, but operands are paths so a test can pass undecodable bytes.
proc applet_run_paths(ctx: TestContext, args: List[Path]) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "echo-raw")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let argv = [ctx.xsh_bin, fp"{ctx.core_dir}/echo.xsh"].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  let raw = out.read_bytes()?

  Ok({status: status.exit_code()?, stdout: raw.utf8() ?? "", stderr: err.read_text()?, bytes: raw})
}

test test_echo_joins_arguments_with_spaces { |ctx|
  assert applet_run(ctx, ["a", "b  c"])?.stdout == "a b  c\n"
  assert applet_run(ctx, [])?.stdout == "\n"
}

test test_echo_preserves_non_utf8_argument_bytes { |ctx|
  let invalid = Path.parse_bytes(b"argument-\xfc")?
  let result = applet_run_paths(ctx, [p"-n", invalid])?
  assert result.status == 0, result.stderr
  assert result.bytes == b"argument-\xfc"
  assert applet_run_paths(ctx, [invalid])?.bytes == b"argument-\xfc\n"
  assert applet_run_paths(ctx, [p"-e", invalid])?.bytes == b"argument-\xfc\n"
}

test test_echo_n_suppresses_the_newline_and_options_stop_at_text { |ctx|
  assert applet_run(ctx, ["-n", "hi"])?.stdout == "hi"
  assert applet_run(ctx, ["-nn", "-n", "hi"])?.stdout == "hi"
  assert applet_run(ctx, ["hi", "-n"])?.stdout == "hi -n\n"
  assert applet_run(ctx, ["-x", "-n"])?.stdout == "-x -n\n", "an unknown flag letter makes the word text"
  assert applet_run(ctx, ["-", "-n"])?.stdout == "- -n\n"
}

test test_echo_escapes_need_e_and_last_option_wins { |ctx|
  assert applet_run(ctx, ["a\\tb"])?.stdout == "a\\tb\n"
  assert applet_run(ctx, ["-e", "a\\tb\\\\"])?.stdout == "a\tb\\\n"
  assert applet_run(ctx, ["-e", "-E", "\\n"])?.stdout == "\\n\n"
  assert applet_run(ctx, ["-E", "-e", "\\n"])?.stdout == "\n\n"
  assert applet_run(ctx, ["-neE", "\\n"])?.stdout == "\\n"
}

test test_echo_numeric_and_hex_escapes_write_bytes { |ctx|
  assert applet_run(ctx, ["-e", "\\0101\\101\\x41"])?.stdout == "AAA\n"
  assert applet_run(ctx, ["-e", "\\0501"])?.bytes == b"A\n", "octal values wrap to one byte"
  assert applet_run(ctx, ["-e", "\\777"])?.bytes == b"\xff\n"
  assert applet_run(ctx, ["-e", "\\xff"])?.bytes == b"\xff\n"
  assert applet_run(ctx, ["-e", "\\xf0\\x9f\\x98\\x82"])?.stdout == "😂\n"
  assert applet_run(ctx, ["-e", "a\\0 b"])?.bytes == b"a\0 b\n"
  assert applet_run(ctx, ["-e", "\\08"])?.bytes == b"\08\n"
}

test test_echo_unrecognized_escapes_keep_the_backslash { |ctx|
  for word in ["\\8", "\\x", "\\xg", "foo\\ bar", "\\u0041", "\\U00000041", "\\\""] {
    assert applet_run(ctx, ["-e", word])?.stdout == word + "\n", word
  }

  assert applet_run(ctx, ["-e", "end\\"])?.stdout == "end\\\n"
}

test test_echo_backslash_c_stops_all_output { |ctx|
  assert applet_run(ctx, ["-e", "a\\cb", "c"])?.stdout == "a"
  assert applet_run(ctx, ["-e", "x", "\\c", "y"])?.stdout == "x "
}

test test_echo_double_dash_is_text_after_other_arguments { |ctx|
  assert applet_run(ctx, ["a", "--", "b"])?.stdout == "a -- b\n"
  assert applet_run(ctx, ["-n", "--", "a"])?.stdout == "-- a"
  assert applet_run(ctx, ["-e", "--", "foo\\n"])?.stdout == "-- foo\n\n"
}

test test_echo_help_and_version_only_as_the_sole_argument { |ctx|
  let help = applet_run(ctx, ["--help"])?
  assert help.status == 0
  assert "Usage: echo" in help.stdout
  assert applet_run(ctx, ["--version"])?.stdout.starts_with("echo ")
  assert applet_run(ctx, ["--help", "--help"])?.stdout == "--help --help\n"
  assert applet_run(ctx, ["--he"])?.stdout == "--he\n"
  assert applet_run(ctx, ["--version", "x"])?.stdout == "--version x\n"
}

test test_echo_posixly_correct_always_expands_and_reads_only_leading_n { |ctx|
  let posix = {LC_ALL: "C", POSIXLY_CORRECT: "1"}
  assert applet_run(ctx, ["--help"], vars: posix)?.stdout == "--help\n"
  assert applet_run(ctx, ["-E", "-n", "foo"], vars: posix)?.stdout == "-E -n foo\n"
  assert applet_run(ctx, ["foo\\tbar"], vars: posix)?.stdout == "foo\tbar\n"
  assert applet_run(ctx, ["-n", "-E", "foo\\cbar"], vars: posix)?.stdout == "foo"
}
