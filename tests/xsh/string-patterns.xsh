# An f-string in pattern position matches text of that shape and binds each
# hole to the text it took.

type Setting = {key: Str, value: Str}

pure count(text: Str, needle: Str) -> Int {
  text.split(needle).len() - 1
}

proc check_errors(ctx: TestContext, source: Str) [fs, process, env, error] -> Result[Str] {
  let output = test.run_script(ctx, source)?
  assert output.status == 2, f"{output.stdout}{output.stderr}"
  assert output.stdout == "", output.stdout
  output.stderr
}

pure setting(line: Str) -> Setting? {
  if let f"{key}={value}" = line {
    return {key, value}
  }

  null
}

pure describe(line: Str) -> Str {
  match line {
    f"#define {name} {body}" => f"define {name} [{body}]"
    f"#include <{header}>" => f"include {header}"
    f"{{{inner}}}" => f"braced {inner}"
    f"port {n:d}" => f"port {n + 1}"
    f"mask {bits:x} {perm:o} {flags:b}" => f"mask {bits} {perm} {flags}"
    f"load {one:f} {five:e} {fifteen:g}" => f"load {one + five + fifteen}"
    f"{word:s}!" => f"shout {word}"
    "exact" => "exact"
    else => "other"
  }
}

test test_text_pattern_binds_each_hole_to_the_shortest_text {
  assert setting("a=b=c") == {key: "a", value: "b=c"}
  assert setting("PATH=/bin:/usr/bin") == {key: "PATH", value: "/bin:/usr/bin"}
  assert setting("EMPTY=") == {key: "EMPTY", value: ""}
  assert setting("=only") == {key: "", value: "only"}
  assert setting("no separator") == null

  assert describe("#define MAX 4 + 4") == "define MAX [4 + 4]"
  assert describe("#include <stdio.h>") == "include stdio.h"
  assert describe("{x}") == "braced x"
  assert describe("exact") == "exact"
  assert describe("exactly") == "other"
}

# The literal that ends a pattern is the end of the text, so the last hole
# takes everything before it.
test test_text_pattern_matches_the_whole_text {
  let name = "a.txt.txt"
  if let f"{stem}.txt" = name {
    assert stem == "a.txt"
  } else {
    assert false, "the suffix is the end of the text"
  }

  assert name is f"{_}.txt"
  assert ! (name is f"{_}.tx")
  assert ! (name is f"b{_}")
  assert ! ("ab" is f"ab{_}ba")
  assert "abba" is f"ab{_}ba"
  assert "naïve café" is f"{_}ï{_} caf{_}"
}

test test_typed_holes_convert_or_do_not_match {
  assert describe("port 8080") == "port 8081"
  assert describe("port -007") == "port -6"
  assert describe("port 80a") == "other"
  assert describe("port ") == "other"
  assert describe("port 9223372036854775808") == "other"
  assert describe("mask ff 0o17 -0b101") == "mask 255 15 -5"
  assert describe("mask fg 17 1") == "other"
  assert describe("load 0.5 1e1 -2") == "load 8.5"
  assert describe("load 0.5 nan 2") == "other"
  assert describe("hey!") == "shout hey"
}

# A hole that fails to convert fails the arm; a later arm still sees the text.
test test_a_failed_conversion_moves_to_the_next_arm {
  let outcome = match "id=seven" {
    f"id={n:d}" => f"number {n}",
    f"id={text}" => f"text {text}",
    else => "none",
  }

  assert outcome == "text seven"
}

test test_text_pattern_binds_in_while_let_and_guard_let {
  var rest = "a,b,c"
  let seen = collect {
    while let f"{head},{tail}" = rest {
      yield head
      rest = tail
    }
  }

  assert seen == ["a", "b"]
  assert rest == "c"

  let found: Str? = "v1.2"
  let version = if let f"v{major:d}.{minor:d}" = found { major * 10 + minor } else { 0 }

  assert version == 12
}

test test_text_pattern_joins_alternatives_and_aliases {
  let label = match "KEY: value" {
    (f"{key}: {value}" | f"{key}={value}") as line => f"{key}/{value}/{line}",
    "" => "empty",
    else => "none",
  }

  assert label == "KEY/value/KEY: value"
}

