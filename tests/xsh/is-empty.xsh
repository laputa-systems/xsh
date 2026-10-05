# `is_empty()` asks whether a `Str`, `Bytes`, `List`, or `Map` has no
# elements, in place of comparing a length with zero.

pure count(text: Str, needle: Str) -> Int {
  text.split(needle).len() - 1
}

pure emptiness(text: Str, raw: Bytes, items: List[Int], table: Map[Str, Int]) -> List[Bool] {
  [text.is_empty(), raw.is_empty(), items.is_empty(), table.is_empty()]
}

test test_is_empty_holds_exactly_for_values_without_elements {
  assert emptiness("", b"", [], {}) == [true, true, true, true]
  assert emptiness(" ", b"\0", [0], {a: 0}) == [false, false, false, false]

  let tail = [1][1..]
  assert tail.is_empty()
  assert "a,b".split(",")[2..].is_empty()
  assert ! "a,b".split(",").is_empty()
  assert " \n".trim().is_empty()
  assert b"abc"[3..].is_empty()
  assert [n for n in [1, 2] if n > 5].is_empty()
}

test test_is_empty_follows_a_null_safe_hop {
  let absent: List[Int]? = null
  let present: Str? = ""
  assert absent?.is_empty() == null
  assert present?.is_empty() == true
  assert absent?.is_empty() ?? true
}

test test_is_empty_is_not_defined_for_other_receivers { |ctx|
  let output = test.expect(
    ctx,
    """type Entry = {name: Str}
let entry: Entry = {name: "a"}
print \${entry.is_empty()} \${p"x".is_empty()} \${3.is_empty()}
""",
    status: 2,
  )?
  assert output.stdout == ""
  assert count(output.stderr, "err[check.unknown-method]") == 3, output.stderr
}

# The migration lint rewrites each comparison of a length with zero, and the
# result is formatted, checks, and behaves the same.
test test_lint_rewrites_lengths_compared_with_zero { |ctx|
  let source = """proc show(text: Str, raw: Bytes, items: List[Int], table: Map[Str, Int]) [io] {
  if items.len() == 0 or 0 == raw.len() {
    print "empty"
  }

  if table.len() != 0 and items.len() > 0 and 0 < text.byte_len() {
    print "full"
  }

  let blank = !(text.trim().count_chars() > 0)
  let single = items.len() == 1
  let agree = (items.len() > 0) == single
  print \$blank \$single \$agree
}

show("", b"", [], {})
show(" x ", b"x", [1], {a: 1})
"""
  let before = test.expect(ctx, source, status: 0)?
  let candidate = test.temp_file(ctx, name: "lengths.xsh", contents: bytes.from_text(source))?

  let linted = run.capture --text --accept=[0, 1] "xsht" lint --only lint.prefer-is-empty $candidate
  let report = linted.stdout + linted.stderr
  assert count(report, "warn[lint.prefer-is-empty]") == 7, report

  let fixed = run.capture --text "xsht" lint --fix --only lint.prefer-is-empty $candidate
  assert fixed.status.exited_with(0), fixed.stderr
  let rewritten = candidate.read_text()?
  assert "  if items.is_empty() or raw.is_empty() {\n" in rewritten, rewritten
  assert "  if ! table.is_empty() and ! items.is_empty() and ! text.is_empty() {\n" in rewritten, rewritten
  assert "  let blank = text.trim().is_empty()\n" in rewritten, rewritten
  assert "  let agree = ! items.is_empty() == single\n" in rewritten, rewritten
  assert "  let single = items.len() == 1\n" in rewritten, rewritten

  let stable = run.capture --text "xsht" fmt --check $candidate
  assert stable.status.exited_with(0), stable.stderr
  let after = test.expect(ctx, rewritten, status: 0)?
  assert after.stdout == before.stdout
  assert after.stdout == "empty\ntrue false true\nfull\nfalse true true\n"
}
