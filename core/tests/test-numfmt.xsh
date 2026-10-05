type Ran = {status: Int, stdout: Str, stderr: Str}

# Runs core/numfmt.xsh by its real path (so the invoked name is `numfmt` and
# `lib.gnu` resolves beside it) with `input` as standard input, capturing both
# streams to files.
proc numfmt_run(ctx: TestContext, args: List[Str], input: Str = "", vars: Record = {LC_ALL: "C"}) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "numfmt")?
  let stdin = test.temp_file(ctx, name: "numfmt-stdin", contents: bytes.from_text(input))?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/numfmt.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, vars, stdin, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

proc numfmt_text(ctx: TestContext, args: List[Str], input: Str = "") [fs, process, error] -> Result[Str] {
  let result = numfmt_run(ctx, args, input)?
  assert result.status == 0, f"numfmt {args.join(" ")}: {result.stderr}"
  Ok(result.stdout)
}

test test_numfmt_scales_down_to_units { |ctx|
  assert numfmt_text(ctx, ["--to=si"], "1000\n1100000\n100000000")? == "1.0k\n1.1M\n100M"
  assert numfmt_text(ctx, ["--to=iec"], "1024\n1153434\n107374182")? == "1.0K\n1.2M\n103M"
  assert numfmt_text(ctx, ["--to=iec-i"], "-1024\n-1153434")? == "-1.0Ki\n-1.2Mi"
  assert numfmt_text(ctx, ["--to=si", "999", "1000", "9999", "10000"])? == "999\n1.0k\n10k\n10k\n"
  assert numfmt_text(ctx, ["--to=si", "100000000000000000000000000000000"])? == "100Q\n", "the largest unit is Q"

  let too_big = numfmt_run(ctx, ["--to=si", "10000000000000000000000000000000000"])?
  assert too_big.status == 2
  assert too_big.stderr == "numfmt: Number is too big and unsupported\n", too_big.stderr
  assert numfmt_text(ctx, ["--to=iec", "--to-unit=689", "701"])? == "1\n"
}

test test_numfmt_scales_up_from_units { |ctx|
  assert numfmt_text(ctx, ["--from=si"], "1000\n1.1M\n0.1G")? == "1000\n1100000\n100000000"
  assert numfmt_text(ctx, ["--from=iec"], "1024\n1.1M\n0.1G")? == "1024\n1153434\n107374183"
  assert numfmt_text(ctx, ["--from=iec-i", "1.1Mi", "1024"])? == "1153434\n1024\n"
  assert numfmt_text(ctx, ["--from=auto"], "1K\n1Ki")? == "1000\n1024"
  assert numfmt_text(ctx, ["--from=si", "--to=iec", "15334263563K"])? == "14T\n"
  assert numfmt_text(ctx, ["--from=si", "--to=si"], "10000000K\n0.001K")? == "10G\n1"
  assert numfmt_text(ctx, ["--from-unit=512", "4"])? == "2048\n"
  assert numfmt_text(ctx, ["--to-unit=512", "2048"])? == "4\n"
  assert numfmt_text(ctx, ["--from=iec", "9153396227555392131"])? == "9153396227555392131\n", "integers keep every digit"

  let huge = numfmt_run(ctx, ["123456789012345678901234567890"])?
  assert huge.status == 2
  assert huge.stderr == "numfmt: value/precision too large to be printed: '1.23457e+29/0' (consider using --to)\n", huge.stderr
}

test test_numfmt_rounding_methods { |ctx|
  for case in [
    {method: "from-zero", expected: "9.1k\n-9.1k\n9.1k\n-9.1k\n"},
    {method: "towards-zero", expected: "9.0k\n-9.0k\n9.0k\n-9.0k\n"},
    {method: "up", expected: "9.1k\n-9.0k\n9.1k\n-9.0k\n"},
    {method: "down", expected: "9.0k\n-9.1k\n9.0k\n-9.1k\n"},
    {method: "nearest", expected: "9.0k\n-9.0k\n9.1k\n-9.1k\n"},
    {method: "n", expected: "9.0k\n-9.0k\n9.1k\n-9.1k\n"},
  ] {
    assert numfmt_text(ctx, ["--to=si", f"--round={case.method}", "--", "9001", "-9001", "9099", "-9099"])? == case.expected, case.method
  }

  assert numfmt_text(ctx, ["--to=si", "--", "0.4", "0.5", "0.6", "1.4", "3.14", "-0.4", "-0.6"])? == "0\n0\n1\n1\n3\n-0\n-1\n"
}

