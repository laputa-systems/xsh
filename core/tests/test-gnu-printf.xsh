use support.uu

proc check(s: uu.Scene, args: List[Str], expected: Bytes, status: Int = 0, diagnostic: Str = "", vars: Record = {}) [fs, process, env, error] {
  let r = uu.invoke(s, "printf", args, vars: vars)?
  uu.fails_with_code(r, status)
  uu.stdout_is_bytes(r, expected)
  uu.stderr_is(r, diagnostic)
}

# origin: gnu printf/printf-cov.log
test test_gnu_printf_printf_cov_log { |ctx|
  let s = uu.scene(ctx)?
  for args in [[], ["--"]] {
    check(s, args, b"", 1, "printf: missing operand\nTry 'printf --help' for more information.\n")
  }
  let escapes = "\\a\\b\\f\\n\\r\\t\\v\\z\\c"
  let expanded = b"\x07\x08\x0c\n\r\t\x0b\\z"
  for case in [{prefix: "", suffix: ""}, {prefix: "", suffix: "a"}, {prefix: "a", suffix: ""}, {prefix: "a", suffix: "b"}] {
    let input = f"{case.prefix}{escapes}{case.suffix}"
    let output = bytes.concat([bytes.from_text(case.prefix), expanded])
    check(s, [input], output)
    check(s, ["%b", input], output)
  }
  for case in [
    {args: ["%X", "999"], out: "3E7"},
    {args: ["%4X", "999"], out: " 3E7"},
    {args: ["%.4X", "999"], out: "03E7"},
    {args: ["%5.4X", "999"], out: " 03E7"},
    {args: ["%*X", "4", "42"], out: "  2A"},
    {args: ["%.*X", "4", "42"], out: "002A"},
    {args: ["%*.*X", "3", "2", "15"], out: " 0F"},
    {args: ["nl\\ntab\\tx"], out: "nl\ntab\tx"},
    {args: ["%c", "123"], out: "1"},
    {args: ["%*c", "3", "123"], out: "  1"},
    {args: ["%5.4d", "999"], out: " 0999"},
    {args: ["%*d", "4", "42"], out: "  42"},
    {args: ["%.*d", "4", "42"], out: "0042"},
    {args: ["%*.*d", "3", "2", "15"], out: " 15"},
    {args: ["%.*d", "-3", "15"], out: "15"},
    {args: ["%F", "1"], out: "1.000000"},
    {args: ["%LF", "1"], out: "1.000000"},
    {args: ["%E", "2"], out: "2.000000E+00"},
    {args: ["%LE", "2"], out: "2.000000E+00"},
    {args: ["%s", "x"], out: "x"},
    {args: ["%*s", "2", "x"], out: " x"},
    {args: ["%.*s", "2", "abcd"], out: "ab"},
    {args: ["%*.*s", "2", "2", "abcd"], out: "ab"},
    {args: ["%*s"], out: ""},
    {args: ["%.*s"], out: ""},
    {args: ["%5.4G", "3"], out: "    3"},
    {args: ["%*G", "4", "42"], out: "  42"},
    {args: ["%.*G", "4", "42"], out: "42"},
    {args: ["%*.*G", "5", "3", "15"], out: "   15"},
    {args: ["\\u0032"], out: "2"},
    {args: ["\\U00000032"], out: "2"},
    {args: ["%%"], out: "%"},
    {args: ["% d", "33"], out: " 33"},
    {args: ["%+d", "33"], out: "+33"},
    {args: ["%-d", "33"], out: "33"},
    {args: ["%02d", "1"], out: "01"},
    {args: ["%'d", "3333"], out: "3333"},
  ] { check(s, case.args, bytes.from_text(case.out)) }
  check(s, ["\\xaa\\0377"], b"\xaa\x1f7")
  for escape in ["\\x", "\\u00", "\\U0000", "\\u"] {
    check(s, [escape], b"", 1, "printf: missing hexadecimal number in escape\n")
  }
  check(s, ["\\ud800"], b"", 1, "printf: invalid universal character name \\ud800\n")
  check(s, ["%d", "no-num"], b"0", 1, "printf: 'no-num': expected a numeric value\n")
  check(s, ["%d", "9z"], b"9", 1, "printf: '9z': value not completely converted\n")
  let overflow = uu.invoke(s, "printf", ["%d", "999999999999999999999999999999"])?
  uu.fails_with_code(overflow, 1)
  assert regex.compile("^[0-9]+$")?.matches(overflow.stdout.utf8()?)
  assert regex.compile("^printf: '9{30}[^\n]*\n$")?.matches(overflow.stderr.utf8()?)
  check(s, ["B", "1"], b"B", 0, "printf: warning: ignoring excess arguments, starting with '1'\n")
  check(s, ["%#d", "3333"], b"", 1, "printf: %#d: invalid conversion specification\n")
}

