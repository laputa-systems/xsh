type Ran = {status: Int, stdout: Bytes, stderr: Str}

# Runs core/uniq.xsh by its real path inside `root`, capturing both streams.
proc uniq_run(ctx: TestContext, root: Path, args: List[Str], input = b"", posix = "") [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/.out"
  let err = fp"{root}/.err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/uniq.xsh".display()].extend(args)
  let plan = if posix == "" {
    process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, input, out, err)
  } else {
    process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C", _POSIX2_VERSION: posix}, input, out, err)
  }
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_uniq_merges_adjacent_lines_and_counts { |ctx|
  let root = test.temp_dir(ctx, name: "uniq")?

  assert uniq_run(ctx, root, [], b"a\na\nb\nc\nc\n")?.stdout == b"a\nb\nc\n"
  assert uniq_run(ctx, root, ["-c"], b"a\na\nb\n")?.stdout == b"      2 a\n      1 b\n"
  assert uniq_run(ctx, root, [], b"x\nx")?.stdout == b"x\n", "an unterminated last line still gets a newline"
  assert uniq_run(ctx, root, [], b"")?.stdout == b""
}

test test_uniq_repeated_unique_and_all_repeated { |ctx|
  let root = test.temp_dir(ctx, name: "uniq")?
  let input = b"a\na\nb\nc\nc\n"

  assert uniq_run(ctx, root, ["-d"], input)?.stdout == b"a\nc\n"
  assert uniq_run(ctx, root, ["-u"], input)?.stdout == b"b\n"
  assert uniq_run(ctx, root, ["-d", "-u"], input)?.stdout == b"", "-d and -u together select nothing"
  assert uniq_run(ctx, root, ["-D"], input)?.stdout == b"a\na\nc\nc\n"
  assert uniq_run(ctx, root, ["--all-repeated=separate"], input)?.stdout == b"a\na\n\nc\nc\n"
  assert uniq_run(ctx, root, ["--all-repeated=prepend"], input)?.stdout == b"\na\na\n\nc\nc\n"
  assert uniq_run(ctx, root, ["--all-repeated=p"], input)?.stdout == b"\na\na\n\nc\nc\n", "method names abbreviate"
  assert uniq_run(ctx, root, ["--all-repeated=prepend", "-D"], input)?.stdout == b"a\na\nc\nc\n", "the last of -D and --all-repeated wins"
}

test test_uniq_group_methods { |ctx|
  let root = test.temp_dir(ctx, name: "uniq")?
  let input = b"a\na\nb\n"

  assert uniq_run(ctx, root, ["--group"], input)?.stdout == b"a\na\n\nb\n"
  assert uniq_run(ctx, root, ["--group=prepend"], input)?.stdout == b"\na\na\n\nb\n"
  assert uniq_run(ctx, root, ["--group=append"], input)?.stdout == b"a\na\n\nb\n\n"
  assert uniq_run(ctx, root, ["--group=both"], input)?.stdout == b"\na\na\n\nb\n\n"
  assert uniq_run(ctx, root, ["--group=both"], b"")?.stdout == b""
}

test test_uniq_skips_fields_chars_and_limits_width { |ctx|
  let root = test.temp_dir(ctx, name: "uniq")?

  assert uniq_run(ctx, root, ["-f", "1"], b"a a\nb a\n")?.stdout == b"a a\n"
  assert uniq_run(ctx, root, ["-f1"], b"a\ta\nb\ta\n")?.stdout == b"a\ta\n", "a field is blanks then non-blanks"
  assert uniq_run(ctx, root, ["-s", "1"], b"aaa\nbaa\n")?.stdout == b"aaa\n"
  assert uniq_run(ctx, root, ["-w", "1"], b"abc\nabd\n")?.stdout == b"abc\n"
  assert uniq_run(ctx, root, ["-f", "1", "-s", "1", "-w", "1"], b"x ab1\ny ac1\n")?.stdout == b"x ab1\n"
  assert uniq_run(ctx, root, ["-f", "1", "-f", "2"], b"x y a\nz w a\n")?.stdout == b"x y a\n", "the last -f wins"
  assert uniq_run(ctx, root, ["-i"], b"A\na\n")?.stdout == b"A\n"
}