test test_numfmt_padding_and_suffix { |ctx|
  assert numfmt_text(ctx, ["--from=si", "--padding=8"], "1K\n1.1M\n0.1G")? == "    1000\n 1100000\n100000000"
  assert numfmt_text(ctx, ["--from=si", "--padding", "-8"], "1K\n1.1M")? == "1000    \n1100000 ", "a negative padding left-aligns"
  assert numfmt_text(ctx, ["--from=auto"], "   1K\n        2K")? == " 1000\n      2000", "leading blanks imply a padding"
  assert numfmt_text(ctx, ["--suffix=TEST"], "1000")? == "1000TEST"
  assert numfmt_text(ctx, ["--suffix=TEST"], "1000TEST")? == "1000TEST"
  assert numfmt_text(ctx, ["--suffix=b", "--to=si"], "2000b")? == "2.0kb"
  assert numfmt_text(ctx, ["--suffix=pad", "--padding=12"], "1000 2000 3000")? == "     1000pad 2000 3000"
  assert numfmt_text(ctx, ["--to=si", "--unit-separator= ", "1000", "500"])? == "1.0 k\n500\n"
  assert numfmt_text(ctx, ["--to=si", "--unit-separator", "-", "1000"])? == "1.0-k\n"
}

test test_numfmt_fields_and_delimiters { |ctx|
  assert numfmt_text(ctx, ["--from=auto", "--field", "3", "1K 2K 3K"])? == "1K 2K 3000\n"
  assert numfmt_text(ctx, ["--from=auto", "--field", "-2,4", "1K 2K 3K 4K 5K"])? == "1000 2000 3K 4000 5K\n"
  assert numfmt_text(ctx, ["--from=auto", "--field", "2-4", "1K 2K 3K 4K 5K"])? == "1K 2000 3000 4000 5K\n"
  assert numfmt_text(ctx, ["--from=auto", "--field", "1,-,3", "1K 2K"])? == "1000 2000\n", "a lone dash selects every field"
  assert numfmt_text(ctx, ["--from=auto", "--field", "9", "1K 2K"])? == "1K 2K\n"
  assert numfmt_text(ctx, ["--from=auto"], "1Ki 2M 3G")? == "1024 2M 3G", "only the first field converts by default"
  assert numfmt_text(ctx, ["-d,", "--to=iec"], "123456")? == "121K"
  assert numfmt_text(ctx, ["-d|", "--to=si", "--padding=5", "--field=-"], "1000|2000")? == " 1.0k| 2.0k"
  assert numfmt_text(ctx, ["-d|", "--to=si"], "             1000|   2000")? == "1.0k|   2000", "unselected fields keep their blanks"
  assert numfmt_text(ctx, ["--field", "2", "1\u{3000}2"])? == "1 2\n", "a multibyte blank separator is normalized to a space"
  assert numfmt_text(ctx, ["--header=2", "--from=si"], "a\nb\n1K\n2K")? == "a\nb\n1000\n2000"
  assert numfmt_text(ctx, ["--header", "--from=si"], "head\n1K")? == "head\n1000"
  assert numfmt_text(ctx, ["-z", "--to=si"], "1000\u{0}2000\u{0}")? == "1.0k\u{0}2.0k\u{0}"
}

