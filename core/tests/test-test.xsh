type Ran = {status: Int, stdout: Str, stderr: Str, bytes: Bytes}

# Runs core/test.xsh by its real path (so the invoked name is test and
# `lib.gnu` resolves beside it), capturing both streams.
proc applet_run(
  ctx: TestContext,
  args: List[Str],
  vars: Record = {LC_ALL: "C"},
  stdin = b"",
) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "test")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/test.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, vars, stdin, out, err)
  let status = process.run(plan)?
  let raw = out.read_bytes()?

  Ok({status: status.exit_code()?, stdout: raw.utf8() ?? "", stderr: err.read_text()?, bytes: raw})
}

proc status_of(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Int] {
  Ok(applet_run(ctx, args)?.status)
}

# Each case is [expected status, ...words].
proc expect(ctx: TestContext, cases: List[List[Str]]) [fs, process, error] {
  for case in cases {
    let status = status_of(ctx, case[1..case.len()])?
    assert status == case[0].parse_int()?, f"test {case[1..case.len()].join(" ")}: {status}"
  }
}

proc fixtures(ctx: TestContext) [fs, error] -> Result[Path] {
  let root = test.temp_dir(ctx, name: "fixtures")?

  fp"{root}/regular".write("data")
  fp"{root}/empty".write("")
  fp"{root}/dir".mkdir()
  fs.symlink(fp"{root}/regular", fp"{root}/link")
  fs.symlink(fp"{root}/missing", fp"{root}/dangling")

  Ok(root)
}

test test_test_empty_and_single_word_forms { |ctx|
  expect(
    ctx,
    [
      ["1"],
      ["1", ""],
      ["0", "!"],
      ["0", "a string"],
      ["0", "-a"],
      ["0", "-o"],
      ["0", "-n"],
      ["0", "-z"],
      ["0", "("],
      ["0", "--help"],
      ["0", "--version"],
      ["1", "!", "!"],
      ["1", "!", "x"],
      ["0", "!", ""],
    ],
  )
}

test test_test_string_comparisons { |ctx|
  expect(
    ctx,
    [
      ["0", "", "=", ""],
      ["1", "", "!=", ""],
      ["0", "t", "==", "t"],
      ["1", "t", "==", "f"],
      ["0", "foo", "!=", "bar"],
      ["0", "(", "=", "("],
      ["0", "=", "=", "="],
      ["0", "123.45", "!=", "123.450"],
      ["0", "a", "<", "b"],
      ["1", "b", "<", "a"],
      ["0", "b", ">", "a"],
      ["1", "", "<", ""],
      ["0", "-n", "x"],
      ["1", "-n", ""],
      ["0", "-z", ""],
      ["1", "-z", "x"],
    ],
  )
}

test test_test_integer_comparisons_use_arbitrary_precision { |ctx|
  expect(
    ctx,
    [
      [
        "0",
        "0",
        "-eq",
        "0",
      ],
      [
        "0",
        "421",
        "-lt",
        "3720",
      ],
      [
        "0",
        "11",
        "-gt",
        "10",
      ],
      [
        "0",
        "1024",
        "-ge",
        "512",
      ],
      [
        "0",
        "-3720",
        "-lt",
        "-421",
      ],
      [
        "0",
        "-0",
        "-eq",
        "+0",
      ],
      [
        "0",
        "007",
        "-eq",
        "7",
      ],
      [
        "0",
        "42",
        "-eq",
        " 42 ",
      ],
      [
        "0",
        "9223372036854775808",
        "-gt",
        "0",
      ],
      [
        "0",
        "-170141183460469231731687303715884105729",
        "-lt",
        "0",
      ],
      [
        "0",
        "16267277278126277227728782172782882627278282882172762677623672762783783",
        "-gt",
        "16267277278126277227728782172782882627278282882172762677623672762783782",
      ],
      [
        "1",
        "5",
        "-lt",
        "05",
      ],
      [
        "0",
        "5",
        "-ne",
        "6",
      ],
      [
        "0",
        "5",
        "-le",
        "5",
      ],
    ],
  )
}

