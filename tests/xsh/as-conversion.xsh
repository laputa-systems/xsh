# `value as T` converts a value to another type or fails. Each accepted pair
# of types is an operation the language already has, under `?`.

type Endpoint = {host: Str, port: UInt}

pure count(text: Str, needle: Str) -> Int {
  text.split(needle).len() - 1
}

proc check_errors(ctx: TestContext, source: Str) [fs, process, env, error] -> Result[Str] {
  let output = test.run_script(ctx, source)?
  assert output.status == 2, f"{output.stdout}{output.stderr}"
  assert output.stdout == "", output.stdout
  output.stderr
}

pure endpoint(spec: Str) -> Result[Endpoint] {
  let fields = spec.split(":")
  Ok({host: fields[0], port: fields[1] as UInt})
}

pure message(outcome: Result[Int]) -> Str {
  match outcome {
    Ok(value) => f"ok {value}"
    Err(problem) => problem.message
  }
}

test test_as_converts_every_pair_of_the_table {
  assert "42" as Int == 42
  assert " -0x10 " as Int == -16
  assert "1_000" as Int == 1000
  assert "007" as UInt == 7
  assert "1.5" as Float == 1.5
  assert "1e3" as Float == 1000.0
  assert "a/../b" as Path == p"a/../b"

  let raw = bytes.from_text("naïve")
  assert raw as Str == "naïve"
  assert b"usr/lib" as Path == p"usr/lib"

  let offset = 7
  let sizes: List[UInt] = [offset as UInt]
  assert sizes[0] == 7
}

# A conversion is its operation under `?`: the same value, and the same
# error when it fails.
test test_as_fails_with_the_error_of_its_operation {
  assert message(try { "4x" as Int }) == message("4x".parse_int())
  assert message(try { "-4" as UInt }) == message("-4".parse_uint())
  assert message(try { "" as Int }) == message("".parse_int())
  assert try { "one" as Float } is Err(_)
  assert try { "a\0b" as Path } is Err(_)
  assert try { b"a\0b" as Path } is Err(_)
  assert try { b"\xff" as Str } is Err(_)

  let below = -1
  assert try { (below as UInt) } is Err(_)
}

test test_as_propagates_out_of_a_result_function {
  assert endpoint("localhost:8080")? == {host: "localhost", port: 8080}
  let refused = endpoint("localhost:http")
  assert refused is Err(_)
  assert message(try { endpoint("localhost:http")?.port }) == message("http".parse_uint())
}

test test_as_binds_tighter_than_operators_and_looser_than_a_prefix {
  let field = "3"
  assert 2 * field as Int + 1 == 7
  assert field as Int * 2 == 6
  assert -(field as Int) == -3
  assert field as Int as UInt == 3
  assert (field as Int).float() == 3.0
  assert field as Int < 4
  assert field as Int in [3, 4]
  assert [field as Int, 1] == [3, 1]

  let debt = 5
  assert try { -debt as UInt } is Err(_)
}

# The word stays an ordinary name wherever no conversion is written.
test test_as_remains_a_name {
  let as = "21"
  let doubled = as as Int * 2
  assert doubled == 42
}

test test_as_reads_as_a_conversion_in_a_match_subject_and_before_is {
  let field = "7"
  let label = match field as Int {
    (7 | 8) as lucky => f"lucky {lucky}",
    0 => "none",
    else => "plain",
  }

  assert label == "lucky 7"
  assert field as Int is 7
  assert ! (field as Int is 8)
}

# The destination of `atomically replace` ends at the `as` that stands before
# a name and `{`; an earlier `as` converts.
test test_as_converts_inside_an_atomically_replace_destination { |ctx|
  let dir = test.temp_dir(ctx)?
  let target = f"{dir}/out.txt"
  atomically replace target as Path as staged {
    staged.write("written\n")
  }

  assert (target as Path).read_text()? == "written\n"
}

# After `is`, `as` belongs to the pattern, and a test binds nothing.
test test_as_after_a_pattern_test_is_the_pattern_alias { |ctx|
  let stderr = check_errors(
    ctx,
    r"""let field: Any = "7"
if field is Str as Int {
  print "text"
}
""",
  )?
  assert "err[check.pattern-test-binding]" in stderr, stderr
  assert "err[check.conversion]" not in stderr, stderr
}

test test_as_rejects_pairs_outside_the_table { |ctx|
  let stderr = check_errors(
    ctx,
    r"""let ratio = 2.5
let count = 3
let dynamic: Any = "4"
let maybe: Str? = "5"
let a = ratio as Int
let b = count as Float
let c = count as Int
let d = p"x" as Str
let e = dynamic as Int
let f = maybe as Int
let g = "6" as Int?
let h = "7" as Bool
let i = "8".parse_int() as Int
print $a $b $c $d $e $f $g $h $i
""",
  )?
  assert count(stderr, "err[check.conversion]") == 9, stderr
  assert count(stderr, "err[") == 9, stderr
  assert "no conversion from Float to Int" in stderr, stderr
  assert "`.floor()`, `.ceil()`, or `.round()`" in stderr, stderr
  assert "the value already has this type" in stderr, stderr
  assert "`.require(TYPE)`" in stderr, stderr
  assert "`T?` is the optional type" in stderr, stderr
}

