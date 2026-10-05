# What `xsht check` reported for one program.
type Checked = {status: Status, stderr: Str}

# Runs only the checker over `source`: these programs read files that do not
# exist, or would fail at run time.
proc check(ctx: TestContext, source: Str) [fs, process, error] -> Result[Checked] {
  let file = test.temp_file(ctx, name: "checked.xsh", contents: bytes.from_text(source))?
  let checked = run.capture --text "xsht" check $file
  {status: checked.status, stderr: checked.stderr}
}

# Requires the checker to reject `source` with every diagnostic in `codes`.
proc expect_rejected(ctx: TestContext, source: Str, codes: List[Str]) [fs, process, error] {
  let checked = check(ctx, source)?
  assert checked.status.exited_with(2), f"{source}: {checked.stderr}"
  for code in codes {
    assert f"[{code}]" in checked.stderr, f"expected {code} for {source}: {checked.stderr}"
  }
}

# Requires the checker to accept `source`.
proc expect_accepted(ctx: TestContext, source: Str) [fs, process, error] {
  let checked = check(ctx, source)?
  assert checked.status.exited_with(0), f"{source}: {checked.stderr}"
}

# Requires the checker to report none of `codes` for `source`, which other
# diagnostics may still reject.
proc expect_free_of(ctx: TestContext, source: Str, codes: List[Str]) [fs, process, error] {
  let checked = check(ctx, source)?
  for code in codes {
    assert f"[{code}]" not in checked.stderr, f"unexpected {code} for {source}: {checked.stderr}"
  }
}

test test_checker_allows_plain_values_for_result_returns { |ctx|
  expect_free_of(
    ctx,
    r"""
pure label(value: Str) -> Result[Str] {
  value
}

pure returned(value: Str) -> Result[Str] {
  return value
}

proc path_value() -> Result[Path] {
  ./target/value
}

let one = label("ok")?
let two = returned("ok")?
let path = path_value()?
""",
    ["check.type-mismatch", "check.missing-return"],
  )
}

test test_checker_accepts_narrow_function_tail_values { |ctx|
  expect_free_of(
    ctx,
    r"""
pure object_path(src: Path) -> Path {
  src.with_ext("o")
}
proc wrap(path: Path) -> Result[Path] {
  Ok(path)
}
proc compile(src: Path) -> Result[Path] {
  let obj = object_path(src)
  wrap (obj)
}
pure empty_names() -> List[Str] {
  []
}
proc empty_result_names() -> Result[List[Str]] {
  []
}
let command = process.command {
  run true
}
""",
    ["check.missing-return", "check.type-mismatch", "check.ignored-result", "check.empty-list"],
  )
}

test test_checker_resolves_tail_bare_identifier_candidates { |ctx|
  expect_accepted(
    ctx,
    r"""
pure pure_tail(value: Str) -> Result[Str] {
  value
}
proc proc_tail(value: Str) -> Result[Str] {
  value
}
""",
  )
  expect_free_of(
    ctx,
    r"""
proc value() -> Result[Str] {
  Ok("proc")
}
proc choose(value: Str) -> Result[Str] {
  value
}
""",
    ["check.unresolved-proc-command", "check.type-mismatch"],
  )
}

test test_checker_rejects_ambiguous_or_non_result_tail_values { |ctx|
  let non_tail = check(ctx, "pure bad() -> Int {\n  1\n  2\n}\n")?
  assert non_tail.status.exited_with(2), non_tail.stderr
  assert "[check.ignored-result]" in non_tail.stderr, non_tail.stderr
  assert "ignored `Int` value" in non_tail.stderr, non_tail.stderr

  expect_rejected(
    ctx,
    "proc bad(value: Str) -> Result[Str] {\n  value\n  value\n}\n",
    ["check.unresolved-proc-command"],
  )
  expect_rejected(ctx, "pure bad() -> Result[Result[Int]] {\n  Ok(1)\n}\n", ["check.type-mismatch"])

  # No branch ends in a value, so the `if` is a statement and the body
  # can fall through, exactly as `return 1 when flag` does.
  expect_rejected(
    ctx,
    "pure bad(flag: Bool) -> Int {\n  if flag {\n    return 1\n  }\n}\n",
    ["check.missing-return"],
  )
}