# origin: gnu printf/printf-hex.log
test test_gnu_printf_printf_hex_log { |ctx|
  let s = uu.scene(ctx)?
  check(s, ["\\x7e3\\n"], b"~3\n")
}

# origin: gnu printf/printf-indexed.log
test test_gnu_printf_printf_indexed_log { |ctx|
  let s = uu.scene(ctx)?
  for case in [
    {args: ["%2$s%1$s\\n", "1", "2"], out: "21\n"},
    {args: ["%1$s%1$s\\n", "1", "2"], out: "11\n22\n"},
    {args: ["%s %3$s %s\\n", "A", "B", "C", "D"], out: "A C B\nD  \n"},
    {args: ["%1$*d\\n", "4", "1"], out: "   4\n1\n"},
    {args: ["%s %s %1$s\\n", "A", "B"], out: "A B A\n"},
    {args: ["%100$*d %s %s %s\\n", "4", "1"], out: "   0 1  \n"},
    {args: ["%1$*2$.*3$d\\n", "1", "3", "2"], out: " 01\n"},
    {args: ["%3$*.*d\\n", "3", "2", "1"], out: " 01\n"},
    {args: ["%3$*2$.*d\\n", "2", "3", "1"], out: " 01\n"},
    {args: ["%3$*.*2$d\\n", "3", "2", "1"], out: " 01\n"},
    {args: ["%2$*1$d\\n", "4", "1"], out: "   1\n"},
    {args: ["%2$*d\\n", "4", "1"], out: "   1\n"},
    {args: ["%01$4d\\n", "1"], out: "   1\n"},
    {args: ["%1$0*2$d\\n", "1", "4"], out: "0001\n"},
  ] { check(s, case.args, bytes.from_text(case.out)) }
  for case in [
    {args: ["%-2$s %1$s\\n", "A", "B"], err: "printf: %-2$: invalid conversion specification\n"},
    {args: ["% 2$s %1$s\\n", "A", "B"], err: "printf: % 2$: invalid conversion specification\n"},
    {args: ["%0x2$s %2$s\\n", "A", "B"], err: "printf: 'A': expected a numeric value\n"},
    {args: ["%$d\\n", "1"], err: "printf: %$: invalid conversion specification\n"},
  ] {
    let r = uu.invoke(s, "printf", case.args)?
    uu.fails_with_code(r, 1)
    uu.stderr_is(r, case.err)
  }
  for index in ["999", "2147483647", "2147483648", "9223372036854775807", "9223372036854775808"] {
    check(s, [f"empty%{index}" + "$s\\n", "foo"], b"empty\n")
  }
}

# origin: gnu printf/printf-mb.log
test test_gnu_printf_printf_mb_log { |ctx|
  let s = uu.scene(ctx)?
  let format = p"%04x\\n"
  for case in [
    {argument: bytes.from_text("\"á"), locale: "fr_FR.UTF-8", trailing: false},
    {argument: b"'\xe1", locale: "fr_FR.UTF-8", trailing: false},
    {argument: b"'\xe1", locale: "C", trailing: false},
    {argument: bytes.from_text("\"á="), locale: "fr_FR.UTF-8", trailing: true},
    {argument: b"'\xe1=", locale: "fr_FR.UTF-8", trailing: true},
  ] {
    let r = uu.invoke_paths(s, "printf", [format, Path.parse_bytes(case.argument)?], vars: {LC_ALL: case.locale})?
    uu.stdout_is_bytes(r, b"00e1\n")
    if case.trailing {
      let messages = r.stderr.utf8()?.split("\n")
      assert messages.len() == 2 and messages[1] == ""
      assert messages[0].starts_with("printf:") and "=" in messages[0]
    } else { uu.no_stderr(r) }
  }
}

