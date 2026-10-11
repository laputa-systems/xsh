use support.uu

pure byte_text(data: Bytes) -> Str { [f"{data.byte_at(index) ?? 0}" for index in range(data.len())].join(",") }

type ExpressionCase = {args: List[Str], out: Str, status: Int, err: Str}

# Run every row before reporting failures so unrelated arithmetic, syntax and
# locale behavior stays observable when one expression differs.
proc check_rows(s: uu.Scene, rows: List[ExpressionCase], locale: Str) [fs, process, env, error] -> Result[List[Str], Error] {
  var failures: List[Str] = []
  for item in rows {
    let r = uu.invoke(s, "expr", item.args, vars: {LC_ALL: locale, LANGUAGE: "C", LANG: "C"}, timeout: 5s)?
    # Regex libraries report either invalid interval content or an oversized
    # expression for this count beyond the supported repetition limit.
    let stderr = if item.args == ["_", ":", r"a\{32768\}"] {
      bytes.from_text(r.stderr.utf8()?.replace("Regular expression too big", with: r"Invalid content of \{\}"))
    } else { r.stderr }
    if r.status != item.status or r.stdout != bytes.from_text(item.out) or stderr != bytes.from_text(item.err) {
      failures += [f"{locale} expr {item.args.join(" ")}: status {r.status} expected {item.status}; stdout {r.stdout.utf8() ?? "raw bytes"} expected {item.out}; stderr {r.stderr.utf8() ?? "raw bytes"} expected {item.err}"]
    }
  }
  Ok(failures)
}