test test_checker_accepts_float_literals_methods_json_and_schema_checks { |ctx|
  expect_accepted(
    ctx,
    r"""
type Metric = {ratio: Float, samples: List[Float]}

let ratio: Float = 1.5
let seconds = 250.float() / 1000.0
let rounded = seconds.round()?
let text = ratio.format(precision: 2)
let raw: Any = json.decode("{\"ratio\":1.5,\"samples\":[0.25,1.25]}")?
let metric = raw.require(Metric)?
let encoded = json.encode({ratio, seconds})?
""",
  )
}

test test_checker_rejects_mixed_numeric_float_operations { |ctx|
  expect_rejected(ctx, "let bad = 1 + 1.0\nlet also_bad = 1.0 < 2\n", ["check.type-mismatch"])
}

test test_checker_explains_unknown_methods_and_accepts_list_concatenation { |ctx|
  let checked = check(
    ctx,
    r"""
let text = "abc"
let bad_length = text.length()
let left: List[Str] = ["a"]
let right: List[Str] = ["b"]
let joined = left + right
""",
  )?
  assert "err[check.unknown-method]: unknown method `length` on Str" in checked.stderr, checked.stderr
  assert "count_chars" in checked.stderr, checked.stderr
  assert "byte_len" in checked.stderr, checked.stderr
  assert "[check.operator-type]" not in checked.stderr, checked.stderr
}

test test_checker_allows_nested_args_and_repeated_discard_bindings { |ctx|
  expect_free_of(
    ctx,
    r"""
let _ = 1
let _ = 2
proc local() {
  let args = "local"
  print $args
}
""",
    ["check.duplicate-name", "check.standard-module-shadow"],
  )
}

test test_checker_accepts_string_concatenation { |ctx|
  expect_accepted(ctx, "let x = \"a\" + \"b\"\n")
  expect_accepted(ctx, "let x = \"a\" + \"b\" + \"c\"\n")
}

test test_checker_rejects_string_concatenation_with_other_types { |ctx|
  expect_rejected(ctx, "let x = \"a\" + 1\n", ["check.type-mismatch"])
  expect_rejected(ctx, "let x = true + \"b\"\n", ["check.operator-type"])
}

test test_checker_accepts_str_parse_float_and_float_math_methods { |ctx|
  expect_accepted(ctx, "let n = \"3.14\".parse_float()?\n")
  expect_accepted(
    ctx,
    r"""
let a = 4.0.sqrt()
let b = 2.0.pow(3.0)
let c = 1.0.exp()
let d = 10.0.ln()
let e = 100.0.log(10.0)
let f = 0.0.sin()
let g = 0.0.cos()
let h = 0.0.tan()
let i = (-3.0).abs()
""",
  )
}

test test_checker_accepts_match_with_all_returning_arms_as_function_body { |ctx|
  for source in [
    r"""
enum Tok { TOp(Str), TEOF }

pure is_op(t: Tok, name: Str) -> Bool {
  match t {
    TOp(s) => return s == name
    _ => return false
  }
}
""",
    r"""
enum Kind { A, B, C }

pure kind_name(k: Kind) -> Str {
  match k {
    A => return "a"
    B => return "b"
    _ => return "other"
  }
}
""",
    r"""
enum Opt { Some(Int), None }

pure unwrap(o: Opt) -> Int {
  match o {
    Some(n) => return n
    None => return 0
  }
}
""",
  ] {
    expect_accepted(ctx, source)
  }
}

