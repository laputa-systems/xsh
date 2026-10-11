use support.uu as uu

# origin: gnu unexpand/unexpand.log
test test_gnu_unexpand_unexpand_log { |ctx|
  let s = uu.scene(ctx)?
  for row in [
    {args: [], input: " y\n", output: " y\n"},
    {args: [], input: "  y\n", output: "  y\n"},
    {args: [], input: "   y\n", output: "   y\n"},
    {args: [], input: "    y\n", output: "    y\n"},
    {args: [], input: "     y\n", output: "     y\n"},
    {args: [], input: "      y\n", output: "      y\n"},
    {args: [], input: "       y\n", output: "       y\n"},
    {args: [], input: "        y\n", output: "\ty\n"},
    {args: ["-a"], input: "w y\n", output: "w y\n"},
    {args: ["-a"], input: "w  y\n", output: "w  y\n"},
    {args: ["-a"], input: "w   y\n", output: "w   y\n"},
    {args: ["-a"], input: "w    y\n", output: "w    y\n"},
    {args: ["-a"], input: "w     y\n", output: "w     y\n"},
    {args: ["-a"], input: "w      y\n", output: "w      y\n"},
    {args: ["-a"], input: "w       y\n", output: "w\ty\n"},
    {args: ["-a"], input: "w        y\n", output: "w\t y\n"},
    {args: ["-t", "2,4"], input: "      .", output: "\t\t  ."},
    {args: ["-t", "1,2"], input: " \t\t .\n", output: "\t\t\t .\n"},
    {args: ["-t", "4,5"], input: "    \t\t \n", output: "\t\t\t \n"},
    {args: ["-t", "2,3"], input: "x \t\t \n", output: "x\t\t\t \n"},
    {args: ["-t", "1,2"], input: " \t\t   \n", output: "\t\t\t   \n"},
    {args: ["-t", "1,2"], input: "x\t\t .\n", output: "x\t\t .\n"},
    {args: ["-t", "3"], input: "   a  b\n", output: "\ta\tb\n"},
    {args: ["-t", "3", "--first-only"], input: "   a  b\n", output: "\ta  b\n"},
    {args: ["-t", "3,6"], input: "   a  b\n", output: "\ta\tb\n"},
    {args: ["-t", "3 6"], input: "   a  b\n", output: "\ta\tb\n"},
    {args: ["-t", "3\t6"], input: "   a  b\n", output: "\ta\tb\n"},
    {args: ["-t", ", 3,6"], input: "   a  b\n", output: "\ta\tb\n"},
    {args: ["-t", "1"], input: " b  c   d\n", output: "\tb\t\tc\t\t\td\n"},
    {args: ["-t", "1"], input: "a \n", output: "a \n"},
    {args: ["-t", "1"], input: "a  \n", output: "a\t\t\n"},
    {args: ["-t", "1"], input: "a   \n", output: "a\t\t\t\n"},
    {args: ["-t", "1"], input: "a ", output: "a "},
    {args: ["-t", "1"], input: "a  ", output: "a\t\t"},
    {args: ["-t", "1"], input: "a   ", output: "a\t\t\t"},
    {args: ["-t", "1"], input: " a a  a\n", output: "\ta a\t\ta\n"},
    {args: ["-t", "2"], input: "   a  a  a\n", output: "\t a\ta\t a\n"},
    {args: ["-t", "3,4"], input: "0 2 4 6\t8\n", output: "0 2 4 6\t8\n"},
    {args: ["-t", "3,4"], input: "    4\n", output: "\t\t4\n"},
    {args: ["-t", "3,4"], input: "01  4\n", output: "01\t\t4\n"},
    {args: ["-t", "3,4"], input: "0   4\n", output: "0\t\t4\n"},
    {args: ["-t", "3,+6"], input: "\t      ", output: "\t\t"},
    {args: ["-t", "3,/9"], input: "\t      ", output: "\t\t"},
    {args: ["-a"], input: "1234567   \t1\n", output: "1234567\t\t1\n"},
    {args: ["-a"], input: "1234567  \t1\n", output: "1234567\t\t1\n"},
    {args: ["-a"], input: "1234567 \t1\n", output: "1234567\t\t1\n"},
    {args: ["-a"], input: "1234567\t1\n", output: "1234567\t1\n"},
    {args: ["-a"], input: "1234567  1\n", output: "1234567\t 1\n"},
    {args: ["-a"], input: "1234567 1\n", output: "1234567 1\n"},
    {args: ["-a", "-t4"], input: "aa  c\n", output: "aa\tc\n"},
    {args: ["-a", "-t4"], input: "aa\x08  c\n", output: "aa\x08  c\n"},
    {args: ["-a", "-t4"], input: "aa\x08   c\n", output: "aa\x08\tc\n"},
    {args: ["-a", "-t3"], input: "aa\x08  c\n", output: "aa\x08\tc\n"},
    {args: ["-a", "-3"], input: "a  b  c", output: "a\tb\tc"},
    {args: ["-a", "-4,9"], input: "a   b    c", output: "a\tb\tc"},
    {args: ["-a", "-11"], input: "a          b", output: "a\tb"},
    {args: ["-a", "-2,6"], input: "a b   c", output: "a b\tc"},
    {args: ["-a", "-7"], input: "a      b", output: "a\tb"},
    {args: ["-a", "-8"], input: "a       b", output: "a\tb"},
    {args: ["-a", "-3,9"], input: "a  b     c", output: "a\tb\tc"},
    {args: ["-3"], input: "   a   b", output: "\ta   b"},
    {args: ["-t8,9"], input: "x\t \t y\n", output: "x\t\t\t y\n"},
    {args: ["-t5,8"], input: "x\t \t y\n", output: "x\t\t y\n"},
  ] {
    let result = uu.invoke(s, "unexpand", row.args, stdin: bytes.from_text(row.input))?
    uu.succeeds(result)
    uu.stdout_only(result, row.output)
  }
  let overflow = uu.invoke(s, "unexpand", ["-18446744073709551616"])?
  uu.fails_with_code(overflow, 1)
  uu.stderr_only(overflow, "unexpand: tab stop is too large\n")
}