# origin: gnu expr/expr.log
test test_gnu_expr_expr_log { |ctx|
  let s = uu.scene(ctx)?
  let rows = [
    {args: ["5", "+", "6"], out: "11\n", status: 0, err: ""},
    {args: ["5", "-", "6"], out: "-1\n", status: 0, err: ""},
    {args: ["5", "*", "6"], out: "30\n", status: 0, err: ""},
    {args: ["100", "/", "6"], out: "16\n", status: 0, err: ""},
    {args: ["100", "%", "6"], out: "4\n", status: 0, err: ""},
    {args: ["3", "+", "-2"], out: "1\n", status: 0, err: ""},
    {args: ["-2", "+", "-2"], out: "-4\n", status: 0, err: ""},
    {args: ["--", "-11", "+", "12"], out: "1\n", status: 0, err: ""},
    {args: ["-11", "+", "12"], out: "1\n", status: 0, err: ""},
    {args: ["--", "-1", "+", "2"], out: "1\n", status: 0, err: ""},
    {args: ["-1", "+", "2"], out: "1\n", status: 0, err: ""},
    {args: ["--", "2", "+", "2"], out: "4\n", status: 0, err: ""},
    {args: ["(", "100", "%", "6", ")"], out: "4\n", status: 0, err: ""},
    {args: ["(", "100", "%", "6", ")", "-", "8"], out: "-4\n", status: 0, err: ""},
    {args: ["9", "/", "(", "100", "%", "6", ")", "-", "8"], out: "-6\n", status: 0, err: ""},
    {args: ["9", "/", "(", "(", "100", "%", "6", ")", "-", "8", ")"], out: "-2\n", status: 0, err: ""},
    {args: ["9", "+", "(", "100", "%", "6", ")"], out: "13\n", status: 0, err: ""},
    {args: ["00", "<", "0!"], out: "0\n", status: 1, err: ""},
    {args: ["00"], out: "00\n", status: 1, err: ""},
    {args: ["-0"], out: "-0\n", status: 1, err: ""},
    {args: ["0", "&", "1", "/", "0"], out: "0\n", status: 1, err: ""},
    {args: ["1", "|", "1", "/", "0"], out: "1\n", status: 0, err: ""},
    {args: ["1", "|", "(", "1", "/", "0", ")"], out: "1\n", status: 0, err: ""},
    {args: ["0", "&", "(", "1", "/", "0", ")"], out: "0\n", status: 1, err: ""},
    {args: ["1", "|", "(", "0", "&", "(", "1", "/", "0", ")", ")"], out: "1\n", status: 0, err: ""},
    {args: ["0", "&", "(", "1", "|", "(", "1", "/", "0", ")", ")"], out: "0\n", status: 1, err: ""},
    {args: ["", "|", ""], out: "0\n", status: 1, err: ""},
    {args: ["3", "+", "-"], out: "", status: 2, err: "expr: non-integer argument\n"},
    {args: ["--", "-2417851639229258349412352", "<", "2417851639229258349412352"], out: "1\n", status: 0, err: ""},
    {args: ["a\nb", ":", "a$"], out: "0\n", status: 1, err: ""},
    {args: ["a", ":", "\\(b\\)*"], out: "\n", status: 1, err: ""},
    {args: ["abc", ":", "a\\(b\\)c"], out: "b\n", status: 0, err: ""},
    {args: ["a(", ":", "a("], out: "2\n", status: 0, err: ""},
    {args: ["_", ":", "a\\("], out: "", status: 2, err: "expr: Unmatched ( or \\(\n"},
    {args: ["_", ":", "a\\(b"], out: "", status: 2, err: "expr: Unmatched ( or \\(\n"},
    {args: ["a(b", ":", "a(b"], out: "3\n", status: 0, err: ""},
    {args: ["a)", ":", "a)"], out: "2\n", status: 0, err: ""},
    {args: ["_", ":", "a\\)"], out: "", status: 2, err: "expr: Unmatched ) or \\)\n"},
    {args: ["_", ":", "\\)"], out: "", status: 2, err: "expr: Unmatched ) or \\)\n"},
    {args: ["ab", ":", "a\\(\\)b"], out: "\n", status: 1, err: ""},
    {args: ["a^b", ":", "a^b"], out: "3\n", status: 0, err: ""},
    {args: ["a$b", ":", "a$b"], out: "3\n", status: 0, err: ""},
    {args: ["", ":", "\\($\\)\\(^\\)"], out: "\n", status: 1, err: ""},
    {args: ["b", ":", "a*\\(^b$\\)c*"], out: "b\n", status: 0, err: ""},
    {args: ["X|", ":", "X\\(|\\)", ":", "(", "X|", ":", "X\\(|\\)", ")"], out: "1\n", status: 0, err: ""},
    {args: ["X*", ":", "X\\(*\\)", ":", "(", "X*", ":", "X\\(*\\)", ")"], out: "1\n", status: 0, err: ""},
    {args: ["abc", ":", "\\(\\)"], out: "\n", status: 1, err: ""},
    {args: ["{1}a", ":", "\\(\\{1\\}a\\)"], out: "{1}a\n", status: 0, err: ""},
    {args: ["X*", ":", "X\\(*\\)", ":", "^*"], out: "1\n", status: 0, err: ""},
    {args: ["{1}", ":", "^\\{1\\}"], out: "3\n", status: 0, err: ""},
    {args: ["{", ":", "{"], out: "1\n", status: 0, err: ""},
    {args: ["abbcbd", ":", "a\\(b*\\)c\\1d"], out: "\n", status: 1, err: ""},
    {args: ["abbcbbbd", ":", "a\\(b*\\)c\\1d"], out: "\n", status: 1, err: ""},
    {args: ["abc", ":", "\\(.\\)\\1"], out: "\n", status: 1, err: ""},
    {args: ["abbccd", ":", "a\\(\\([bc]\\)\\2\\)*d"], out: "cc\n", status: 0, err: ""},
    {args: ["abbcbd", ":", "a\\(\\([bc]\\)\\2\\)*d"], out: "\n", status: 1, err: ""},
    {args: ["abbbd", ":", "a\\(\\(b\\)*\\2\\)*d"], out: "bbb\n", status: 0, err: ""},
    {args: ["aabcd", ":", "\\(a\\)\\1bcd"], out: "a\n", status: 0, err: ""},
    {args: ["aabcd", ":", "\\(a\\)\\1bc*d"], out: "a\n", status: 0, err: ""},
    {args: ["aabd", ":", "\\(a\\)\\1bc*d"], out: "a\n", status: 0, err: ""},
    {args: ["aabcccd", ":", "\\(a\\)\\1bc*d"], out: "a\n", status: 0, err: ""},
    {args: ["aabcccd", ":", "\\(a\\)\\1bc*[ce]d"], out: "a\n", status: 0, err: ""},
    {args: ["aabcccd", ":", "\\(a\\)\\1b\\(c\\)*cd$"], out: "a\n", status: 0, err: ""},
    {args: ["a*b", ":", "a\\(*\\)b"], out: "*\n", status: 0, err: ""},
    {args: ["ab", ":", "a\\(**\\)b"], out: "\n", status: 1, err: ""},
    {args: ["ab", ":", "a\\(***\\)b"], out: "\n", status: 1, err: ""},
    {args: ["*a", ":", "*a"], out: "2\n", status: 0, err: ""},
    {args: ["a", ":", "**a"], out: "1\n", status: 0, err: ""},
    {args: ["a", ":", "***a"], out: "1\n", status: 0, err: ""},
    {args: ["ab", ":", "a\\{1\\}b"], out: "2\n", status: 0, err: ""},
    {args: ["ab", ":", "a\\{1,\\}b"], out: "2\n", status: 0, err: ""},
    {args: ["aab", ":", "a\\{1,2\\}b"], out: "3\n", status: 0, err: ""},
    {args: ["_", ":", "a\\{1"], out: "", status: 2, err: "expr: Unmatched \\{\n"},
    {args: ["_", ":", "a\\{1a"], out: "", status: 2, err: "expr: Unmatched \\{\n"},
    {args: ["_", ":", "a\\{1a\\}"], out: "", status: 2, err: "expr: Invalid content of \\{\\}\n"},
    {args: ["a", ":", "a\\{,2\\}"], out: "1\n", status: 0, err: ""},
    {args: ["a", ":", "a\\{,\\}"], out: "1\n", status: 0, err: ""},
    {args: ["_", ":", "a\\{1,x\\}"], out: "", status: 2, err: "expr: Invalid content of \\{\\}\n"},
    {args: ["_", ":", "a\\{1,x"], out: "", status: 2, err: "expr: Unmatched \\{\n"},
    {args: ["_", ":", "a\\{32768\\}"], out: "", status: 2, err: "expr: Invalid content of \\{\\}\n"},
    {args: ["_", ":", "a\\{1,0\\}"], out: "", status: 2, err: "expr: Invalid content of \\{\\}\n"},
    {args: ["acabc", ":", ".*ab\\{0,0\\}c"], out: "2\n", status: 0, err: ""},
    {args: ["abcac", ":", "ab\\{0,1\\}c"], out: "3\n", status: 0, err: ""},
    {args: ["abbcac", ":", "ab\\{0,3\\}c"], out: "4\n", status: 0, err: ""},
    {args: ["abcac", ":", ".*ab\\{1,1\\}c"], out: "3\n", status: 0, err: ""},
    {args: ["abcac", ":", ".*ab\\{1,3\\}c"], out: "3\n", status: 0, err: ""},
    {args: ["abbcabc", ":", ".*ab\\{2,2\\}c"], out: "4\n", status: 0, err: ""},
    {args: ["abbcabc", ":", ".*ab\\{2,4\\}c"], out: "4\n", status: 0, err: ""},
    {args: ["aa", ":", "a\\{1\\}\\{1\\}"], out: "1\n", status: 0, err: ""},
    {args: ["aa", ":", "a*\\{1\\}"], out: "2\n", status: 0, err: ""},
    {args: ["aa", ":", "a\\{1\\}*"], out: "2\n", status: 0, err: ""},
    {args: ["acd", ":", "a\\(b\\)?c\\1d"], out: "\n", status: 1, err: ""},
    {args: ["--", "-5", ":", "-\\{0,1\\}[0-9]*$"], out: "2\n", status: 0, err: ""},
    {args: [], out: "", status: 2, err: "expr: missing operand\nTry 'expr --help' for more information.\n"},
    {args: ["98782897298723498732987928734", "+", "1"], out: "98782897298723498732987928735\n", status: 0, err: ""},
    {args: ["98782897298723498732987928734", "+", "98782897298723498732987928735"], out: "197565794597446997465975857469\n", status: 0, err: ""},
    {args: ["98782897298723498732987928735", "-", "1"], out: "98782897298723498732987928734\n", status: 0, err: ""},
    {args: ["197565794597446997465975857469", "-", "98782897298723498732987928734"], out: "98782897298723498732987928735\n", status: 0, err: ""},
    {args: ["98782897298723498732987928735", "*", "98782897298723498732987928734"], out: "9758060798730154302876482828124348356960410232492450771490\n", status: 0, err: ""},
    {args: ["9758060798730154302876482828124348356960410232492450771490", "/", "98782897298723498732987928734"], out: "98782897298723498732987928735\n", status: 0, err: ""},
    {args: ["9", "9"], out: "", status: 2, err: "expr: syntax error: unexpected argument '9'\n"},
    {args: ["2", "a"], out: "", status: 2, err: "expr: syntax error: unexpected argument 'a'\n"},
    {args: ["2", "+"], out: "", status: 2, err: "expr: syntax error: missing argument after '+'\n"},
    {args: ["2", ":"], out: "", status: 2, err: "expr: syntax error: missing argument after ':'\n"},
    {args: ["length"], out: "", status: 2, err: "expr: syntax error: missing argument after 'length'\n"},
    {args: ["(", "2"], out: "", status: 2, err: "expr: syntax error: expecting ')' after '2'\n"},
    {args: ["(", "2", "a"], out: "", status: 2, err: "expr: syntax error: expecting ')' instead of 'a'\n"},
  ]
  var failures = check_rows(s, rows, "C")?
  failures = failures.extend(check_rows(s, rows[..100], "fr_FR.UTF-8")?)
  assert failures.is_empty(), failures.join("\n")
}

