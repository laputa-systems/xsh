use support.uu

proc output_is(s: uu.Scene, args: List[Str], output: Str) [fs, process, env, error] {
  let r = uu.invoke(s, "seq", args, timeout: 10s)?
  uu.succeeds(r)
  uu.stdout_only(r, output)
}

# The shell owns the pipe or device descriptor, preserving the producer's
# stream lifecycle while the helper supplies the oracle's executable argv.
proc redirected(s: uu.Scene, args: List[Str], setup: Str) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let launch = uu.argv(s, "seq", [Path(arg) for arg in args])?
  let words = [p"/bin/sh", p"-c", Path(setup), p"seq-stream"].extend(launch)
  let out = uu.at(s, ".redirect-out")
  let err = uu.at(s, ".redirect-err")
  let status = process.run(process.command_argv(p"/bin/sh", words, s.root, {}, b"", out, err, timeout: 10s))?
  Ok({util: "seq", args: args, status: status.shell_code()?, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}

# origin: gnu seq/seq-extra-number.log
test test_gnu_seq_seq_extra_number_log { |ctx|
  let s = uu.scene(ctx)?
  output_is(s, ["0", "0.000001", "0.000003"], "0.000000\n0.000001\n0.000002\n0.000003\n")
  output_is(s, ["-f", "%g=", "1000000", "1000000"], "1e+06=\n")
}

# origin: gnu seq/seq-io-errors.log
test test_gnu_seq_seq_io_errors_log { |ctx|
  let s = uu.scene(ctx)?
  for args in [["1", "inf"], ["1.1", ".1", "inf"], ["1", "0.0001", "99999999"]] {
    uu.fails_with_code(redirected(s, args, r"""exec "$@" >/dev/full 2>/dev/null; """)?, 1)
  }
}

# origin: gnu seq/seq-locale.log
test test_gnu_seq_seq_locale_log { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "seq", ["0.1", "0.2", "0.7"], vars: {LC_ALL: "", LANG: "invalid", LC_NUMERIC: "fr_FR.UTF-8"})?
  uu.succeeds(r)
  let lines = r.stdout.utf8()?.lines()
  assert ! lines.is_empty()
  for line in lines { assert line[0..2] == lines[0][0..2] }
}

# origin: gnu seq/seq-precision.log
test test_gnu_seq_seq_precision_log { |ctx|
  let s = uu.scene(ctx)?
  let first_two = r""""$@" | head -n2; """
  let integral = redirected(s, ["999999", "inf"], first_two)?
  uu.succeeds(integral)
  uu.stdout_is(integral, "999999\n1000000\n")
  for digits in range(1, 101) {
    let nines = ["9"] |> repeat(digits) |> join("")
    let next = "1" + (["0"] |> repeat(digits)).join("")
    output_is(s, [nines, next], f"{nines}\n{next}\n")
  }
  output_is(s, ["0xF423F", "0xF4240"], "999999\n1000000\n")
  let decimal_inf = redirected(s, ["1", ".1", "inf"], first_two)?
  uu.succeeds(decimal_inf)
  uu.stdout_is(decimal_inf, "1.0\n1.1\n")
  let infinite_start = redirected(s, ["inf", "inf"], r""""$@" | head -n2 | uniq; """)?
  uu.succeeds(infinite_start)
  assert infinite_start.stdout.utf8()?.count_lines() == 1
  output_is(s, ["1", "0x1p-1", "2"], "1\n1.5\n2\n")
  let decimal_hex = redirected(s, ["1", ".1", "0x2"], first_two)?
  uu.succeeds(decimal_hex)
  uu.stdout_is(decimal_hex, "1.0\n1.1\n")
  output_is(s, ["1.1e1", "12"], "11\n12\n")
  output_is(s, ["11", "1.2e1"], "11\n12\n")
  let padded = redirected(s, ["-w", "1.1e4"], r""""$@" | head -n1; """)?
  uu.succeeds(padded)
  uu.stdout_is(padded, "00001\n")
  output_is(s, ["-w", "1.10000e5", "1.10000e5"], "110000\n")
  let underflow = uu.invoke(s, "seq", ["1e-9223372036854775808"], timeout: 10s)?
  uu.succeeds(underflow)
  uu.no_stderr(underflow)
  let precision = uu.invoke(s, "seq", ["-f", "%.70000f", "1"], timeout: 10s)?
  uu.succeeds(precision)
  assert precision.stdout.len() == 70003
  assert precision.stdout.utf8()?.replace("0", with: "") == "1.\n"
}

