use support.uu

type TabCase = {name: Str, args: List[Str], input: Bytes, output: Bytes}

proc check_tabs(s: uu.Scene, rows: List[TabCase]) [fs, process, env, error] {
  for row in rows {
    let name = f"{row.name}.input"
    uu.write_bytes(s, name, row.input)?
    let r = uu.invoke(s, "expand", row.args.extend([name]))?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, row.output)
  }
}

# origin: gnu expand/expand.log
test test_gnu_expand_expand_log { |ctx|
  let s = uu.scene(ctx)?
  check_tabs(s, [
    {name: "t1", args: ["--tabs=3"], input: b"a\tb", output: b"a  b"},
    {name: "t2", args: ["--tabs=3,6,9"], input: b"a\tb\tc\td\te", output: b"a  b  c  d e"},
    {name: "t3", args: ["--tabs=3 6 9"], input: b"a\tb\tc\td\te", output: b"a  b  c  d e"},
    {name: "t4", args: ["--tabs=, 3,6 9"], input: b"a\tb\tc\td\te", output: b"a  b  c  d e"},
    {name: "t4a", args: ["--tabs=3\t6\t9"], input: b"a\tb\tc\td\te", output: b"a  b  c  d e"},
    {name: "t5", args: ["--tabs="], input: b"a\tb\tc", output: b"a       b       c"},
    {name: "t6", args: ["--tabs=,"], input: b"a\tb\tc", output: b"a       b       c"},
    {name: "t7", args: ["--tabs= "], input: b"a\tb\tc", output: b"a       b       c"},
    {name: "t8", args: ["--tabs=/"], input: b"a\tb\tc", output: b"a       b       c"},
    {name: "if", args: ["--tabs=6,9"], input: b"a\tbbbbbbbbbbbbb\tc", output: b"a     bbbbbbbbbbbbb c"},
    {name: "i1", args: ["--tabs=3", "-i"], input: b"\ta\tb", output: b"   a\tb"},
    {name: "i2", args: ["--tabs=3", "-i"], input: b" \ta\tb", output: b"   a\tb"},
    {name: "u1", args: ["-3"], input: b"a\tb\tc", output: b"a  b  c"},
    {name: "u2", args: ["-4", "-9"], input: b"a\tb\tc", output: b"a   b    c"},
    {name: "u3", args: ["-11"], input: b"a\tb\tc", output: b"a          b          c"},
    {name: "u4", args: ["-2", "-6"], input: b"a\tb\tc", output: b"a b   c"},
    {name: "u5", args: ["-7"], input: b"a\tb", output: b"a      b"},
    {name: "u6", args: ["-8"], input: b"a\tb", output: b"a       b"},
    {name: "u7", args: ["-3,9"], input: b"a\tb\tc", output: b"a  b     c"},
    {name: "b1", args: [], input: b"aaa\x08\x08\x08c\td\n", output: b"aaa\x08\x08\x08c       d\n"},
    {name: "b2", args: [], input: b"\x08c\td", output: b"\x08c       d"},
    {name: "b3", args: ["--tabs", "2,4,6,10"], input: b"1\t2\t3\t4\t5\na\tb\tc\td\te\n", output: b"1 2 3 4   5\na b c d   e\n"},
    {name: "b4", args: ["--tabs", "2,4,6,10"], input: b"1\t2\t3\t4\t5\na\tbHELLO\x08\x08\x08\x08\x08\tc\td\te\n", output: b"1 2 3 4   5\na bHELLO\x08\x08\x08\x08\x08 c d   e\n"},
    {name: "b5", args: ["--tabs", "2,4,6,10"], input: b"1\t2\t3\t4\t5\na\tbHELLO\x08\x08\x08\tc\td\te\n", output: b"1 2 3 4   5\na bHELLO\x08\x08\x08 c   d e\n"},
    {name: "trail1", args: ["--tabs=1,/5"], input: b"\ta\tb\tc", output: b" a   b    c"},
    {name: "trail2", args: ["--tabs=2,/5"], input: b"\ta\tb\tc", output: b"  a  b    c"},
    {name: "trail3", args: ["--tabs=1,2,/5"], input: b"\ta\tb\tc", output: b" a   b    c"},
    {name: "trail4", args: ["--tabs=/5"], input: b"\ta\tb", output: b"     a    b"},
    {name: "trail5", args: ["--tabs=//5"], input: b"\ta\tb", output: b"     a    b"},
    {name: "trail5a", args: ["--tabs=+/5"], input: b"\ta\tb", output: b"     a    b"},
    {name: "trail6", args: ["--tabs=/,/5"], input: b"\ta\tb", output: b"     a    b"},
    {name: "trail7", args: ["--tabs=,/5"], input: b"\ta\tb", output: b"     a    b"},
    {name: "trail8", args: ["--tabs=1", "-t/5"], input: b"\ta\tb\tc", output: b" a   b    c"},
    {name: "trail9", args: ["--tab=1,2", "-t/5"], input: b"\ta\tb\tc", output: b" a   b    c"},
    {name: "incre0", args: ["--tab=1,+5"], input: b"+\t\ta\tb", output: b"+          a    b"},
    {name: "incre1", args: ["--tabs=1,+5"], input: b"\ta\tb\tc", output: b" a    b    c"},
    {name: "incre2", args: ["--tabs=2,+5"], input: b"\ta\tb\tc", output: b"  a    b    c"},
    {name: "incre3", args: ["--tabs=1,2,+5"], input: b"\ta\tb\tc", output: b" a     b    c"},
    {name: "incre4", args: ["--tabs=+5"], input: b"\ta\tb", output: b"     a    b"},
    {name: "incre5", args: ["--tabs=++5"], input: b"\ta\tb", output: b"     a    b"},
    {name: "incre5a", args: ["--tabs=/+5"], input: b"\ta\tb", output: b"     a    b"},
    {name: "incre6", args: ["--tabs=+,+5"], input: b"\ta\tb", output: b"     a    b"},
    {name: "incre7", args: ["--tabs=,+5"], input: b"\ta\tb", output: b"     a    b"},
    {name: "incre8", args: ["--tabs=1", "-t+5"], input: b"\ta\tb\tc", output: b" a    b    c"},
    {name: "incre9", args: ["--tab=1,2", "-t+5"], input: b"\ta\tb\tc", output: b" a     b    c"},
  ])
  for row in [
    {first: b"a\tb\n", second: b"c\td\n", output: b"a   b\nc   d\n"},
    {first: b"", second: b"c\td\n", output: b"c   d\n"},
    {first: b"a\tb\n", second: b"", output: b"a   b\n"},
  ] {
    uu.write_bytes(s, "in1", row.first)?
    uu.write_bytes(s, "in2", row.second)?
    let r = uu.invoke(s, "expand", ["--tabs=4", "in1", "in2"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, row.output)
  }
  for row in [
    {args: ["--tabs=a"], error: "expand: tab size contains invalid character(s): 'a'\n"},
    {args: ["-t", "18446744073709551616"], error: "expand: tab stop is too large '18446744073709551616'\n"},
    {args: ["--tabs=0"], error: "expand: tab size cannot be 0\n"},
    {args: ["--tabs=3,3"], error: "expand: tab sizes must be ascending\n"},
    {args: ["--tabs=/3,6,8"], error: "expand: '/' specifier only allowed with the last value\n"},
    {args: ["-t/3", "-t/6"], error: "expand: '/' specifier only allowed with the last value\n"},
    {args: ["--tabs=3/"], error: "expand: '/' specifier not at start of number: '/'\n"},
  ] {
    uu.touch(s, "empty")?
    let r = uu.invoke(s, "expand", row.args.extend(["empty"]))?
    uu.fails_with_code(r, 1)
    uu.stderr_only(r, row.error)
  }
}

# origin: gnu expand/mb.log
test test_gnu_expand_mb_log { |ctx|
  let s = uu.scene(ctx)?
  let vars = {LC_ALL: "fr_FR.UTF-8"}
  let input = bytes.from_text("1234567812345678123456781\n.       .       .       .\na\tb\tc\td\n.       .       .       .\nä\tö\tü\tß\n.       .       .       .\n   äöü\t.    öüä.   \tä xx\n")
  let output = bytes.from_text("1234567812345678123456781\n.       .       .       .\na       b       c       d\n.       .       .       .\nä       ö       ü       ß\n.       .       .       .\n   äöü  .    öüä.       ä xx\n")
  uu.write_bytes(s, "in", input)?
  let first = uu.invoke_from_path(s, "expand", [], uu.at(s, "in"), vars)?
  uu.succeeds(first)
  uu.stdout_is_bytes(first, output)
  let twice = uu.invoke(s, "expand", ["./in", "./in"], vars: vars)?
  uu.succeeds(twice)
  uu.stdout_is_bytes(twice, bytes.concat([output, output]))
  uu.write(s, "in", "12345678\ne\t|ascii(1)\né\t|composed(1)\né\t|decomposed(1)\n　\t|ideo-space(2)\n－\t|full-hypen(2)\n")?
  let widths = uu.invoke_from_path(s, "expand", [], uu.at(s, "in"), vars)?
  uu.succeeds(widths)
  uu.stdout_is(widths, "12345678\ne       |ascii(1)\né       |composed(1)\né       |decomposed(1)\n　      |ideo-space(2)\n－      |full-hypen(2)\n")
  uu.write_bytes(s, "in", b"\n")?
  let newline = uu.invoke_from_path(s, "expand", [], uu.at(s, "in"), vars)?
  uu.succeeds(newline)
  uu.stdout_is_bytes(newline, b"\n")
  let a = bytes.from_text("ä")
  uu.write_bytes(s, "in", bytes.concat([b"12345678\n\t\xff|\n\xff\t|\n\t\xff", a, b"|\n", a, b"\xff\t|\n\t", a, b"\xff|\n\xff\t", a, b"|\n", a, b"bcdef\xff\t|\n"]))?
  let invalid = uu.invoke_from_path(s, "expand", [], uu.at(s, "in"), vars)?
  uu.succeeds(invalid)
  uu.stdout_is_bytes(invalid, bytes.concat([b"12345678\n        \xff|\n\xff       |\n        \xff", a, b"|\n", a, b"\xff      |\n        ", a, b"\xff|\n\xff       ", a, b"|\n", a, b"bcdef\xff |\n"]))
  uu.write_bytes(s, "in", bytes.concat([b"\xef\xbb\xbf", input]))?
  let bom = uu.invoke_from_path(s, "expand", [], uu.at(s, "in"), vars)?
  uu.succeeds(bom)
  uu.stdout_is_bytes(bom, bytes.concat([b"\xef\xbb\xbf", output]))
  uu.write_bytes(s, "in1", bytes.concat([b"\xef\xbb\xbf", input]))?
  let multiple_bom = uu.invoke(s, "expand", ["in1", "in1"], vars: vars)?
  uu.succeeds(multiple_bom)
  uu.stdout_is_bytes(multiple_bom, bytes.concat([b"\xef\xbb\xbf", output, b"\xef\xbb\xbf", output]))
}
