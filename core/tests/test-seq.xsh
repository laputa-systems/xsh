type Ran = {status: Int, stdout: Str, stderr: Str}

# Runs core/seq.xsh by its real path (so the invoked name is `seq` and
# `lib.gnu` resolves beside it), capturing both streams to files.
proc seq_run(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "seq")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/seq.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

proc seq_out(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Str] {
  let result = seq_run(ctx, args)?
  assert result.status == 0, f"seq {args.join(" ")}: status {result.status}: {result.stderr}"
  Ok(result.stdout)
}

test test_seq_integers_and_directions { |ctx|
  assert seq_out(ctx, ["3"])? == "1\n2\n3\n"
  assert seq_out(ctx, ["2", "4"])? == "2\n3\n4\n"
  assert seq_out(ctx, ["2", "2", "6"])? == "2\n4\n6\n"
  assert seq_out(ctx, ["5", "-2", "-1"])? == "5\n3\n1\n-1\n"
  assert seq_out(ctx, ["3", "1"])? == "", "an omitted increment is 1 even when LAST is smaller"
  assert seq_out(ctx, ["-1", "1"])? == "-1\n0\n1\n", "a negative FIRST is a number, not an option"
  assert seq_out(ctx, ["-s,", "-1", "2"])? == "-1,0,1,2\n"
  assert seq_out(ctx, ["-s", "-1", "2"])? == "1-12\n", "an option value may start with a dash"
}

test test_seq_separator_and_terminator { |ctx|
  assert seq_out(ctx, ["-s", ",", "2", "6"])? == "2,3,4,5,6\n"
  assert seq_out(ctx, ["-s", ",", "-t", "!", "2", "6"])? == "2,3,4,5,6!"
  assert seq_out(ctx, ["--separator=", "2", "6"])? == "23456\n"
  assert seq_out(ctx, ["-s", "\\n", "2", "4"])? == "2\\n3\\n4\n", "the separator is taken literally"
  assert seq_out(ctx, ["--terminator=END", "2"])? == "1\n2END"
}

test test_seq_equal_width { |ctx|
  assert seq_out(ctx, ["-w", "8", "10"])? == "08\n09\n10\n"
  assert seq_out(ctx, ["--equal-width", "999", "1e3"])? == "0999\n1000\n"
  assert seq_out(ctx, ["-w", "-0", "1"])? == "-0\n01\n"
  assert seq_out(ctx, ["-w", "-1e-3", "1"])? == "-0.001\n00.999\n"
  assert seq_out(ctx, ["-w", "-.1e2", "10", "30"])? == "-010\n0000\n0010\n0020\n0030\n"
  assert seq_out(ctx, ["-w", "0e15", "1"])? == "0000000000000000\n0000000000000001\n"
  assert seq_out(ctx, ["-w", "9.0", "10.0"])? == "09.0\n10.0\n"
  assert seq_out(ctx, ["-w", "0x1", "5.2", "10.0000"])? == "01.0\n06.2\n", "the last operand's integral digits widen the output"
  assert seq_out(ctx, ["-w", "0x1.0000", "5.2", "10"])? == "1\n6.2\n", "a hex float turns the default format into %g"
}

test test_seq_decimal_precision_follows_first_and_increment { |ctx|
  assert seq_out(ctx, ["10.0"])? == "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n", "only FIRST and INCREMENT set the precision"
  assert seq_out(ctx, ["5", "-1.0", "1"])? == "5.0\n4.0\n3.0\n2.0\n1.0\n"
  assert seq_out(ctx, ["1", "1.2", "3"])? == "1.0\n2.2\n"
  assert seq_out(ctx, ["999", "0.1", "1000.1"])? == "999.0\n999.1\n999.2\n999.3\n999.4\n999.5\n999.6\n999.7\n999.8\n999.9\n1000.0\n1000.1\n"
  assert seq_out(ctx, ["0.1", "-0.1", "-0.2"])? == "0.1\n0.0\n-0.1\n-0.2\n"
  assert seq_out(ctx, [".64999", "1e-7", ".6499901"])? == "0.6499900\n0.6499901\n"
  assert seq_out(ctx, ["1", "-1", "0.1"])? == "1\n"
}

test test_seq_negative_zero_and_tiny_exponents { |ctx|
  assert seq_out(ctx, ["-0", "1"])? == "-0\n1\n"
  assert seq_out(ctx, ["-0", "0.1", "0.1"])? == "-0.0\n0.1\n"
  assert seq_out(ctx, ["1", "-1", "-0"])? == "1\n0\n"
  assert seq_out(ctx, ["1e-9223372036854775808"])? == "", "a value that underflows is zero"
  assert seq_out(ctx, ["-1e-922337203685477580800000000", "1"])? == "-0\n1\n"
}

test test_seq_arbitrary_precision_and_hex { |ctx|
  assert seq_out(ctx, ["1000000000000000000000000000", "1000000000000000000000000001"])? == "1000000000000000000000000000\n1000000000000000000000000001\n"
  assert seq_out(ctx, ["0xa", "0XA"])? == "10\n"
  assert seq_out(ctx, ["0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF", "0x100000000000000000000000000000000"])? == "340282366920938463463374607431768211455\n340282366920938463463374607431768211456\n"
  assert seq_out(ctx, ["0x1p-1", "2"])? == "0.5\n1.5\n"
  assert seq_out(ctx, ["0xffff.4p-4", "4096"])? == "4095.95\n"
  assert seq_out(ctx, ["  0xee.", "  0xef."])? == "238\n239\n"
  assert seq_out(ctx, ["   1"])? == "1\n", "leading whitespace is accepted"
}