test test_text_pattern_rejects_what_it_cannot_compile { |ctx|
  let stderr = check_errors(
    ctx,
    r"""pure probe(line: Str, count: Int) -> Str {
  if let f"{a}{b}" = line {
    return a + b
  }

  if let f"{n:>5}" = line {
    return n
  }

  if let f"{n:05d} {m:q}" = line {
    return f"{n}{m}"
  }

  if let f"{n:d}" = count {
    return f"{n}"
  }

  line
}

print ${probe("x", 1)}
""",
  )?
  # Every error is this one; a condition's pattern is reported per check of it.
  assert count(stderr, "err[check.text-pattern]") == count(stderr, "err["), stderr
  assert "two holes of a text pattern have no text between them" in stderr, stderr
  assert "unsupported spec `>5`" in stderr, stderr
  assert "unsupported spec `05d`" in stderr, stderr
  assert "unsupported spec `q`" in stderr, stderr
  assert "a text pattern matches a Str, but the value is Int" in stderr, stderr
}

test test_text_pattern_hole_is_a_name { |ctx|
  let stderr = check_errors(
    ctx,
    r"""let line = "a=b"
if let f"{line.trim()}={value}" = line {
  print $value
}
if let f"{}={value}" = line {
  print $value
}
""",
  )?
  assert "err[parse.text-pattern-hole]" in stderr, stderr
  assert "a hole binds the text it matches" in stderr, stderr
  assert ":2:11" in stderr, stderr
}

test test_text_pattern_binds_each_name_once_and_nothing_in_a_test { |ctx|
  let stderr = check_errors(
    ctx,
    r"""let line = "a-a"
if let f"{same}-{same}" = line {
  print $same
}
if line is f"{bound}-{_}" {
  print "bound"
}
""",
  )?
  assert "err[check.pattern-binding]" in stderr, stderr
  assert ":2:18" in stderr, stderr
  assert "err[check.pattern-test-binding]" in stderr, stderr
  assert ":5:15" in stderr, stderr
  assert "write the hole as `{_}`" in stderr, stderr
}

test test_text_pattern_never_covers_a_match { |ctx|
  let stderr = check_errors(
    ctx,
    r"""pure whole(line: Str) -> Str {
  match line {
    f"{all}" => all
  }
}

print ${whole("x")}
""",
  )?
  assert "err[check.match-value-exhaustive]" in stderr, stderr
}

test test_text_pattern_formats_as_written { |ctx|
  let source = r"""let line = "k=v\t1"
match   line {
  f"{key}={value}\t{n:d}"   =>   print $key $value $n
  f"{{{_}}}"=>print "braced"
  else => print "other"
}
"""
  let candidate = test.temp_file(ctx, name: "text.xsh", contents: bytes.from_text(source))?
  let formatted = run.capture --text "xsht" fmt $candidate
  assert formatted.status.exited_with(0), formatted.stderr
  let text = candidate.read_text()?
  assert "  f\"{key}={value}\\t{n:d}\" => print $key $value $n\n" in text, text
  assert "  f\"{{{_}}}\" => print \"braced\"\n" in text, text
  let stable = run.capture --text "xsht" fmt --check $candidate
  assert stable.status.exited_with(0), stable.stderr
  let output = test.expect(ctx, text, status: 0)?
  assert output.stdout == "k v 1\n"
}

# The opt-in lint notes text taken apart by position and rewrites nothing.
test test_lint_notes_text_taken_apart_by_position { |ctx|
  let source = r"""pure probe(line: Str) -> Str {
  let parts = line.split("=")
  let key = parts[0]
  let value = parts[1]
  if line.starts_with("#define ") {
    return line.byte_slice(8, line.byte_len() - 8)
  }

  key + value
}

print ${probe("a=b")}
"""
  let candidate = test.temp_file(ctx, name: "positions.xsh", contents: bytes.from_text(source))?
  let quiet = run.capture --text --accept=[0, 1] "xsht" lint $candidate
  assert "lint.prefer-text-pattern" not in quiet.stdout + quiet.stderr, quiet.stderr

  let linted = run.capture --text --accept=[0, 1] "xsht" lint --only lint.prefer-text-pattern $candidate
  let report = linted.stdout + linted.stderr
  assert linted.status.exited_with(0), report
  assert count(report, "note[lint.prefer-text-pattern]") == 2, report
  assert "if let f\"{a}={b}\" = ..." in report, report
  assert "if let f\"#define {rest}\" = line" in report, report

  let fixed = run.capture --text --accept=[0, 1] "xsht" lint --fix --only lint.prefer-text-pattern $candidate
  assert fixed.status.exited_with(0) or fixed.status.exited_with(1), fixed.stderr
  assert candidate.read_text()? == source
}