test test_numfmt_format { |ctx|
  assert numfmt_text(ctx, ["--format=--%6f--", "50"])? == "--    50--\n"
  assert numfmt_text(ctx, ["--format=--%-6f--", "50"])? == "--50    --\n"
  assert numfmt_text(ctx, ["--format=%06f", "1234"])? == "001234\n"
  assert numfmt_text(ctx, ["--format=%05.1f", "--to=si", "--suffix=B", "1234567"])? == "001.3MB\n", "units and suffix stay outside the zero padding"
  assert numfmt_text(ctx, ["--format=%018.2f", "--from=none", "--", "-9869647"])? == "-00000009869647.00\n"
  assert numfmt_text(ctx, ["--format=%.2f", "1", "0.99", "1.01"])? == "1.00\n0.99\n1.01\n"
  assert numfmt_text(ctx, ["--format=%.1f", "--round=down", "0.99"])? == "0.9\n"
  assert numfmt_text(ctx, ["--format=%.4f", "--to=si", "9991239123"])? == "9.9913G\n"
  assert numfmt_text(ctx, ["--to=iec", "--format=%.5f", "1500"])? == "1.46500K\n", "iec stops rounding at three decimals"
  assert numfmt_text(ctx, ["--to=si", "--format=%.5f", "1234567"])? == "1.23457M\n"
  assert numfmt_text(ctx, ["--suffix=€", "--format=%10.2f", "692"])? == "   692.00€\n"
  assert numfmt_text(ctx, ["--format=%.18f", "0"])? == "0.000000000000000000\n"

  let large = numfmt_run(ctx, ["--format=%5.1f", "1000000000000000000"])?
  assert large.status == 2
  assert large.stderr == "numfmt: value/precision too large to be printed: '1e+18/1' (consider using --to)\n", large.stderr
}

test test_numfmt_format_errors { |ctx|
  for case in [
    {format: "hello", message: "format 'hello' has no % directive"},
    {format: "hello%", message: "format 'hello%' ends in %"},
    {format: "%f %f", message: "format '%f %f' has too many % directives"},
    {format: "%d", message: "invalid format '%d', directive must be %[0]['][-][N][.][N]f"},
    {format: "%18446744073709551616f", message: "invalid format '%18446744073709551616f' (width overflow)"},
    {format: "%.-1f", message: "invalid precision in format '%.-1f'"},
    {format: "a\nb%f%", message: "format 'a\\nb%f%' has too many % directives"},
  ] {
    let result = numfmt_run(ctx, [f"--format={case.format}"])?
    assert result.status == 1, case.format
    assert result.stderr == f"numfmt: {case.message}\n", result.stderr
  }

  let grouping = numfmt_run(ctx, ["--format=%'f", "--to=si"])?
  assert grouping.status == 1
  assert grouping.stderr == "numfmt: grouping cannot be combined with --to\n", grouping.stderr

  let both = numfmt_run(ctx, ["--format=%f", "--grouping"])?
  assert both.stderr == "numfmt: --grouping cannot be combined with --format\n", both.stderr
}

test test_numfmt_input_errors_and_invalid_modes { |ctx|
  for case in [
    {input: "1Kx", message: "invalid suffix in input '1Kx': 'x'", unit: "si"},
    {input: "1x0K", message: "invalid suffix in input: '1x0K'", unit: "auto"},
    {input: "+5", message: "invalid number: '+5'", unit: "auto"},
    {input: "1e-3", message: "invalid suffix in input: '1e-3'", unit: "auto"},
    {input: "NaN", message: "invalid number: 'NaN'", unit: "auto"},
    {input: "10M", message: "missing 'i' suffix in input: '10M' (e.g Ki/Mi/Gi)", unit: "iec-i"},
    {input: "10Mi", message: "invalid suffix in input '10Mi': 'i'", unit: "iec"},
    {input: "4Q", message: "rejecting suffix in input: '4Q' (consider using --from)", unit: "none"},
    {input: "12.K", message: "invalid number: '12.K'", unit: "si"},
  ] {
    let result = numfmt_run(ctx, [f"--from={case.unit}", case.input])?
    assert result.status == 2, case.input
    assert result.stdout == ""
    assert result.stderr == f"numfmt: {case.message}\n", result.stderr
  }

  let blank = numfmt_run(ctx, ["--from=auto"], "  \t \n")?
  assert blank.status == 2
  assert blank.stderr == "numfmt: invalid number: ''\n", blank.stderr

  let abort = numfmt_run(ctx, ["--field=3", "--from=auto", "Hello 40M World 90G"])?
  assert abort.status == 2
  assert abort.stdout == "Hello 40M ", "an aborted line keeps what was converted before the error"

  let fail = numfmt_run(ctx, ["--invalid=fail", "--debug", "--to=si", "1000", "Foo", "3000"])?
  assert fail.status == 2
  assert fail.stdout == "1.0k\nFoo\n3.0k\n"
  assert fail.stderr == "numfmt: invalid number: 'Foo'\nnumfmt: failed to convert some of the input numbers\n", fail.stderr

  let warn = numfmt_run(ctx, ["--invalid=warn", "100", "1e5", "200"])?
  assert warn.status == 0
  assert warn.stdout == "100\n1e5\n200\n"
  assert warn.stderr == "numfmt: invalid suffix in input: '1e5'\n", warn.stderr

  let ignore = numfmt_run(ctx, ["--invalid=ignore", "4Q"])?
  assert ignore.status == 0
  assert ignore.stdout == "4Q\n"
  assert ignore.stderr == ""
}