# origin: gnu unexpand/mb.log
test test_gnu_unexpand_mb_log { |ctx|
  let s = uu.scene(ctx)?
  let vars = {LC_ALL: "fr_FR.utf8"}
  let scale = ["12345678" for _ in range(3)].join("") + "1"
  let dots = ["." for _ in range(4)].join("       ")
  let basic = [scale, dots, ["a", "b", "c", "d"].join("       "), dots,
    ["ä", "ö", "ü", "ß"].join("       "), dots, "   äöü  .    öüä.       ä xx"].join("\n") + "\n"
  let expected = [scale, ".\t.\t.\t.", "a\tb\tc\td", ".\t.\t.\t.",
    "ä\tö\tü\tß", ".\t.\t.\t.", "   äöü\t.    öüä.\tä xx"].join("\n") + "\n"
  uu.write(s, "in", basic)?
  let first = uu.invoke_from_path(s, "unexpand", ["-a"], uu.at(s, "in"), vars: vars)?
  uu.succeeds(first)
  uu.stdout_is(first, expected)
  let twice = uu.invoke(s, "unexpand", ["-a", "./in", "./in"], vars: vars)?
  uu.succeeds(twice)
  uu.stdout_is(twice, expected + expected)

  let width_input = ["12345678", "e       |ascii(1)", "é       |composed(1)", "é       |decomposed(1)",
    "　      |ideo-space(2)", "　　　　|ideo-space(2) * 4", "－      |full-hypen(2)"].join("\n") + "\n"
  let width_expected = ["12345678", "e\t|ascii(1)", "é\t|composed(1)", "é\t|decomposed(1)",
    "\t|ideo-space(2)", "\t|ideo-space(2) * 4", "－\t|full-hypen(2)"].join("\n") + "\n"
  uu.write(s, "in", width_input)?
  for _ in range(2) {
    let width = uu.invoke_from_path(s, "unexpand", ["-a"], uu.at(s, "in"), vars: vars)?
    uu.succeeds(width)
    uu.stdout_is(width, width_expected)
  }

  let invalid_input = b"12345678\n        \xff|\n\xff       |\n        \xff\xc3\xa4|\n\xc3\xa4\xff      |\n        \xc3\xa4\xff|\n\xff       \xc3\xa4|\n\xc3\xa4bcde\xff  |\n"
  let invalid_expected = b"12345678\n\t\xff|\n\xff\t|\n\t\xff\xc3\xa4|\n\xc3\xa4\xff\t|\n\t\xc3\xa4\xff|\n\xff\t\xc3\xa4|\n\xc3\xa4bcde\xff\t|\n"
  uu.write_bytes(s, "in", invalid_input)?
  let invalid = uu.invoke_from_path(s, "unexpand", ["-a"], uu.at(s, "in"), vars: vars)?
  uu.succeeds(invalid)
  uu.stdout_is_bytes(invalid, invalid_expected)

  let bom_input = bytes.concat([b"\xef\xbb\xbf", bytes.from_text(basic)])
  let bom_expected = bytes.concat([b"\xef\xbb\xbf", bytes.from_text(expected)])
  uu.write_bytes(s, "in", bom_input)?
  let bom = uu.invoke_from_path(s, "unexpand", ["-a"], uu.at(s, "in"), vars: vars)?
  uu.succeeds(bom)
  uu.stdout_is_bytes(bom, bom_expected)
  let repeated = uu.invoke(s, "unexpand", ["-a", "./in", "./in"], vars: vars)?
  uu.succeeds(repeated)
  uu.stdout_is_bytes(repeated, bytes.concat([bom_expected, bom_expected]))
  for stop in ["4611686018427387904", "3074457345618258603"] {
    let result = uu.invoke(s, "unexpand", ["-t", stop], stdin: b"   \n", vars: vars)?
    assert result.status in [0, 1]
  }
  let wide_run = bytes.from_text(["　" for _ in range(40000)].join("") + "\n")
  uu.succeeds(uu.invoke(s, "unexpand", ["-t1"], stdin: wide_run, vars: vars)?)
}
