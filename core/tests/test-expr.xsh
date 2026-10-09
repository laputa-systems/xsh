type Ran = {status: Int, stdout: Str, stderr: Str}

# Runs core/expr.xsh by its real path (so the invoked name is `expr` and
# `lib.gnu` resolves beside it), capturing both streams to files.
proc expr_run(ctx: TestContext, args: List[Str], vars: Record = {LC_ALL: "C"}) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "expr")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/expr.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, vars, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

proc expr_out(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Str] {
  Ok(expr_run(ctx, args)?.stdout)
}

proc expr_status(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Int] {
  Ok(expr_run(ctx, args)?.status)
}

test test_expr_values_and_exit_status { |ctx|
  assert expr_out(ctx, ["1"])? == "1\n"
  assert expr_status(ctx, ["1"])? == 0

  for word in ["", "0", "00", "-0"] {
    let result = expr_run(ctx, [word])?
    assert result.status == 1, f"expr '{word}'"
    assert result.stdout == f"{word}\n"
  }
}

test test_expr_arithmetic_is_exact_and_unbounded { |ctx|
  assert expr_out(ctx, ["1", "+", "1"])? == "2\n"
  assert expr_out(ctx, ["7", "*", "6"])? == "42\n"
  assert expr_out(ctx, ["-7", "/", "2"])? == "-3\n", "division truncates toward zero"
  assert expr_out(ctx, ["-7", "%", "2"])? == "-1\n", "the remainder follows the dividend"
  assert expr_out(ctx, ["9223372036854775807", "+", "9223372036854775807"])? == "18446744073709551614\n"
  assert expr_out(ctx, ["98782897298723498732987928735", "*", "98782897298723498732987928734"])? == "9758060798730154302876482828124348356960410232492450771490\n"
  assert expr_out(ctx, ["9758060798730154302876482828124348356960410232492450771490", "/", "98782897298723498732987928734"])? == "98782897298723498732987928735\n"
  assert expr_out(ctx, ["92233720368547758076549841651981984981498415651", "%", "922337203685"])? == "533691697086\n"
  assert expr_out(ctx, ["0", "-", "9223372036854775808"])? == "-9223372036854775808\n"
  assert expr_out(ctx, ["2", "*", "+", "3"])? == "6\n", "+ quotes the next word"
}

test test_expr_precedence_and_parentheses { |ctx|
  assert expr_out(ctx, ["2", "+", "3", "*", "4"])? == "14\n"
  assert expr_out(ctx, ["(", "2", "+", "3", ")", "*", "4"])? == "20\n"
  assert expr_out(ctx, ["18", "/", "(", "120", "%", "7", ")", "-", "6"])? == "12\n"
  assert expr_out(ctx, ["10", "-", "3", "-", "2"])? == "5\n", "operators associate to the left"
  assert expr_out(ctx, ["1a", "<", "1", "+", "1"])? == "1\n", "+ binds tighter than a comparison"
  assert expr_out(ctx, ["length", "abcd", "=", "4"])? == "1\n", "keyword operands are single words"
}

test test_expr_deep_and_long_expressions { |ctx|
  let nested = ["(" for n in range(3000)].extend(["1"]).extend([")" for n in range(3000)])
  assert expr_out(ctx, nested)? == "1\n"

  let lengths = ["length" for n in range(3000)].extend(["1"])
  assert expr_out(ctx, lengths)? == "1\n"

  var chain = ["1"]

  repeat 3000 times {
    chain += ["+", "1"]
  }

  assert expr_out(ctx, chain)? == "3001\n"
}

test test_expr_comparisons_are_numeric_only_for_integers { |ctx|
  assert expr_out(ctx, ["10", ">", "9"])? == "1\n"
  assert expr_out(ctx, ["10a", ">", "9"])? == "0\n", "a non-integer makes the comparison a string one"
  assert expr_out(ctx, ["-2417851639229258349412352", "<", "2417851639229258349412352"])? == "1\n"
  assert expr_out(ctx, ["abc", "!=", "abd"])? == "1\n"
  assert expr_out(ctx, ["abc", "<=", "abc"])? == "1\n"
  assert expr_out(ctx, ["007", "=", "7"])? == "1\n"
  assert expr_out(ctx, ["00", "<", "0!"])? == "0\n"
}