# origin: gnu seq/seq.log
test test_gnu_seq_seq_log { |ctx|
  let s = uu.scene(ctx)?
  for row in [
    {args: ["10"], output: "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n"},
    {args: ["-1"], output: ""},
    {args: ["1", "-1", "3"], output: ""},
    {args: ["-10", "10", "10"], output: "-10\n0\n10\n"},
    {args: ["1", "-1", "0"], output: "1\n0\n"},
    {args: ["1", "-1", "-1"], output: "1\n0\n-1\n"},
    {args: ["0.8", "0.1", "0.9"], output: "0.8\n0.9\n"},
    {args: ["0.1", "0.99", "1.99"], output: "0.10\n1.09\n"},
    {args: ["10.8", "0.1", "10.95"], output: "10.8\n10.9\n"},
    {args: ["0.8", "1e-1", "0.9"], output: "0.8\n0.9\n"},
    {args: ["0.8", "0.1", "0.9000000000000"], output: "0.8\n0.9\n"},
    {args: [".8", "1e-2", ".81"], output: "0.80\n0.81\n"},
    {args: [".89999", "1e-7", ".8999901"], output: "0.8999900\n0.8999901\n"},
    {args: ["-w", "1", "-1", "-1"], output: "01\n00\n-1\n"},
    {args: ["-w", "-.1", ".1", ".11"], output: "-0.1\n00.0\n00.1\n"},
    {args: ["-w", "1", "3.0"], output: "1\n2\n3\n"},
    {args: ["-w", ".8", "1e-2", ".81"], output: "0.80\n0.81\n"},
    {args: ["-w", "1", ".5", "2"], output: "1.0\n1.5\n2.0\n"},
    {args: ["-w", "+1", "2"], output: "1\n2\n"},
    {args: ["-w", "    .1", "    .1"], output: "0.1\n"},
    {args: ["-w", "9", "0.5", "10"], output: "09.0\n09.5\n10.0\n"},
    {args: ["-w", "-1e-3", "1"], output: "-0.001\n00.999\n"},
    {args: ["-w", "-1e-003", "1"], output: "-0.001\n00.999\n"},
    {args: ["-w", "-1.e-3", "1"], output: "-0.001\n00.999\n"},
    {args: ["-w", "-1.0e-4", "1"], output: "-0.00010\n00.99990\n"},
    {args: ["-w", "999", "1e3"], output: "0999\n1000\n"},
    {args: ["-w", "-1", "1.0", "0"], output: "-1.0\n00.0\n"},
    {args: ["-w", "10", "-.1", "9.9"], output: "10.0\n09.9\n"},
    {args: ["-f", "%2.1f", "1.5", ".5", "2"], output: "1.5\n2.0\n"},
    {args: ["-f", "%0.1f", "1.5", ".5", "2"], output: "1.5\n2.0\n"},
    {args: ["-f", "%.1f", "1.5", ".5", "2"], output: "1.5\n2.0\n"},
    {args: ["-f", "%3.0f", "1", "2"], output: "  1\n  2\n"},
    {args: ["-f", "%-3.0f", "1", "2"], output: "1  \n2  \n"},
    {args: ["-f", "%+3.0f", "1", "2"], output: " +1\n +2\n"},
    {args: ["-f", "%0+3.0f", "1", "2"], output: "+01\n+02\n"},
    {args: ["-f", "%0+.0f", "1", "2"], output: "+1\n+2\n"},
    {args: ["-f", "% -3.0f", "-1", "0"], output: "-1 \n 0 \n"},
    {args: ["-f", "% -.0f", "-1", "0"], output: "-1\n 0\n"},
    {args: ["-f", "%%%g%%", "1"], output: "%1%\n"},
    {args: ["000", "2"], output: "0\n1\n2\n"},
    {args: ["000", "02"], output: "0\n1\n2\n"},
    {args: ["00", "02"], output: "0\n1\n2\n"},
    {args: ["0", "02"], output: "0\n1\n2\n"},
    {args: ["-s,", "1", "3"], output: "1,2,3\n"},
    {args: ["-s,", "1", "1"], output: "1\n"},
    {args: ["-s,,", "1", "3"], output: "1,,2,,3\n"},
    {args: ["1", "3", "1"], output: "1\n"},
    {args: ["1", "1", "4.2"], output: "1\n2\n3\n4\n"},
    {args: ["1", "1", "0"], output: ""},
    {args: ["-0", "10"], output: "-0\n1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n"},
    {args: ["1", "-0"], output: ""},
    {args: ["4"], output: "1\n2\n3\n4\n"},
    {args: ["1", "4"], output: "1\n2\n3\n4\n"},
    {args: ["1", "1", "4"], output: "1\n2\n3\n4\n"},
    {args: ["1", "2", "4"], output: "1\n3\n"},
    {args: ["1", "4", "4"], output: "1\n"},
    {args: ["1", "1e0", "4"], output: "1\n2\n3\n4\n"},
  ] { output_is(s, row.args, row.output) }
  let crossing_zero = uu.invoke(s, "seq", ["0.1", "-0.1", "-0.2"])?
  uu.succeeds(crossing_zero)
  uu.no_stderr(crossing_zero)
  assert crossing_zero.stdout.utf8()?.replace("-0.0\n", with: "0.0\n") == "0.1\n0.0\n-0.1\n-0.2\n"
  let french = uu.invoke(s, "seq", ["-0.1", "0.1", "2"], vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.succeeds(french)
  uu.no_stderr(french)
  assert french.stdout.utf8()?.replace(",", with: ".") == "-0.1\n0.0\n0.1\n0.2\n0.3\n0.4\n0.5\n0.6\n0.7\n0.8\n0.9\n1.0\n1.1\n1.2\n1.3\n1.4\n1.5\n1.6\n1.7\n1.8\n1.9\n2.0\n"
  let nines = ["9"] |> repeat(81) |> join("")
  let next = "1" + (["0"] |> repeat(81)).join("")
  let last = "1" + (["0"] |> repeat(80)).join("") + "1"
  output_is(s, [nines, last], f"{nines}\n{next}\n{last}\n")
  for row in [
    {args: ["-f", "%%g", "1"], error: "seq: format '%%g' has no % directive\n"},
    {args: ["-f", "%", "1"], error: "seq: format '%' ends in %\n"},
    {args: ["-f", "%g%", "1"], error: "seq: format '%g%' has too many % directives\n"},
    {args: ["-f", "", "1"], error: "seq: format '' has no % directive\n"},
    {args: ["-f", "%g%g", "1"], error: "seq: format '%g%g' has too many % directives\n"},
  ] {
    let r = uu.invoke(s, "seq", row.args)?
    uu.fails_with_code(r, 1)
    uu.stderr_only(r, row.error)
  }
  let hint = "Try 'seq --help' for more information.\n"
  for args in [["1", "0", "10"], ["0", "-0", "0"], ["1", "0.0", "10"], ["1", "-0.0e-10", "10"]] {
    let r = uu.invoke(s, "seq", args)?
    uu.fails_with_code(r, 1)
    uu.no_stdout(r)
    assert r.stderr.utf8()?.replace(args[1], with: "0") == f"seq: invalid Zero increment value: '0'\n{hint}"
  }
  for row in [
    {args: ["nan"], word: "nan"}, {args: ["NaN", "2"], word: "NaN"},
    {args: ["nan", "1", "2"], word: "nan"}, {args: ["--", "-nan"], word: "-nan"},
    {args: ["1", "nan", "2"], word: "nan"}, {args: ["1", "-NaN", "2"], word: "-NaN"},
    {args: ["1", "1", "nan"], word: "nan"}, {args: ["1", "NaN"], word: "NaN"},
    {args: ["0", "-1", "-NaN"], word: "-NaN"},
  ] {
    let r = uu.invoke(s, "seq", row.args)?
    uu.fails_with_code(r, 1)
    uu.no_stdout(r)
    assert r.stderr.utf8()?.replace(row.word, with: "nan") == f"seq: invalid 'not-a-number' argument: 'nan'\n{hint}"
  }
}