# Non-exhaustive match without catch-all: neither exhaustiveness nor the
# all-arms-return rule applies. The tail match must produce the declared Str,
# so the gap is the value-match error, reported once rather than also as the
# statement-match warning.
test test_checker_rejects_non_exhaustive_returning_match_without_catchall_once { |ctx|
  let checked = check(
    ctx,
    r"""
enum Kind { A, B, C }

pure kind_name(k: Kind) -> Str {
  match k {
    A => return "a"
    B => return "b"
  }
}
""",
  )?
  assert checked.status.exited_with(2), checked.stderr
  assert checked.stderr.split("[check.").len() == 2, checked.stderr
  assert "err[check.match-value-exhaustive]" in checked.stderr, checked.stderr
}

test test_checker_accepts_type_patterns_for_dynamic_values { |ctx|
  expect_accepted(
    ctx,
    r"""
pure label(value: Any) -> Result[Str] {
  match value {
    i is Int => return Ok(i.float().format(precision: 1))
    f is Float => return Ok(f.format(precision: 1))
    s is Str => return Ok(s)
    _ is Null => return Ok("null")
    _ => return Ok("other")
  }
}
""",
  )
}

test test_checker_rejects_type_patterns_for_concrete_values { |ctx|
  expect_rejected(
    ctx,
    r"""
let value = 1
match value {
  i is Int => { print ${i} }
}
""",
    ["check.pattern-type"],
  )
}

test test_checker_narrows_optional_record_membership_result_and_tag_flows { |ctx|
  expect_free_of(
    ctx,
    r"""
type Row = {name: Str}
enum State { Ready(Str), Stopped }

let maybe: Str? = "demo"
if maybe != null {
  let name: Str = maybe
}
if maybe == null {
  let fallback = "missing"
} else {
  let name: Str = maybe
}

let row: Row = {name: "demo"}
if "version" in row {
  let version: Any = row.version
}

let result: Result[Str] = Ok("demo")
match result {
  Ok(value) => {
    let name: Str = value
  }
  Err(err) => {
    let error: Error = err
  }
}

let state = Ready("demo")
match state {
  Ready(value) => {
    let name: Str = value
  }
  Stopped => {}
}
""",
    ["check.dynamic-boundary", "check.type-mismatch", "check.unknown-field", "check.pattern-type"],
  )
}

test test_checker_allows_local_mutation_inside_pure_functions { |ctx|
  expect_free_of(
    ctx,
    r"""
pure count_chars(value: Str) -> Int {
  var count = 0

  for ch in value.split("") {
    count += 1
  }

  return count
}

pure split_name(input: Record) -> Str {
  var {name} = input
  name = name.trim()
  return name
}
""",
    ["check.pure-var", "check.pure-assignment", "check.assign-let", "check.type-mismatch"],
  )
}

test test_checker_rejects_nonlocal_assignment_inside_pure_functions { |ctx|
  expect_rejected(
    ctx,
    r"""
var counter = 0

pure bump() -> Int {
  counter += 1
  return counter
}
""",
    ["check.pure-assignment"],
  )
  expect_rejected(
    ctx,
    r"""
pure trim_name(name: Str) -> Str {
  name = name.trim()
  return name
}
""",
    ["check.pure-assignment", "check.assign-let"],
  )
}

# `xsht check` reveals types; a malformed call is rejected and reveals nothing.
test test_checker_rejects_reveal_type_bad_argument_shapes { |ctx|
  for case in [
    {source: "reveal_type()\n", code: "check.arity"},
    {source: "reveal_type(1, 2)\n", code: "check.arity"},
    {source: "reveal_type(value: 1)\n", code: "check.named-arg"},
    {source: "let xs = [1]\nreveal_type(@xs)\n", code: "check.call-splice"},
  ] {
    let checked = check(ctx, case.source)?
    assert checked.status.exited_with(2), f"{case.source}: {checked.stderr}"
    assert f"[{case.code}]" in checked.stderr, f"{case.source}: {checked.stderr}"
    assert "revealed type" not in checked.stderr, f"{case.source}: {checked.stderr}"
  }
}
