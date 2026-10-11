use support.uu

type ReverseCase = {args: List[Str], input: Bytes, output: Bytes}

# Named files, regular stdin and pipe stdin exercise separate seekability paths.
# Each pipe still launches its applet through the oracle's lossless argv helper.
proc pipe_tac(s: uu.Scene, args: List[Str], vars: Record = {}) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let launch = uu.argv(s, "tac", [Path(arg) for arg in args], vars)?
  let argv = [p"/bin/sh", p"-c", p"cat input | \"$@\"", p"tac-pipe"].extend(launch)
  let out = uu.at(s, ".pipe-out")
  let err = uu.at(s, ".pipe-err")
  let status = process.run(process.command_argv(p"/bin/sh", argv, s.root, vars, b"", out, err, timeout: 10s))?
  Ok({util: "tac", args: args, status: status.shell_code()?, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}

proc reverse_transports(s: uu.Scene, rows: List[ReverseCase]) [fs, process, env, error] {
  for row in rows {
    uu.write_bytes(s, "input", row.input)?
    let file = uu.invoke(s, "tac", row.args.extend(["input"]), timeout: 10s)?
    uu.succeeds(file)
    uu.stdout_only_bytes(file, row.output)
    let regular_stdin = uu.invoke_from_path(s, "tac", row.args, uu.at(s, "input"), timeout: 10s)?
    uu.succeeds(regular_stdin)
    uu.stdout_only_bytes(regular_stdin, row.output)
    let pipe = pipe_tac(s, row.args)?
    uu.succeeds(pipe)
    uu.stdout_only_bytes(pipe, row.output)
  }
}

# origin: gnu tac/tac-locale.log
test test_gnu_tac_tac_locale_log { |ctx|
  let s = uu.scene(ctx)?
  for row in [
    {locale: "en_US.iso8859-1", separator: b"\xe9"},
    {locale: "en_US.iso8859-1", separator: b"\xe9\xea"},
    {locale: "fr_FR.UTF-8", separator: bytes.from_text("д")},
    {locale: "fr_FR.UTF-8", separator: bytes.from_text("дж")},
    {locale: "fr_FR.UTF-8", separator: b"\xff|\xed\xba\xad|\xc2\x89|\xed\xa6\xbf\xed\xbf\xbf"},
  ] {
    uu.write_bytes(s, "inp", bytes.concat([b"1", row.separator, b"2", row.separator, b"3", row.separator]))?
    let option = Path.parse_bytes(bytes.concat([b"--separator=", row.separator]))?
    let r = uu.invoke_paths(s, "tac", [option, p"inp"], vars: {LC_ALL: row.locale}, timeout: 10s)?
    uu.succeeds(r)
    assert bytes.concat([r.stdout, b"\n"]) == bytes.concat([b"3", row.separator, b"2", row.separator, b"1", row.separator, b"\n"])
  }
}

# origin: gnu tac/tac.log
test test_gnu_tac_tac_log { |ctx|
  let s = uu.scene(ctx)?
  for row in [
    {first: b"a\n", second: b"b\n", output: b"a\nb\n"},
    {first: b"a\nb\n", second: b"1\n2\n", output: b"b\na\n2\n1\n"},
  ] {
    uu.write_bytes(s, "first", row.first)?
    uu.write_bytes(s, "second", row.second)?
    let r = uu.invoke(s, "tac", ["-r", "first", "second"], timeout: 10s)?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, row.output)
  }
  reverse_transports(s, [
    {args: [], input: b"", output: b""},
    {args: [], input: b"a", output: b"a"},
    {args: [], input: b"\n", output: b"\n"},
    {args: [], input: b"a\n", output: b"a\n"},
    {args: [], input: b"a\nb", output: b"ba\n"},
    {args: [], input: b"a\nb\n", output: b"b\na\n"},
    {args: [], input: b"1234567\n8\n", output: b"8\n1234567\n"},
    {args: [], input: b"12345678\n9\n", output: b"9\n12345678\n"},
    {args: [], input: b"123456\n8\n", output: b"8\n123456\n"},
    {args: [], input: b"12345\n8\n", output: b"8\n12345\n"},
    {args: [], input: b"1234\n8\n", output: b"8\n1234\n"},
    {args: [], input: b"123\n8\n", output: b"8\n123\n"},
    {args: ["-s", ""], input: b"", output: b""},
    {args: ["-s", ""], input: b"a", output: b"a"},
    {args: ["-s", ""], input: b"\0", output: b"\0"},
    {args: ["-s", ""], input: b"a\0", output: b"a\0"},
    {args: ["-s", ""], input: b"a\0b", output: b"ba\0"},
    {args: ["-s", ""], input: b"a\0b\0", output: b"b\0a\0"},
    {args: ["-b"], input: b"\na\nb\nc", output: b"\nc\nb\na"},
    {args: ["-s:"], input: b"a:b:c:", output: b"c:b:a:"},
    {args: ["-s", ":", "-b"], input: b":a:b:c", output: b":c:b:a"},
    {args: ["-r", "-s", "\\._+"], input: b"1._2.__3.___4._", output: b"4._3.___2.__1._"},
    {args: ["-r", "-s", "\\._+"], input: b"a.___b.__1._2.__3.___4._", output: b"4._3.___2.__1._b.__a.___"},
    {args: ["-r", "-s", "^"], input: b"a\nb\nc\n", output: b"c\nb\na\n"},
    {args: ["-r", "-s", "$"], input: b"a\nb\nc\n", output: b"\n\nc\nba"},
    {args: ["-r", "-s", "^$"], input: b"a\nb\nc\n", output: b"a\nb\nc\n"},
    {args: ["-b", "-r", "-s", "\\._+"], input: b"._1._2.__3.___4", output: b".___4.__3._2._1"},
    {args: ["-b", "-r", "-s", "\\._+"], input: b".__x.___y.____z._1._2.__3.___4", output: b".___4.__3._2._1.____z.___y.__x"},
  ])
  uu.write_bytes(s, "input", b"a\n")?
  let bad_tmpdir = pipe_tac(s, [], {TMPDIR: "no/such/dir"})?
  uu.succeeds(bad_tmpdir)
  uu.stdout_only_bytes(bad_tmpdir, b"a\n")
  let long_line = bytes.from_text(["o"] |> repeat(16385) |> join(""))
  reverse_transports(s, [{args: [], input: long_line, output: long_line}])
}