test test_test_length_operand { |ctx|
  expect(
    ctx,
    [
      ["0", "-l", "abc", "-eq", "3"],
      ["0", "3", "-eq", "-l", "abc"],
      ["0", "-l", "abc", "-ne", "4"],
      ["1", "-l", "abc", "-eq", "4"],
    ],
  )

  let result = applet_run(ctx, ["-l", "a", "-nt", "b"])?
  assert result.status == 2
  assert result.stderr == "test: -nt does not accept -l\n", result.stderr
}

test test_test_boolean_operators_and_parentheses { |ctx|
  expect(
    ctx,
    [
      ["0", "foo", "-o", ""],
      ["0", "foo", "-a", "bar"],
      ["1", "", "-a", "bar"],
      ["0", " ", "-o", "", "-a", ""],
      ["1", "(", " ", "-o", "", ")", "-a", ""],
      ["0", "(", "(", "a", "!=", "b", ")", "-o", "-n", "c", ")"],
      ["1", "!", "foo", "-o", "bar"],
      ["0", "!", "", "-o", "", "-a", ""],
      ["0", "!", "(", "", "-a", "", ")", "-o", ""],
      ["1", "!", "-n", "", "-a", ""],
      ["0", "(", "foo", ")"],
      ["1", "(", "-f", ")", ")"],
    ],
  )
}

test test_test_boolean_operators_do_not_short_circuit { |ctx|
  let result = applet_run(ctx, ["", "-a", "1", "-eq", "bad"])?
  assert result.status == 2
  assert result.stderr == "test: invalid integer 'bad'\n", result.stderr
  assert applet_run(ctx, ["x", "-o", "1", "-eq", "bad"])?.status == 2
}

test test_test_syntax_errors_exit_with_status_two { |ctx|
  for case in [
    {args: ["-a", "arg"], message: "test: '-a': unary operator expected\n"},
    {args: ["-Q", "x"], message: "test: '-Q': unary operator expected\n"},
    {args: ["x", "-Q", "y"], message: "test: '-Q': binary operator expected\n"},
    {args: ["missing_something", "="], message: "test: missing argument after '='\n"},
    {args: ["x", "-a"], message: "test: missing argument after '-a'\n"},
    {args: ["-n", "a", "-a"], message: "test: 'a': binary operator expected\n"},
    {args: ["(", "foo"], message: "test: missing argument after 'foo'\n"},
    {args: ["(", ")"], message: "test: missing argument after ')'\n"},
    {args: ["(", "a", "b", "c", "d", "e", ")"], message: "test: ')' expected, found 'b'\n"},
    {args: ["a", "!=", "(", "b", "-a", "b", ")", "!=", "c"], message: "test: extra argument 'b'\n"},
    {args: ["k", "!=", "m", "spare"], message: "test: extra argument 'spare'\n"},
    {args: ["7", "-eq", "zap"], message: "test: invalid integer 'zap'\n"},
    {args: ["123.45", "-ge", "6"], message: "test: invalid integer '123.45'\n"},
    {args: ["1_0", "-eq", "0"], message: "test: invalid integer '1_0'\n"},
  ] {
    let result = applet_run(ctx, case.args)?
    assert result.status == 2, case.args.join(" ")
    assert result.stdout == ""
    assert result.stderr == case.message, f"{case.args.join(" ")}: {result.stderr}"
  }
}

test test_test_file_type_and_size_operators { |ctx|
  let root = fixtures(ctx)?
  let regular = f"{root}/regular"

  expect(
    ctx,
    [
      ["0", "-e", regular],
      ["1", "-e", f"{root}/missing"],
      ["0", "-f", regular],
      ["1", "-f", f"{root}/dir"],
      ["0", "-d", f"{root}/dir"],
      ["1", "-d", regular],
      ["0", "-s", regular],
      ["1", "-s", f"{root}/empty"],
      ["1", "-s", f"{root}/missing"],
      ["0", "-h", f"{root}/link"],
      ["0", "-L", f"{root}/link"],
      ["1", "-h", regular],
      ["0", "-h", f"{root}/dangling"],
      ["1", "-e", f"{root}/dangling"],
      ["0", "-f", f"{root}/link"],
      ["0", "-c", "/dev/null"],
      ["1", "-b", "/dev/null"],
      ["1", "-p", regular],
      ["1", "-S", regular],
      ["1", "-e", ""],
      ["0", "!", "-e", f"{root}/missing"],
    ],
  )
}

