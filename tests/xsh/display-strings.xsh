proc widen(width: Int) -> Int {
  width * 2
}

test test_display_strings_parse_each_interpolation_as_an_expression {
  let name = "demo"
  let m = {"k-1": "v"}
  let text = "abcd"
  let ok = true
  let x = 7
  assert f"<{f"{name}!"}>" == "<demo!>"
  assert f"<{f"[{f"{x}!"}]"}>" == "<[7!]>"
  assert f"<{ {a: 1}.a }>" == "<1>"
  assert f"<{m["k-1"]}>" == "<v>"
  assert f"<{widen(width: 4)}>" == "<8>"
  assert f"<{text[1..3]}>" == "<bc>"
  assert f"[{x:>4}] [{widen(width: 4)}] [{x:<4}] [{x:04}] [{-x:04}]" == "[   7] [8] [7   ] [0007] [-007]"
  assert f"<{if ok { "a" } else { "b" }}>" == "<a>"
  assert f"<{if ok { "}" } else { "{" }}>" == "<}>"
}

test test_display_strings_escape_braces_and_keep_dollars_and_hashes_as_text {
  let x = 7
  assert f"{{" == "{"
  assert f"}}" == "}"
  assert f"{{{x}}}" == "{7}"
  assert f"{{y}}" == "{y}"
  assert f"{x}{{}}{x}" == "7{}7"
  assert f"costs $5" == "costs $5"
  assert f"${{y}}" == "\${y}"
  assert f"\$x" == r"$x"
  assert f"# {x + 1} #" == "# 8 #"
  assert f"café {x}" == "café 7"
}

test test_display_block_strings_allow_braces_and_breaks_on_indented_lines {
  let x = 7
  let block = f"""
    {{
      value: {x}
    }}
    {if x > 1 {
      "big"
    } else { "small" }}
    """
  assert block == """{
  value: 7
}
big"""
}

test test_path_display_strings_join_native_path_bytes {
  let raw = b"raw\xff" as Path
  assert fp"{{{raw}}}/{1}" == b"{raw\xff}/1" as Path
}

proc assert_rejected(ctx: TestContext, source: Str, code: Str, location: Str) [fs, process, error] {
  let output = test.run_script(ctx, source)?
  assert ! output.success, source
  assert code in output.stderr, output.stderr
  assert location in output.stderr, output.stderr
}

test test_display_strings_reject_malformed_interpolations_at_exact_columns { |ctx|
  assert_rejected(
    ctx,
    """print f"a } b"
""",
    "parse.fmt-lone-brace",
    ":1:11",
  )
  assert_rejected(
    ctx,
    """print f"a {x"
""",
    "parse.unterminated-interpolation",
    ":1:11",
  )
  assert_rejected(
    ctx,
    """print f"a {  } b"
""",
    "parse.fmt-empty-interpolation",
    ":1:11",
  )
  assert_rejected(
    ctx,
    """let x = 1
print f"{x # note}"
""",
    "parse.fmt-interpolation-comment",
    ":2:12",
  )
  assert_rejected(
    ctx,
    """let x = 1
print f"{x +
  1}"
""",
    "parse.fmt-interpolation-line-break",
    ":2:13",
  )
  assert_rejected(
    ctx,
    """let x = 1
print f"{x:4}"
""",
    "parse.fmt-spec",
    ":2:11",
  )
  assert_rejected(
    ctx,
    """let x = 1
print f"{x y}"
""",
    "parse.fmt-interpolation-trailing",
    ":2:12",
  )
  assert_rejected(
    ctx,
    """print f"{{a: 1}.a}"
""",
    "needs a space",
    ":1:15",
  )
  assert_rejected(
    ctx,
    """print f"é ü {nope}"
""",
    "check.unresolved-name",
    ":1:14",
  )
  assert_rejected(
    ctx,
    """let v = f\"""
  é {{ {nope}
  \"""
""",
    "check.unresolved-name",
    ":2:9",
  )
}

test test_display_strings_reject_shell_style_dollar_interpolation { |ctx|
  let output = test.run_script(
    ctx,
    """let x = 1
print f"\${x}"
""",
  )?
  assert ! output.success
  assert "parse.fmt-dollar-interpolation" in output.stderr, output.stderr
  assert "write `{`" in output.stderr, output.stderr
  let named = test.run_script(
    ctx,
    """let name = "n"
print f"hi $name"
""",
  )?
  assert ! named.success
  assert "check.fmt-dollar-name" in named.stderr, named.stderr
  assert ":2:12" in named.stderr, named.stderr
  assert "interpolate with `{name}`" in named.stderr, named.stderr
  let unbound = test.expect(
    ctx,
    """print f"$HOME_UNBOUND"
""",
    status: 0,
  )?
  assert unbound.stdout == """$HOME_UNBOUND
"""
}

test test_display_strings_round_trip_through_the_formatter { |ctx|
  let source = """let x = 7
let m = {"k": "v"}
print f"{ {a: 1}.a } {m["k"]} {{x}} {x:>4} {f"{x}"} $5 \\$x"
print fp"/tmp/{{{x}}}"
print f"{({a: 2}).a}"
"""
  let expected = """let x = 7
let m = {"k": "v"}
print f"{ {a: 1}.a } {m["k"]} {{x}} {x:>4} {f"{x}"} $5 \\$x"
print fp"/tmp/{{{x}}}"
print f"{ {a: 2}.a }"
"""
  let before = test.expect(ctx, source, status: 0)?
  assert before.stdout == """1 v {x}    7 7 $5 $x
/tmp/{7}
2
"""
  let candidate = test.temp_file(ctx, name: "display-strings.xsh", contents: bytes.from_text(source))?
  let formatted = run.capture --text "xsht" fmt $candidate
  assert formatted.status.exited_with(0), formatted.stderr
  assert candidate.read_text()? == expected
  let stable = run.capture --text "xsht" fmt --check $candidate
  assert stable.status.exited_with(0), stable.stderr
  let linted = run.capture --text "xsht" lint $candidate
  let report = linted.stdout + linted.stderr
  assert "err[" not in report, report
  assert "redundant-parens" not in report, report
  assert "missing-f-prefix" not in report, report
}

test test_missing_f_prefix_lint_adds_the_prefix { |ctx|
  let source = """let name = "n"
let dir = p"/tmp"
let called = "hello {name.len()}"
let plain = "hello {name}"
let joined = p"{dir}/x"
let braces = "{{ name }} {unbound}"
print $called $plain $joined $braces
"""
  let candidate = test.temp_file(ctx, name: "missing-f.xsh", contents: bytes.from_text(source))?
  let linted = run.capture --text "xsht" lint $candidate
  let report = linted.stdout + linted.stderr
  assert report.split("[lint.missing-f-prefix]").len() == 3, report
  let fixed = run.capture --text "xsht" lint --fix $candidate
  assert fixed.status.exited_with(0), fixed.stdout + fixed.stderr
  let text = candidate.read_text()?
  assert "let plain = f\"hello {name}\"" in text, text
  assert "let joined = fp\"{dir}/x\"" in text, text
  assert "called = \"hello {name.len()}\"" in text, text
  assert "braces = \"{{ name }} {unbound}\"" in text, text
}