test test_expr_or_and_short_circuit { |ctx|
  assert expr_out(ctx, ["0", "|", "foo"])? == "foo\n"
  assert expr_out(ctx, ["foo", "|", "bar"])? == "foo\n"
  assert expr_out(ctx, ["", "|", "-0"])? == "0\n", "two null operands give 0"
  assert expr_out(ctx, ["foo", "&", "1"])? == "foo\n"
  assert expr_out(ctx, ["0", "&", "1"])? == "0\n"
  assert expr_out(ctx, ["1", "|", "1", "/", "0"])? == "1\n", "an unneeded branch is never evaluated"
  assert expr_out(ctx, ["0", "&", "(", "1", "/", "0", ")"])? == "0\n"

  let needed = expr_run(ctx, ["0", "|", "1", "/", "0"])?
  assert needed.status == 2
  assert needed.stderr == "expr: division by zero\n", needed.stderr
}

test test_expr_string_functions { |ctx|
  assert expr_out(ctx, ["length", "abcdef"])? == "6\n"
  assert expr_out(ctx, ["substr", "abcdef", "2", "3"])? == "bcd\n"
  assert expr_out(ctx, ["substr", "abc", "1", "18446744073709551615"])? == "abc\n"
  assert expr_out(ctx, ["substr", "abc", "0", "1"])? == "\n"
  assert expr_status(ctx, ["substr", "abc", "0", "1"])? == 1
  assert expr_out(ctx, ["index", "abcdef", "fb"])? == "2\n"
  assert expr_out(ctx, ["index", "abc", "z"])? == "0\n"
  assert expr_out(ctx, ["match", "abcd", "ab\\(.*\\)"])? == "cd\n"
}

test test_expr_regular_expressions_use_gnu_basic_syntax { |ctx|
  for case in [
    {subject: "abc", pattern: "a\\(b\\)c", result: "b"},
    {subject: "abc", pattern: "^abc", result: "3"},
    {subject: "abc", pattern: "bc", result: "0"},
    {subject: "a^b", pattern: "a^b", result: "3"},
    {subject: "a$b", pattern: "a$b", result: "3"},
    {subject: "ab", pattern: "a\\|ab", result: "2"},
    {subject: "aaa", pattern: "a\\{2\\}", result: "2"},
    {subject: "aaa", pattern: "a\\{2,\\}", result: "3"},
    {subject: "aaaa", pattern: "a\\{,\\}", result: "4"},
    {subject: "a+", pattern: "a+", result: "2"},
    {subject: "aaa", pattern: "a\\+", result: "3"},
    {subject: "*a", pattern: "*a", result: "2"},
    {subject: "x*", pattern: "x\\(*\\)", result: "*"},
    {subject: "{1}a", pattern: "\\(\\{1\\}a\\)", result: "{1}a"},
    {subject: "aabcd", pattern: "\\(a\\)\\1bcd", result: "a"},
    {subject: "abbccd", pattern: "a\\(\\([bc]\\)\\2\\)*d", result: "cc"},
    {subject: "abc", pattern: "\\(.\\)\\1", result: ""},
    {subject: "a", pattern: "\\(b\\)*", result: ""},
    {subject: "line1\nline2\nline3 ", pattern: ".*line2.*", result: "18"},
    {subject: "-5", pattern: "-\\{0,1\\}[0-9]*$", result: "2"},
    {subject: "Ab1", pattern: "[[:upper:]][[:lower:]][[:digit:]]", result: "3"},
    {subject: "a]", pattern: "[]a]*", result: "2"},
  ] {
    let result = expr_out(ctx, [case.subject, ":", case.pattern])?
    assert result == f"{case.result}\n", f"expr {case.subject} : {case.pattern}: {result}"
  }

  let slow = expr_run(ctx, ["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaac", ":", "\\(a\\+a\\+\\)\\+b"])?
  assert slow.status == 1
  assert slow.stdout == "\n", "a failing match with nested repeats finishes at once"
}