test test_as_needs_somewhere_to_propagate { |ctx|
  let stderr = check_errors(
    ctx,
    r"""pure width(field: Str) -> Int {
  let columns = field as Int
  columns * 2
}

proc quiet(field: Str) [io] -> Int {
  let columns = field as Int
  print $columns
  columns
}

print ${width("4")} ${quiet("4")}
""",
  )?
  assert count(stderr, "err[check.try-context]") == 2, stderr
  assert count(stderr, "err[check.effect-violation]") == 1, stderr
  assert count(stderr, "`as` fails the enclosing function") == 3, stderr
  assert ":2:17" in stderr, stderr
}

test test_as_fails_the_script_at_top_level { |ctx|
  let output = test.expect(
    ctx,
    r"""print "before"
let width = "wide" as Int
print $width
""",
    status: 3,
  )?
  assert output.stdout == "before\n"
  assert "invalid integer `wide`" in output.stderr, output.stderr
}

# A statement that begins with a name and a word is a command, and `as` is a
# word; a conversion takes no suffix.
test test_as_keeps_command_statements_and_takes_no_suffix { |ctx|
  let command = check_errors(
    ctx,
    r"""let field = "4"
let width = try { field as Int }
print ${width ?? 0}
""",
  )?
  assert "err[check.unresolved-proc-command]" in command, command
  assert "group the conversion: `(field as TYPE)`" in command, command

  let grouped = test.expect(
    ctx,
    r"""let field = "4"
let width = try { (field as Int) }
print ${width ?? 0}
""",
    status: 0,
  )?
  assert grouped.stdout == "4\n"

  let suffix = check_errors(
    ctx,
    r"""let field = "4"
let width = field as Int.float()
print $width
""",
  )?
  assert "err[parse." in suffix, suffix
}

test test_as_formats_with_exactly_the_parentheses_it_needs { |ctx|
  let source = r"""let field = "4"
let a = (field)   as   Int
let b = -(field as Int)
let c = (field as Int) * 2
let d = (field as Int).float()
let e = (field as Int) as UInt
let f = try { (field as Int) }
print $a $b $c $d $e ${f ?? 0}
"""
  let candidate = test.temp_file(ctx, name: "as.xsh", contents: bytes.from_text(source))?
  let fixed = run.capture --text --accept=[0, 1] "xsht" lint --fix --only check.redundant-parens $candidate
  assert fixed.status.exited_with(0), fixed.stderr
  let formatted = run.capture --text "xsht" fmt $candidate
  assert formatted.status.exited_with(0), formatted.stderr
  let text = candidate.read_text()?
  assert text == r"""let field = "4"
let a = field as Int
let b = -(field as Int)
let c = field as Int * 2
let d = (field as Int).float()
let e = field as Int as UInt
let f = try { (field as Int) }
print $a $b $c $d $e ${f ?? 0}
""", text
  let output = test.expect(ctx, text, status: 0)?
  assert output.stdout == "4 -4 8 4 4 4\n"
}

# The migration lint rewrites each propagated operation the table names, and
# the result is formatted, checks, and behaves the same. A `Result` kept as a
# value stays a call.
test test_lint_rewrites_propagated_operations_as_conversions { |ctx|
  let source = r"""proc show(spec: Str, raw: Bytes, shift: Int) -> Result[Unit] {
  let fields = spec.split(":")
  let port = fields[1].parse_uint()?
  let scale = fields[2].trim().parse_float()? * 2.0
  let offset = -fields[3].parse_int()? + shift.require(UInt)?
  let name = raw.utf8()?
  let place = Path.parse_bytes(raw)?
  let fallback = fields[0].parse_int() ?? 7
  let captured = try { spec.parse_int()? }
  print $port $scale $offset $name $place $fallback ${captured is Err(_)} (fields[3].parse_int()?)
}

show("x:8080: 1.5 :4", b"etc", 2)?
"""
  let before = test.expect(ctx, source, status: 0)?
  let candidate = test.temp_file(ctx, name: "conversions.xsh", contents: bytes.from_text(source))?

  let linted = run.capture --text --accept=[0, 1] "xsht" lint --only lint.prefer-as-conversion $candidate
  let report = linted.stdout + linted.stderr
  assert count(report, "warn[lint.prefer-as-conversion]") == 8, report

  let fixed = run.capture --text "xsht" lint --fix --only lint.prefer-as-conversion $candidate
  assert fixed.status.exited_with(0), fixed.stderr
  let rewritten = candidate.read_text()?
  assert "  let port = fields[1] as UInt\n" in rewritten, rewritten
  assert "  let scale = fields[2].trim() as Float * 2.0\n" in rewritten, rewritten
  assert "  let offset = -(fields[3] as Int) + shift as UInt\n" in rewritten, rewritten
  assert "  let name = raw as Str\n" in rewritten, rewritten
  assert "  let place = raw as Path\n" in rewritten, rewritten
  assert "  let fallback = fields[0].parse_int() ?? 7\n" in rewritten, rewritten
  assert "  let captured = try { (spec as Int) }\n" in rewritten, rewritten
  assert " (fields[3] as Int)\n" in rewritten, rewritten

  let stable = run.capture --text "xsht" fmt --check $candidate
  assert stable.status.exited_with(0), stable.stderr
  let clean = run.capture --text "xsht" lint --only lint.prefer-as-conversion $candidate
  assert clean.status.exited_with(0), clean.stdout + clean.stderr
  let after = test.expect(ctx, rewritten, status: 0)?
  assert after.stdout == before.stdout
  assert after.stdout == "8080 3 -2 etc etc 7 true 4\n"
}