test test_seq_format { |ctx|
  assert seq_out(ctx, ["-f", "%.2f", "0.0", "0.1", "0.3"])? == "0.00\n0.10\n0.20\n0.30\n"
  assert seq_out(ctx, ["--format=%g", "0", "0.987654321", "2"])? == "0\n0.987654\n1.97531\n"
  assert seq_out(ctx, ["-f", "%E", "0", "0.7", "2"])? == "0.000000E+00\n7.000000E-01\n1.400000E+00\n"
  assert seq_out(ctx, ["-f", "%010g", "1e5", "1e5"])? == "0000100000\n"
  assert seq_out(ctx, ["-f", "%010g", "1e6", "1e6"])? == "000001e+06\n"
  assert seq_out(ctx, ["-f", "%.2g", "10", "10"])? == "10\n"
  assert seq_out(ctx, ["-f", "<%5.1f|%%>", "1", "2"])? == "<  1.0|%>\n<  2.0|%>\n"
  assert seq_out(ctx, ["-f", "%-6.1f|", "1"])? == "1.0   |\n"
  assert seq_out(ctx, ["-f", "%+.0f", "2"])? == "+1\n+2\n"
  assert seq_out(ctx, ["-f", "%.0f", "0.5", "1", "2.5"])? == "0\n2\n2\n", "ties round to even"
  assert seq_out(ctx, ["-f", "%.66000f", "4", "4"])?.byte_len() == 66003
}

test test_seq_format_errors { |ctx|
  for case in [
    {format: "%%g", message: "format '%%g' has no % directive"},
    {format: "%g%g", message: "format '%g%g' has too many % directives"},
    {format: "%g%", message: "format '%g%' has too many % directives"},
    {format: "%", message: "format '%' ends in %"},
    {format: "%5.2c", message: "format '%5.2c' has unknown %c directive"},
    {format: "%a", message: "format '%a': the %a conversion is not supported"},
  ] {
    let result = seq_run(ctx, ["-f", case.format, "1"])?
    assert result.status == 1
    assert result.stdout == ""
    assert result.stderr == f"seq: {case.message}\n", result.stderr
  }

  let both = seq_run(ctx, ["-w", "-f", "%f", "1"])?
  assert both.status == 1
  assert both.stderr == "seq: format string may not be specified when printing equal width strings\nTry 'seq --help' for more information.\n", both.stderr
}

test test_seq_argument_errors_are_gnu_usage_errors { |ctx|
  for case in [
    {args: ["foo"], message: "invalid floating point argument: 'foo'"},
    {args: ["1e2.3", "2"], message: "invalid floating point argument: '1e2.3'"},
    {args: ["1 "], message: "invalid floating point argument: '1 '"},
    {args: ["0x-123ABC"], message: "invalid floating point argument: '0x-123ABC'"},
    {args: ["NaN"], message: "invalid 'not-a-number' argument: 'NaN'"},
    {args: ["1", "0", "5"], message: "invalid Zero increment value: '0'"},
    {args: ["1e9223372036854775807"], message: "invalid floating point argument: '1e9223372036854775807'"},
    {args: [], message: "missing operand"},
    {args: ["1", "2", "3", "4"], message: "extra operand '4'"},
  ] {
    let result = seq_run(ctx, case.args)?
    assert result.status == 1, f"seq {case.args.join(" ")}"
    assert result.stdout == ""
    assert result.stderr == f"seq: {case.message}\nTry 'seq --help' for more information.\n", result.stderr
  }
}

test test_seq_option_errors_and_endless_sequences_fail_explicitly { |ctx|
  let unknown = seq_run(ctx, ["--definitely-invalid"])?
  assert unknown.status == 1
  assert unknown.stderr == "seq: unrecognized option '--definitely-invalid'\nTry 'seq --help' for more information.\n", unknown.stderr

  let endless = seq_run(ctx, ["inf"])?
  assert endless.status == 1
  assert endless.stdout == ""
  assert "endless sequence" in endless.stderr
}

test test_seq_reports_stdout_errors_before_expanding_the_remaining_numbers { |ctx|
  let root = test.temp_dir(ctx, name: "seq-write-error")?
  let stderr = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/seq.xsh".display(), "1", "0.0001", "99999999"]
  let plan = process.command_argv(
    ctx.xsh_bin,
    argv,
    root,
    {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"},
    b"",
    /dev/full,
    stderr,
    timeout: 2s,
  )
  let status = process.run(plan)?

  assert status.exit_code()? == 1
  assert stderr.read_text()? == "seq: write error: No space left on device\n"
}

test test_seq_help_and_version { |ctx|
  let help = seq_run(ctx, ["--help"])?
  assert help.status == 0
  assert help.stderr == ""
  assert "Usage: seq [OPTION]... LAST" in help.stdout

  let version = seq_run(ctx, ["--version"])?
  assert version.status == 0
  assert version.stdout.starts_with("seq")
}