test test_numfmt_option_errors_are_gnu_usage_errors { |ctx|
  for case in [
    {args: ["--header=0"], message: "invalid header value '0'"},
    {args: ["--padding=0", "5"], message: "invalid padding value '0'"},
    {args: ["--from-unit=0"], message: "invalid unit size: '0'"},
    {args: ["--to-unit=18446744073709551616"], message: "invalid unit size: '18446744073709551616'"},
    {args: ["--delimiter", "sad"], message: "the delimiter must be a single character"},
    {args: ["--to=auto", "100"], message: "invalid argument 'auto' for '--to'"},
    {args: ["--from=xyz", "100"], message: "invalid argument 'xyz' for '--from'"},
    {args: ["--field=0", "1"], message: "range '0' was invalid: fields and positions are numbered from 1"},
  ] {
    let result = numfmt_run(ctx, case.args)?
    assert result.status == 1, f"{case.args.join(" ")}"
    assert result.stderr == f"numfmt: {case.message}\n", result.stderr
  }

  let round = numfmt_run(ctx, ["--round=sideways", "1"])?
  assert round.status == 1
  assert round.stderr.starts_with("numfmt: invalid argument 'sideways' for '--round'\nValid arguments are:\n  - 'up'\n"), round.stderr

  let negative = numfmt_run(ctx, ["--to=iec", "-8765432"])?
  assert negative.status == 1
  assert negative.stderr == "numfmt: invalid option -- '8'\nTry 'numfmt --help' for more information.\n", negative.stderr
  assert numfmt_text(ctx, ["--to=iec", "--", "-8765432"])? == "-8.4M\n"
}

test test_numfmt_locale_decimal_separator { |ctx|
  assert numfmt_run(ctx, ["--to=iec", "1500"], "", {LC_ALL: "fr_FR.UTF-8"})?.stdout == "1,5K\n"
  assert numfmt_run(ctx, ["--format=%.3f", "1,5"], "", {LC_ALL: "fr_FR.UTF-8"})?.stdout == "1,500\n"
  assert numfmt_run(ctx, ["--format=%.3f", "1.5"], "", {LC_ALL: "fr_FR.UTF-8"})?.status == 2
  assert numfmt_run(ctx, ["--to=iec", "1500"], "", {LC_ALL: "C"})?.stdout == "1.5K\n"
  assert numfmt_run(ctx, ["--grouping", "1234567"], "", {LC_ALL: "en_US.UTF-8"})?.stdout == "1,234,567\n"
}

test test_numfmt_debug_warnings { |ctx|
  let plain = numfmt_run(ctx, ["--debug", "4096"])?
  assert plain.stdout == "4096\n"
  assert plain.stderr == "numfmt: no conversion option specified\n", plain.stderr

  let header = numfmt_run(ctx, ["--debug", "--header", "--to=iec", "4096"])?
  assert header.stdout == "4.0K\n"
  assert header.stderr == "numfmt: --header ignored with command-line input\n", header.stderr

  let grouping = numfmt_run(ctx, ["--debug", "--grouping", "--from=si", "4.0K"])?
  assert grouping.stderr == "numfmt: grouping has no effect in this locale\n", grouping.stderr
}

test test_numfmt_help_and_version { |ctx|
  let help = numfmt_run(ctx, ["--help"])?
  assert help.status == 0
  assert "Usage: numfmt [OPTION]... [NUMBER]..." in help.stdout

  let version = numfmt_run(ctx, ["--version"])?
  assert version.stdout.starts_with("numfmt")
}
