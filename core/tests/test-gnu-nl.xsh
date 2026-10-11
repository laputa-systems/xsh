type DelimiterCase = {argument: Bytes, section: Bytes}
type NumberCase = {args: List[Str], input: Bytes, output: Str}
use support.uu

proc missing_between_files(s: uu.Scene) [fs, process, env, error] {
  uu.write(s, "file1", "a\n")?
  uu.write(s, "file2", "b\n")?
  let r = uu.invoke(s, "nl", ["file1", "missing", "file2"])?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "     1\ta\n     2\tb\n")
  uu.stderr_is(r, "nl: missing: No such file or directory\n")
}
# origin: gnu nl/multiple-files.log
test test_gnu_nl_multiple_files_log { |ctx|
  let s = uu.scene(ctx)?
  missing_between_files(s)
}
# origin: gnu nl/multibyte.log
test test_gnu_nl_multibyte_log { |ctx|
  let s = uu.scene(ctx)?
  let korean = b"\xeb\x89\x90"
  let twice = bytes.concat([korean, korean])
  let three = bytes.concat([twice, korean])
  let invalid = b"\xff|\xed\xba\xad|\xc2\x89|\xed\xa6\xbf\xed\xbf\xbf"
  let cases: List[DelimiterCase] = [
    {argument: b"\xc3", section: b"\xc3:"},
    {argument: korean, section: bytes.concat([korean, b":"])},
    {argument: b"\xc3\xc3", section: b"\xc3\xc3"},
    {argument: twice, section: twice},
    {argument: three, section: three},
    {argument: invalid, section: invalid},
  ]
  for c in cases {
    let input = bytes.concat([c.section, c.section, c.section, b"\na\n", c.section, c.section, b"\nb\n", c.section, b"\nc\n"])
    uu.write_bytes(s, "inp", input)?
    let args = [p"-p", p"-ha", p"-fa", p"-d", Path.parse_bytes(c.argument)?]
    let vars = {LC_ALL: "fr_FR.utf-8", LANGUAGE: "C"}
    let words = uu.argv(s, "nl", args, vars)?
    let out = uu.at(s, "out")
    let err = uu.at(s, "err")
    let plan = process.command_argv(words[0], words, s.root, vars, uu.at(s, "inp"), out, err)
    let status = process.run(plan)?
    assert status.exited_with(0)
    assert out.read_bytes()? == b"\n     1\ta\n\n     2\tb\n\n     3\tc\n"
    assert err.read_bytes()? == b""
  }
}
# origin: gnu nl/nl.log
test test_gnu_nl_nl_log { |ctx|
  let s = uu.scene(ctx)?
  let cases: List[NumberCase] = [
    {args: [], input: b"a\n", output: "     1\ta\n"},
    {args: ["-s%n"], input: b"b\n", output: "     1%nb\n"},
    {args: ["-n", "ln"], input: b"c\n", output: "1     \tc\n"},
    {args: ["-n", "rn"], input: b"d\n", output: "     1\td\n"},
    {args: ["-n", "rz"], input: b"e\n", output: "000001\te\n"},
    {args: [], input: b"a\n\n", output: "     1\ta\n       \n"},
  ]
  for c in cases {
    let r = uu.invoke(s, "nl", c.args, c.input)?
    uu.succeeds(r)
    uu.stdout_only(r, c.output)
  }
  uu.write(s, "in.txt", "\\:\\:\\:\na\n\\:\\:\nb\n\\:\nc\n")?
  for no_reset in [false, true] {
    let args = if no_reset { ["-p", "-ha", "-fa", "in.txt"] } else { ["-ha", "-fa", "in.txt"] }
    let r = uu.invoke(s, "nl", args)?
    uu.succeeds(r)
    let expected = if no_reset { "\n     1\ta\n\n     2\tb\n\n     3\tc\n" } else { "\n     1\ta\n\n     1\tb\n\n     1\tc\n" }
    uu.stdout_only(r, expected)
  }
  uu.fails_with_code(uu.invoke(s, "nl", ["-v9223372036854775808", "/dev/null"])?, 1)
  uu.write(s, "in.txt", "a\n\\:\\:\nb\n")?
  let reset_max = uu.invoke(s, "nl", ["-v9223372036854775807", "in.txt"])?
  uu.succeeds(reset_max)
  uu.stdout_only(reset_max, "9223372036854775807\ta\n\n9223372036854775807\tb\n")
  uu.fails_with_code(uu.invoke(s, "nl", ["-p", "-v9223372036854775807", "in.txt"])?, 1)
  uu.fails_with_code(uu.invoke(s, "nl", ["-i-9223372036854775809", "/dev/null"])?, 1)
  uu.write(s, "in.txt", "a\nb\n")?
  let negative = uu.invoke(s, "nl", ["-v9223372036854775807", "-i-9223372036854775808", "in.txt"])?
  uu.succeeds(negative)
  uu.stdout_only(negative, "9223372036854775807\ta\n    -1\tb\n")
  uu.write(s, "in.txt", "a\nb\nc\n")?
  uu.fails_with_code(uu.invoke(s, "nl", ["-v9223372036854775807", "-i-9223372036854775808", "in.txt"])?, 1)
  uu.write(s, "in.txt", "a\n\\:\\:\nc\n")?
  let disabled = uu.invoke(s, "nl", ["-d", "", "in.txt"])?
  uu.succeeds(disabled)
  uu.stdout_only(disabled, "     1\ta\n     2\t\\:\\:\n     3\tc\n")
  uu.write(s, "in.txt", "a\nfoofoo\nc\n")?
  let long_delimiter = uu.invoke(s, "nl", ["-d", "foo", "in.txt"])?
  uu.succeeds(long_delimiter)
  uu.stdout_only(long_delimiter, "     1\ta\n\n     1\tc\n")
  uu.write(s, "in.txt", "a\nx:x:\nc\n")?
  let short_delimiter = uu.invoke(s, "nl", ["-d", "x", "in.txt"])?
  uu.succeeds(short_delimiter)
  uu.stdout_only(short_delimiter, "     1\ta\n\n     1\tc\n")
  missing_between_files(s)
}