# origin: gnu expr/expr-multibyte.log
test test_gnu_expr_expr_multibyte_log { |ctx|
  let s = uu.scene(ctx)?
  var failures: List[Str] = []
  for item in [
    {locale: "fr_FR.UTF-8", args: [b"length", b"abcdef"], out: b"6\x0a", status: 0},
    {locale: "C", args: [b"length", b"abcdef"], out: b"6\x0a", status: 0},
    {locale: "fr_FR.UTF-8", args: [b"length", b"\xce\xb1bcdef"], out: b"6\x0a", status: 0},
    {locale: "C", args: [b"length", b"\xce\xb1bcdef"], out: b"7\x0a", status: 0},
    {locale: "fr_FR.UTF-8", args: [b"length", b"abc\xce\xb4ef"], out: b"6\x0a", status: 0},
    {locale: "C", args: [b"length", b"abc\xce\xb4ef"], out: b"7\x0a", status: 0},
    {locale: "fr_FR.UTF-8", args: [b"length", b"fedcb\xce\xb1"], out: b"6\x0a", status: 0},
    {locale: "C", args: [b"length", b"fedcb\xce\xb1"], out: b"7\x0a", status: 0},
    {locale: "fr_FR.UTF-8", args: [b"length", b"\xb1aaa"], out: b"4\x0a", status: 0},
    {locale: "C", args: [b"length", b"\xb1aaa"], out: b"4\x0a", status: 0},
    {locale: "fr_FR.UTF-8", args: [b"length", b"aaa\xce"], out: b"4\x0a", status: 0},
    {locale: "C", args: [b"length", b"aaa\xce"], out: b"4\x0a", status: 0},
    {locale: "fr_FR.UTF-8", args: [b"length", b"\xe1\xbc\x94\xce\xba\xcf\x86\xcf\x81\xce\xb1\xcf\x83\xce\xb9\xcf\x82"], out: b"8\x0a", status: 0},
    {locale: "C", args: [b"length", b"\xe1\xbc\x94\xce\xba\xcf\x86\xcf\x81\xce\xb1\xcf\x83\xce\xb9\xcf\x82"], out: b"17\x0a", status: 0},
    {locale: "fr_FR.UTF-8", args: [b"index", b"abcdef", b"fb"], out: b"2\x0a", status: 0},
    {locale: "C", args: [b"index", b"abcdef", b"fb"], out: b"2\x0a", status: 0},
    {locale: "fr_FR.UTF-8", args: [b"index", b"\xce\xb1bc\xce\xb4ef", b"b"], out: b"2\x0a", status: 0},
    {locale: "C", args: [b"index", b"\xce\xb1bc\xce\xb4ef", b"b"], out: b"3\x0a", status: 0},
    {locale: "fr_FR.UTF-8", args: [b"index", b"\xce\xb1bc\xce\xb4ef", b"f"], out: b"6\x0a", status: 0},
    {locale: "C", args: [b"index", b"\xce\xb1bc\xce\xb4ef", b"f"], out: b"8\x0a", status: 0},
    {locale: "fr_FR.UTF-8", args: [b"index", b"\xce\xb1bc\xce\xb4ef", b"\xce\xb4"], out: b"4\x0a", status: 0},
    {locale: "C", args: [b"index", b"\xce\xb1bc\xce\xb4ef", b"\xce\xb4"], out: b"1\x0a", status: 0},
    {locale: "fr_FR.UTF-8", args: [b"index", b"\xcebc\xce\xb4ef", b"\xce\xb4"], out: b"4\x0a", status: 0},
    {locale: "C", args: [b"index", b"\xcebc\xce\xb4ef", b"\xce\xb4"], out: b"1\x0a", status: 0},
    {locale: "fr_FR.UTF-8", args: [b"index", b"\xce\xb1bc\xce\xb4ef", b"\xb4"], out: b"0\x0a", status: 1},
    {locale: "C", args: [b"index", b"\xce\xb1bc\xce\xb4ef", b"\xb4"], out: b"6\x0a", status: 0},
    {locale: "fr_FR.UTF-8", args: [b"index", b"\xce\xb1bc\xb4ef", b"\xb4"], out: b"4\x0a", status: 0},
    {locale: "fr_FR.UTF-8", args: [b"substr", b"abcdef", b"2", b"3"], out: b"bcd\x0a", status: 0},
    {locale: "C", args: [b"substr", b"abcdef", b"2", b"3"], out: b"bcd\x0a", status: 0},
    {locale: "fr_FR.UTF-8", args: [b"substr", b"\xce\xb1bc\xce\xb4ef", b"1", b"1"], out: b"\xce\xb1\x0a", status: 0},
    {locale: "C", args: [b"substr", b"\xce\xb1bc\xce\xb4ef", b"1", b"1"], out: b"\xce\x0a", status: 0},
    {locale: "fr_FR.UTF-8", args: [b"substr", b"\xce\xb1bc\xce\xb4ef", b"3", b"2"], out: b"c\xce\xb4\x0a", status: 0},
    {locale: "C", args: [b"substr", b"\xce\xb1bc\xce\xb4ef", b"3", b"2"], out: b"bc\x0a", status: 0},
    {locale: "fr_FR.UTF-8", args: [b"substr", b"\xce\xb1bc\xce\xb4ef", b"4", b"1"], out: b"\xce\xb4\x0a", status: 0},
    {locale: "C", args: [b"substr", b"\xce\xb1bc\xce\xb4ef", b"4", b"1"], out: b"c\x0a", status: 0},
    {locale: "fr_FR.UTF-8", args: [b"substr", b"\xce\xb1bc\xce\xb4ef", b"4", b"2"], out: b"\xce\xb4e\x0a", status: 0},
    {locale: "C", args: [b"substr", b"\xce\xb1bc\xce\xb4ef", b"4", b"2"], out: b"c\xce\x0a", status: 0},
    {locale: "fr_FR.UTF-8", args: [b"substr", b"\xce\xb1bc\xce\xb4ef", b"6", b"1"], out: b"f\x0a", status: 0},
    {locale: "C", args: [b"substr", b"\xce\xb1bc\xce\xb4ef", b"6", b"1"], out: b"\xb4\x0a", status: 0},
    {locale: "fr_FR.UTF-8", args: [b"substr", b"\xce\xb1bc\xce\xb4ef", b"7", b"1"], out: b"\x0a", status: 1},
    {locale: "C", args: [b"substr", b"\xce\xb1bc\xce\xb4ef", b"7", b"1"], out: b"e\x0a", status: 0},
    {locale: "fr_FR.UTF-8", args: [b"substr", b"\xce\xb1bc\xb4ef", b"3", b"3"], out: b"c\xb4e\x0a", status: 0},
    {locale: "C", args: [b"substr", b"\xce\xb1bc\xb4ef", b"3", b"3"], out: b"bc\xb4\x0a", status: 0},
    {locale: "fr_FR.UTF-8", args: [b"match", b"abcdef", b"ab"], out: b"2\x0a", status: 0},
    {locale: "C", args: [b"match", b"abcdef", b"ab"], out: b"2\x0a", status: 0},
    {locale: "fr_FR.UTF-8", args: [b"match", b"abcdef", b"\\(ab\\)"], out: b"ab\x0a", status: 0},
    {locale: "C", args: [b"match", b"abcdef", b"\\(ab\\)"], out: b"ab\x0a", status: 0},
    {locale: "fr_FR.UTF-8", args: [b"match", b"\xce\xb1bc\xce\xb4ef", b".bc"], out: b"3\x0a", status: 0},
    {locale: "C", args: [b"match", b"\xce\xb1bc\xce\xb4ef", b".bc"], out: b"0\x0a", status: 1},
    {locale: "fr_FR.UTF-8", args: [b"match", b"\xce\xb1bc\xce\xb4ef", b"..bc"], out: b"0\x0a", status: 1},
    {locale: "C", args: [b"match", b"\xce\xb1bc\xce\xb4ef", b"..bc"], out: b"4\x0a", status: 0},
    {locale: "fr_FR.UTF-8", args: [b"match", b"\xce\xb1bc\xce\xb4ef", b"\\(.b\\)c"], out: b"\xce\xb1b\x0a", status: 0},
    {locale: "C", args: [b"match", b"\xce\xb1bc\xce\xb4ef", b"\\(.b\\)c"], out: b"\x0a", status: 1},
    {locale: "fr_FR.UTF-8", args: [b"match", b"\xcebc\xce\xb4ef", b"\\(.\\)"], out: b"\x0a", status: 1},
    {locale: "C", args: [b"match", b"\xcebc\xce\xb4ef", b"\\(.\\)"], out: b"\xce\x0a", status: 0},
    {locale: "fr_FR.UTF-8", args: [b"match", b"\xce\xb1bc\xce\xb4e", b"\\([\xce\xb1]\\)"], out: b"\xce\xb1\x0a", status: 0},
    {locale: "C", args: [b"match", b"\xce\xb1bc\xce\xb4e", b"\\([\xce\xb1]\\)"], out: b"\xce\x0a", status: 0},
    {locale: "C.UTF-8", args: [b"length", b"abc\xce\xb4ef"], out: b"6\x0a", status: 0},
    {locale: "C.UTF-8", args: [b"index", b"\xce\xb1bc\xce\xb4ef", b"\xce\xb4"], out: b"4\x0a", status: 0},
    {locale: "C.UTF-8", args: [b"substr", b"\xce\xb1bc\xce\xb4ef", b"3", b"2"], out: b"c\xce\xb4\x0a", status: 0},
    {locale: "C.UTF-8", args: [b"match", b"\xce\xb1bc\xce\xb4ef", b".bc"], out: b"3\x0a", status: 0},
  ] {
    let r = uu.invoke_paths(s, "expr", [Path.parse_bytes(word)? for word in item.args], vars: {LC_ALL: item.locale, LANGUAGE: "C", LANG: "C"}, timeout: 5s)?
    if r.status != item.status or r.stdout != item.out or r.stderr != b"" {
      failures += [f"{item.locale} expr {[byte_text(word) for word in item.args].join(" / ")}: status {r.status} expected {item.status}; output bytes {byte_text(r.stdout)} expected {byte_text(item.out)}; stderr {byte_text(r.stderr)}"]
    }
  }
  assert failures.is_empty(), failures.join("\n")
}