test test_test_permission_and_ownership_operators { |ctx|
  let root = fixtures(ctx)?
  let regular = fp"{root}/regular"
  let me = unix.id()?

  regular.chmod(0o640)
  expect(
    ctx,
    [
      ["0", "-r", f"{regular}"],
      ["0", "-w", f"{regular}"],
      ["1", "-k", f"{regular}"],
      ["1", "-u", f"{regular}"],
      ["1", "-g", f"{regular}"],
      ["0", "-O", f"{regular}"],
      ["0", "-G", f"{regular}"],
      ["0", "-x", f"{root}/dir"],
      ["1", "-O", f"{root}/missing"],
      ["1", "-G", f"{root}/missing"],
    ],
  )

  regular.chmod(0o755)
  assert status_of(ctx, ["-x", f"{regular}"])? == 0
  regular.chmod(0o4755)
  assert status_of(ctx, ["-u", f"{regular}"])? == 0
  regular.chmod(0o2755)
  assert status_of(ctx, ["-g", f"{regular}"])? == 0
  regular.chmod(0o1755)
  assert status_of(ctx, ["-k", f"{regular}"])? == 0

  if me.euid != 0 {
    regular.chmod(0o000)
    assert status_of(ctx, ["-r", f"{regular}"])? == 1
    assert status_of(ctx, ["-w", f"{regular}"])? == 1
    assert status_of(ctx, ["-x", f"{regular}"])? == 1
  }
}

test test_test_modification_time_comparisons { |ctx|
  let root = fixtures(ctx)?
  let older = f"{root}/regular"
  let newer = f"{root}/newer"

  time.sleep(1100ms)
  fp"{root}/newer".write("later")

  expect(
    ctx,
    [
      ["0", newer, "-nt", older],
      ["1", older, "-nt", newer],
      ["0", older, "-ot", newer],
      ["1", newer, "-ot", older],
      ["0", older, "-nt", f"{root}/missing"],
      ["1", f"{root}/missing", "-nt", older],
      ["0", f"{root}/missing", "-ot", older],
      ["1", older, "-nt", older],
      ["1", older, "-ot", older],
    ],
  )
}

test test_test_same_file_comparison_follows_symlinks { |ctx|
  let root = fixtures(ctx)?
  let regular = f"{root}/regular"
  let hardlink = f"{root}/hardlink"
  let distinct = f"{root}/distinct"
  fs.link(fp"{regular}", fp"{hardlink}")?
  fp"{distinct}".write("data")

  expect(
    ctx,
    [
      ["0", regular, "-ef", regular],
      ["0", regular, "-ef", f"{root}/link"],
      ["0", regular, "-ef", hardlink],
      ["1", regular, "-ef", distinct],
      ["1", regular, "-ef", f"{root}/missing"],
      ["1", regular, "-ef", f"{root}/dangling"],
    ],
  )

  assert applet_run(ctx, ["-ef"])?.status == 0, "-ef alone is a plain string"
}

test test_test_terminal_descriptor_operator { |ctx|
  expect(
    ctx,
    [
      ["1", "-t", "99"],
      ["1", "-t", "-1"],
      ["1", "-t", "999999999999999999999999999999999999"],
      ["1", "-t", " 99 "],
      ["1", "-t", "0"],
    ],
  )

  let result = applet_run(ctx, ["-t", "abc"])?
  assert result.status == 2
  assert result.stderr == "test: invalid integer 'abc'\n", result.stderr
}

test test_test_help_and_version_are_plain_strings { |ctx|
  for word in ["--help", "--version"] {
    let result = applet_run(ctx, [word])?
    assert result.status == 0
    assert result.stdout == "" and result.stderr == ""
  }
}