# origin: gnu printf/printf-quote.log
test test_gnu_printf_printf_quote_log { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printf", ["%q\\n", "", "'", "a", "a b", "~a", "a~", "a\r", "\u{1}'\u{1}"])?
  uu.stdout_is(r, "''\n\"'\"\na\n'a b'\n'~a'\na~\n'a'$'\\r'\n''$'\\001'\\'''$'\\001'\n")
  let printable = uu.invoke(s, "printf", ["%q\\n", "áḃç"], vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.stdout_is(printable, "áḃç\n")
  let control = uu.invoke_paths(s, "printf", [p"%q\\n", Path.parse_bytes(b"\xc2\x81")?], vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.stdout_is(control, "''$'\\302\\201'\n")
  let plain = uu.invoke(s, "printf", ["%q\\n", "áḃç"], vars: {LC_ALL: "C"})?
  uu.stdout_is(plain, "''$'\\303\\241\\341\\270\\203\\303\\247'\n")
}

# origin: gnu printf/printf.log
test test_gnu_printf_printf_log { |ctx|
  let s = uu.scene(ctx)?
  check(s, ["\\x1b\\n\\33\\n\\e\\n"], b"\x1b\n\x1b\n\x1b\n")
  check(s, ["--", "foo\\n"], b"foo\n")
  for case in [
    {args: ["1 %*sy\\n", "-3", "x"], out: "1 x  y\n", status: 0},
    {args: ["3 \\x40\\n"], out: "3 @\n", status: 0},
    {args: ["5 % +d\\n", "234"], out: "5 +234\n", status: 0},
    {args: ["9 %*dx\\n", "-2", "0"], out: "9 0 x\n", status: 0},
    {args: ["10 %.*dx\\n", "-2147483649", "0"], out: "10 0x\n", status: 0},
    {args: ["%.*dx\\n", "2147483648", "0"], out: "", status: 1},
    {args: ["11 %*c\\n", "2", "x"], out: "11  x\n", status: 0},
    {args: ["12 %*s\\n", "", "empty width"], out: "12 empty width\n", status: 1},
    {args: ["13 %*s\\n", " ", "space width"], out: "13 space width\n", status: 1},
    {args: ["14 %.*sx\\n", "", "empty precision"], out: "14 x\n", status: 1},
    {args: ["15 %.*sx\\n", " ", "space precision"], out: "15 x\n", status: 1},
    {args: ["%#d\\n", "0"], out: "", status: 1},
    {args: ["%0s\\n", "0"], out: "", status: 1},
    {args: ["%.9c\\n", "0"], out: "", status: 1},
    {args: ["%'s\\n", "0"], out: "", status: 1},
  ] {
    let r = uu.invoke(s, "printf", case.args)?
    uu.fails_with_code(r, case.status)
    uu.stdout_is(r, case.out)
  }
  let invalid = uu.invoke(s, "printf", ["2 \\x"], vars: {POSIXLY_CORRECT: "1"})?
  uu.fails(invalid)
  check(s, ["4 \\x40\\n"], b"4 @\n", vars: {POSIXLY_CORRECT: "1"})
  check(s, ["6 \\41\\n"], b"6 !\n")
  check(s, ["7 \\2y \\02y \\002y \\0002y\\n"], b"7 \x02y \x02y \x02y \x002y\n")
  check(s, ["8 %b %b %b %b\\n", "\\1y", "\\01y", "\\001y", "\\0001y"], b"8 \x01y \x01y \x01y \x01y\n")
  for case in [
    {argument: "\"a", output: "97\n", error: ""},
    {argument: "\"a\"", output: "97\n", error: "printf: warning: \": character(s) following character constant have been ignored\n"},
    {argument: "\"", output: "0\n", error: "printf: '\"': expected a numeric value\n"},
    {argument: "a", output: "0\n", error: "printf: 'a': expected a numeric value\n"},
  ] {
    let r = uu.invoke(s, "printf", ["%d\\n", case.argument])?
    uu.stdout_is(r, case.output)
    uu.stderr_is(r, case.error)
  }
  let integer = uu.invoke(s, "printf", ["%70000d", "1"])?
  uu.succeeds(integer)
  assert integer.stdout.len() == 70000
  assert integer.stdout.utf8()?.replace(" ", with: "") == "1"
  let string = uu.invoke(s, "printf", ["%500000s", "x"])?
  uu.succeeds(string)
  assert string.stdout.len() == 500000
  assert string.stdout.utf8()?.replace(" ", with: "") == "x"
  let decimal = uu.invoke(s, "printf", ["%.70000f", "1"])?
  uu.succeeds(decimal)
  assert decimal.stdout.len() == 70002
  assert decimal.stdout.utf8()?.replace("0", with: "") == "1."
}