test test_expr_regular_expression_errors { |ctx|
  for case in [
    {pattern: "a\\(", message: "Unmatched ( or \\("},
    {pattern: "a\\)", message: "Unmatched ) or \\)"},
    {pattern: "a\\{1", message: "Unmatched \\{"},
    {pattern: "a\\{1a\\}", message: "Invalid content of \\{\\}"},
    {pattern: "a\\{1,0\\}", message: "Invalid content of \\{\\}"},
    {pattern: "a\\{32768\\}", message: "Regular expression too big"},
    {pattern: "\\", message: "Trailing backslash"},
    {pattern: "\\1", message: "Invalid back reference"},
    {pattern: "[a", message: "Unmatched [, [^, [:, [., or [="},
    {pattern: "[b-a]", message: "Invalid range end"},
  ] {
    let result = expr_run(ctx, ["_", ":", case.pattern])?
    assert result.status == 2, f"expr _ : {case.pattern}"
    assert result.stdout == ""
    assert result.stderr == f"expr: {case.message}\n", result.stderr
  }
}

test test_expr_syntax_and_operand_errors { |ctx|
  for case in [
    {args: ["9", "9"], message: "syntax error: unexpected argument '9'"},
    {args: ["2", "+"], message: "syntax error: missing argument after '+'"},
    {args: ["length"], message: "syntax error: missing argument after 'length'"},
    {args: ["(", "2"], message: "syntax error: expecting ')' after '2'"},
    {args: ["(", "2", "a"], message: "syntax error: expecting ')' instead of 'a'"},
    {args: ["(", "1", "/", "0"], message: "syntax error: expecting ')' after '0'"},
    {args: ["1", "(", ")"], message: "syntax error: unexpected argument '('"},
    {args: ["3", "+", "-"], message: "non-integer argument"},
    {args: ["a", "=", "2", "+", "a"], message: "non-integer argument"},
    {args: ["9", "/", "0"], message: "division by zero"},
    {args: ["substr", "abc", "x", "1"], message: "non-integer argument"},
  ] {
    let result = expr_run(ctx, case.args)?
    assert result.status == 2, f"expr {case.args.join(" ")}"
    assert result.stdout == ""
    assert result.stderr == f"expr: {case.message}\n", result.stderr
  }

  let none = expr_run(ctx, [])?
  assert none.status == 2
  assert none.stderr == "expr: missing operand\nTry 'expr --help' for more information.\n", none.stderr
}

test test_expr_string_functions_follow_the_locale { |ctx|
  let utf8 = {LC_ALL: "en_US.UTF-8"}
  let bytes_locale = {LC_ALL: "C"}
  let word = "αbcδef"

  assert expr_run(ctx, ["length", word], utf8)?.stdout == "6\n"
  assert expr_run(ctx, ["length", word], bytes_locale)?.stdout == "8\n", "a single-byte locale counts bytes"
  assert expr_run(ctx, ["index", word, "δ"], utf8)?.stdout == "4\n"
  assert expr_run(ctx, ["substr", word, "3", "2"], utf8)?.stdout == "cδ\n"
  assert expr_run(ctx, ["substr", word, "3", "2"], bytes_locale)?.stdout == "bc\n"
  assert expr_run(ctx, ["match", word, ".bc"], utf8)?.stdout == "3\n"
  assert expr_run(ctx, ["match", word, ".bc"], bytes_locale)?.stdout == "0\n"
  assert expr_run(ctx, ["50n", ">", "-51"], bytes_locale)?.stdout == "1\n"
  assert expr_run(ctx, ["50n", ">", "-51"], utf8)?.stdout == "0\n", "outside the C locale punctuation is ignored when ordering"
}

test test_expr_help_and_version { |ctx|
  let help = expr_run(ctx, ["--help"])?
  assert help.status == 0
  assert help.stdout.starts_with("Usage: expr EXPRESSION")

  let version = expr_run(ctx, ["--version"])?
  assert version.status == 0
  assert version.stdout.starts_with("expr")
}

test test_expr_write_failure_uses_status_3 { |ctx|
  if ! p"/dev/full".exists()? {
    test.skip("/dev/full is not available")
  }

  let root = test.temp_dir(ctx, name: "expr-full")?
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/expr.xsh"
  let argv = [ctx.xsh_bin.display(), script.display(), "2", "+", "2"]
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", p"/dev/full", err)
  let status = process.run(plan)?

  assert status.exit_code()? == 3
  assert err.read_text()? == "expr: No space left on device\n"
}