test test_uniq_obsolete_numeric_options { |ctx|
  let root = test.temp_dir(ctx, name: "uniq")?

  assert uniq_run(ctx, root, ["-1"], b"a a\nb a\n")?.stdout == b"a a\n", "-N skips N fields"
  assert uniq_run(ctx, root, ["+1"], b"aaa\nbaa\n", "199209")?.stdout == b"aaa\n", "+N skips N characters under the old POSIX"

  let plain = uniq_run(ctx, root, ["+1"], b"aaa\n")?
  assert plain.status == 1, "without the old POSIX, +1 is a file name"
  assert plain.stderr == "uniq: +1: No such file or directory\n", plain.stderr
}

test test_uniq_zero_terminated { |ctx|
  let root = test.temp_dir(ctx, name: "uniq")?

  assert uniq_run(ctx, root, ["-z"], b"a\0a\0b")?.stdout == b"a\0b\0"
  assert uniq_run(ctx, root, ["-z"], b"a\na\n")?.stdout == b"a\na\n\0"
  assert uniq_run(ctx, root, ["-dz"], b"a\na\n")?.stdout == b"", "newlines are data under -z"
}

test test_uniq_input_and_output_operands { |ctx|
  let root = test.temp_dir(ctx, name: "uniq")?
  fp"{root}/in".write("a\na\nb\n")

  assert uniq_run(ctx, root, ["in"])?.stdout == b"a\nb\n"
  assert uniq_run(ctx, root, ["in", "out"])?.stdout == b""
  assert fp"{root}/out".read_bytes()? == b"a\nb\n"
  assert uniq_run(ctx, root, ["-", "out2"], b"x\nx\n")?.status == 0
  assert fp"{root}/out2".read_bytes()? == b"x\n"
}

test test_uniq_errors_follow_gnu_wording { |ctx|
  let root = test.temp_dir(ctx, name: "uniq")?

  let missing = uniq_run(ctx, root, ["nosuchfile"])?
  assert missing.status == 1
  assert missing.stderr == "uniq: nosuchfile: No such file or directory\n", missing.stderr

  let meaningless = uniq_run(ctx, root, ["-D", "-c"])?
  assert meaningless.status == 1
  assert meaningless.stderr == "uniq: printing all duplicated lines and repeat counts is meaningless\nTry 'uniq --help' for more information.\n", meaningless.stderr

  let exclusive = uniq_run(ctx, root, ["--group", "-c"])?
  assert exclusive.stderr == "uniq: --group is mutually exclusive with -c/-d/-D/-u\nTry 'uniq --help' for more information.\n", exclusive.stderr

  let method = uniq_run(ctx, root, ["--group=badoption"])?
  assert method.status == 1
  assert method.stderr == "uniq: invalid argument 'badoption' for '--group'\nValid arguments are:\n  - 'prepend'\n  - 'append'\n  - 'separate'\n  - 'both'\nTry 'uniq --help' for more information.\n", method.stderr

  let skip = uniq_run(ctx, root, ["-f", "x"])?
  assert skip.stderr == "uniq: 'x': invalid number of fields to skip\n", skip.stderr

  let extra = uniq_run(ctx, root, ["a", "b", "c"])?
  assert extra.stderr == "uniq: extra operand 'c'\nTry 'uniq --help' for more information.\n", extra.stderr
}

test test_uniq_help_and_version_go_to_stdout { |ctx|
  let root = test.temp_dir(ctx, name: "uniq")?
  let help = uniq_run(ctx, root, ["--help"])?

  assert help.status == 0
  assert help.stderr == ""
  assert help.stdout.utf8()?.starts_with("Usage: uniq [OPTION]... [INPUT [OUTPUT]]")

  let version = uniq_run(ctx, root, ["--version"])?
  assert version.stdout.utf8()?.starts_with("uniq")
}
