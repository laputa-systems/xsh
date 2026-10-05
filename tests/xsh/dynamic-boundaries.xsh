test test_dynamic_boundary_rejects_unchecked_scalar { |ctx|
  let output = test.run_xsh(
    ctx,
    """let raw: Any = 7
let count: Int = raw
print reached
""",
  )?
  assert output.status == 2
  assert output.stdout == ""
  assert "check.dynamic-boundary" in output.stderr == true
}

test test_dynamic_boundary_rejects_unchecked_nested_container { |ctx|
  let output = test.run_xsh(
    ctx,
    """let raw: List[Any] = [7]
let values: List[Int] = raw
print reached
""",
  )?
  assert output.status == 2
  assert output.stdout == ""
  assert "check.dynamic-boundary" in output.stderr == true
}

test test_dynamic_boundary_rejects_unchecked_optional { |ctx|
  let output = test.run_xsh(
    ctx,
    """let raw: Any = null
let count: Int? = raw
print reached
""",
  )?
  assert output.status == 2
  assert output.stdout == ""
  assert "check.dynamic-boundary" in output.stderr == true
}

test test_dynamic_boundary_rejects_erased_record { |ctx|
  let output = test.run_xsh(
    ctx,
    """type Row = {name: Str}
let raw: Record = {name: "demo"}
let row: Row = raw
print reached
""",
  )?
  assert output.status == 2
  assert output.stdout == ""
  assert "check.dynamic-boundary" in output.stderr == true
}

test test_dynamic_boundary_rejects_exact_empty_record { |ctx|
  let output = test.run_xsh(
    ctx,
    """type Row = {name: Str}
let row: Row = {}
print reached
""",
  )?
  assert output.status == 2
  assert output.stdout == ""
  assert "check.type-mismatch" in output.stderr == true
}

test test_dynamic_boundary_rejects_host_data_without_validation { |ctx|
  let output = test.run_xsh(
    ctx,
    """type Row = {name: Str}
let row: Row = json.decode("{\\"name\\":\\"demo\\"}")?
print reached
""",
  )?
  assert output.status == 2
  assert output.stdout == ""
  assert "check.dynamic-boundary" in output.stderr == true
}

test test_dynamic_boundary_rejects_unknown_known_record_field { |ctx|
  let output = test.run_xsh(
    ctx,
    """let row = {name: "demo"}
let missing = row.version
print reached
""",
  )?
  assert output.status == 2
  assert output.stdout == ""
  assert "check.unknown-field" in output.stderr == true
}

test test_dynamic_boundary_keeps_explicit_validation { |ctx|
  let output = test.run_xsh(
    ctx,
    """type Row = {name: Str}
let raw = json.decode("{\\"name\\":\\"demo\\"}")?
let row = raw.require(Row)?
print \${row.name}
""",
  )?
  assert output.status == 0
  assert output.stdout == """demo
"""
}

test test_dynamic_boundary_keeps_concrete_erasure_and_serialization { |ctx|
  let output = test.run_xsh(
    ctx,
    """let raw: Any = {name: "demo"}
print \${json.encode(raw)?}
""",
  )?
  assert output.status == 0
  assert output.stdout == """{"name":"demo"}
"""
}

test test_dynamic_boundary_keeps_known_record_width { |ctx|
  let output = test.run_xsh(
    ctx,
    """type Row = {name: Str}
let full = {name: "demo", version: 1}
let row: Row = full
print \${row.name}
""",
  )?
  assert output.status == 0
  assert output.stdout == """demo
"""
}

test test_dynamic_boundary_rejects_nested_dynamic_record_field { |ctx|
  let output = test.run_xsh(
    ctx,
    """type Row = {count: Int}
let count: Any = 7
let raw = {count}
let row: Row = raw
print reached
""",
  )?
  assert output.status == 2
  assert output.stdout == ""
  assert "check.dynamic-boundary" in output.stderr == true
}

test test_dynamic_boundary_rejects_erased_callable_result { |ctx|
  let output = test.run_xsh(
    ctx,
    """pure answer() -> Int { 7 }
let callback: Pure = answer
let count: Int = callback.call()
print reached
""",
  )?
  assert output.status == 2
  assert output.stdout == ""
  assert "check." in output.stderr == true
}

test test_dynamic_boundary_keeps_container_contract_invariant { |ctx|
  let output = test.run_xsh(
    ctx,
    """let values = [7]
let erased: List[Any] = values
print reached
""",
  )?
  assert output.status == 2
  assert output.stdout == ""
  assert "check." in output.stderr == true
}

test test_dynamic_boundary_keeps_type_pattern_proof { |ctx|
  let output = test.run_xsh(
    ctx,
    """let raw: Any = 7
match raw {
  count is Int => print \${count + 1}
  _ => print wrong
}
""",
  )?
  assert output.status == 0
  assert output.stdout == """8
"""
}

test test_dynamic_boundary_keeps_null_optional_and_dynamic_equality { |ctx|
  let output = test.run_xsh(
    ctx,
    """let count: Int? = null
let raw: Any = 7
print \${count == null}
print \${raw == 7}
""",
  )?
  assert output.status == 0
  assert output.stdout == """true
true
"""
}

test test_dynamic_boundary_keeps_explicit_erased_record_validation { |ctx|
  let output = test.run_xsh(
    ctx,
    """type Row = {name: Str}
let raw: Record = {name: "demo"}
let row = raw.require(Row)?
print \${row.name}
""",
  )?
  assert output.status == 0
  assert output.stdout == """demo
"""
}

test test_dynamic_boundary_agrees_across_runner_preparation { |ctx|
  let source = p"tests/fixtures/sema/invalid/unchecked-json-boundary.xsh".read_text()?
  let in_process = test.run_script(ctx, source)?
  let product = test.run_xsh(ctx, source)?
  let traced = test.run_xsht_trace(ctx, source)?
  assert in_process.status == 2
  assert product.status == 2
  assert traced.status == 2
  assert in_process.stdout == ""
  assert product.stdout == ""
  assert traced.stdout == ""
  assert "check.dynamic-boundary" in in_process.stderr == true
  assert "check.dynamic-boundary" in product.stderr == true
  assert "check.dynamic-boundary" in traced.stderr == true
}

test test_dynamic_boundary_rejects_unchecked_dynamic_module { |ctx|
  let output = test.run_xsh(
    ctx,
    """type Plugin = module { export let name: Str }
let plugin: Plugin = module.load(p"missing.xsh")?
print reached
""",
  )?
  assert output.status == 2
  assert output.stdout == ""
  assert "check.dynamic-boundary" in output.stderr == true
}

test test_dynamic_boundary_keeps_explicit_dynamic_get { |ctx|
  let output = test.run_xsh(
    ctx,
    """let raw: Record = {count: 7}
let count = raw.get("count")?.require(Int)?
print \${count}
""",
  )?
  assert output.status == 0
  assert output.stdout == """7
"""
}

test test_dynamic_boundary_keeps_symmetric_equality { |ctx|
  let output = test.run_xsh(
    ctx,
    """let raw: Any = 7
print \${7 == raw}
print \${raw == 7}
""",
  )?
  assert output.status == 0
  assert output.stdout == """true
true
"""
}

test test_dynamic_boundary_does_not_certify_dynamic_arithmetic { |ctx|
  let output = test.run_xsh(
    ctx,
    """let raw: Any = 7
let computed: Int = raw + 1
print reached
""",
  )?
  assert output.status == 2
  assert output.stdout == ""
  assert "check.dynamic-boundary" in output.stderr == true
}

test test_dynamic_boundary_rejects_unchecked_mutable_rebinding { |ctx|
  let output = test.run_xsh(
    ctx,
    """var count: Int = 1
let raw: Any = 7
count = raw
print reached
""",
  )?
  assert output.status == 2
  assert output.stdout == ""
  assert "check.dynamic-boundary" in output.stderr == true
}

test test_dynamic_boundary_rejects_unchecked_parameter { |ctx|
  let output = test.run_xsh(
    ctx,
    """pure consume(count: Int) -> Int { count }
let raw: Any = 7
let count = consume(raw)
print reached
""",
  )?
  assert output.status == 2
  assert output.stdout == ""
  assert "check.dynamic-boundary" in output.stderr == true
}

test test_dynamic_boundary_rejects_unchecked_return { |ctx|
  let output = test.run_xsh(
    ctx,
    """pure answer() -> Int { let raw: Any = 7; raw }
print reached
""",
  )?
  assert output.status == 2
  assert output.stdout == ""
  assert "check.dynamic-boundary" in output.stderr == true
}

test test_dynamic_boundary_rejects_unchecked_map_values { |ctx|
  let output = test.run_xsh(
    ctx,
    """let raw: Map[Any] = {count: 7}
let counts: Map[Int] = raw
print reached
""",
  )?
  assert output.status == 2
  assert output.stdout == ""
  assert "check.dynamic-boundary" in output.stderr == true
}

test test_dynamic_boundary_keeps_dynamic_membership_comparison { |ctx|
  let output = test.run_xsh(
    ctx,
    """let raw: Any = 7
print \${raw in [7]}
""",
  )?
  assert output.status == 0
  assert output.stdout == """true
"""
}

test test_dynamic_boundary_keeps_explicit_dynamic_index_validation { |ctx|
  let output = test.run_xsh(
    ctx,
    """let raw: Record = {count: 7}
let count = raw["count"].require(Int)?
print \${count}
""",
  )?
  assert output.status == 0
  assert output.stdout == """7
"""
}

test test_dynamic_boundary_keeps_stream_numeric_domain_invariant { |ctx|
  let output = test.run_xsh(
    ctx,
    """stream numbers() [] -> Stream[Int] { yield -1 }
let narrowed: Stream[UInt] = numbers()
print reached
""",
  )?
  assert output.status == 2
  assert output.stdout == ""
  assert "check.type-mismatch" in output.stderr == true
}

test test_dynamic_boundary_rejects_unchecked_json_adapter_rows { |ctx|
  let output = test.run_xsh(
    ctx,
    """type Row = {name: Str}
let rows = "{\\"name\\":\\"demo\\"}\\n" |> json.lines()
let row: Row = rows[0]
print reached
""",
  )?
  assert output.status == 2
  assert output.stdout == ""
  assert "check.dynamic-boundary" in output.stderr == true
}

test test_dynamic_boundary_keeps_contextual_collection_construction { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""
let rows: List[Record] = [{count: n} for n in range(2)]
let values: Map[Any] = {row.name: row.count for row in [{name: "first", count: 0}, {name: "second", count: 1}]}
print ${rows.len()}
print ${values.len()}
""",
  )?
  assert output.status == 0
  assert output.stdout == """2
2
"""
}

test test_dynamic_boundary_keeps_concrete_builtin_results_in_wider_destinations { |ctx|
  let output = test.run_xsh(
    ctx,
    """let label: Str? = "demo".trim()
let dynamic: Any = "demo".trim()
print \${label ?? "missing"}
""",
  )?
  assert output.status == 0
  assert output.stdout == """demo
"""
}

test test_dynamic_boundary_keeps_concrete_result_in_dynamic_inspection {
  let result: Result[Any] = "7".parse_int()
  assert result is Ok(_)
}

test test_dynamic_boundary_keeps_nullable_equality_in_both_operand_orders { |ctx|
  let output = test.run_xsh(
    ctx,
    """let nullable: Int? = 7
let concrete = 7
print \${concrete == nullable}
print \${nullable == concrete}
print \${concrete != nullable}
""",
  )?
  assert output.status == 0
  assert output.stdout == """true
true
false
"""
}

test test_dynamic_boundary_rejects_unchecked_compound_rebinding { |ctx|
  let output = test.run_xsh(
    ctx,
    """var values: List[Int] = [1]
let raw: Any = [2]
values += raw
print reached
""",
  )?
  assert output.status == 2
  assert output.stdout == ""
  assert "check.dynamic-boundary" in output.stderr == true
}

test test_dynamic_boundary_rejects_known_non_json_value { |ctx|
  let output = test.run_xsh(
    ctx,
    """let path = p"demo"
let encoded = json.encode(path)?
print reached
""",
  )?
  assert output.status == 2
  assert output.stdout == ""
  assert "check.json-compatible" in output.stderr == true
}

test test_dynamic_boundary_rejects_unchecked_result_return_payloads { |ctx|
  for body in ["values.get(0)", "return values.get(0)"] {
    let source = f"""
      pure selected(values: List[Any]) -> Result[Int] {{ {body} }}
      let input: List[Any] = [7]
      let count = selected(input)?
      print reached

      """
    let output = test.run_xsh(ctx, source)?
    assert output.status == 2
    assert output.stdout == ""
    assert "check.dynamic-boundary" in output.stderr == true
  }
}

test test_dynamic_boundary_rejects_erased_requirement_target { |ctx|
  let output = test.run_xsh(
    ctx,
    """let raw: Any = {}
let erased: Record = raw.require()?
print reached
""",
  )?
  assert output.status == 2
  assert output.stdout == ""
  assert "check.require-target" in output.stderr == true
}

test test_dynamic_boundary_keeps_nominal_error_in_contextual_success { |ctx|
  let output = test.run_xsh(
    ctx,
    """error Failure = Bad(message: Str)
pure selected() -> Result[Int, Failure] { Ok(4) }
print \${selected()?}
""",
  )?
  assert output.status == 0
  assert output.stdout == """4
"""
}

test test_dynamic_boundary_serializes_concrete_record_lists_without_erasing_them { |ctx|
  let output = test.run_xsh(
    ctx,
    """type Row = {name: Str}
let root = fs.tempdir()?
defer root.close()?
let row_path = fp"{root.host_path()?}/rows.jsonl"
let rows: List[Row] = [{name: "demo"}]
json.write_lines(row_path, rows)?
print row_path.read_text()?
""",
  )?
  assert output.status == 0
  assert output.stdout == """{"name":"demo"}

"""
}

# Every operation that interprets an `Any` (a typed slot, an operator, a
# condition, a display, or a command word) needs a validated type first.
test test_dynamic_boundary_rejects_each_interpreting_use { |ctx|
  let prelude = r"""let raw: Any = {a: 1, ok: true, xs: [1, 2], name: "n"}
pure takes(count: Int) -> Int { count }
"""
  for snippet in [
    "let count: Int = raw.a",
    "let count: Int = raw[\"a\"]",
    "let count: Int = raw.xs.len()",
    "let count = takes(raw.a)",
    "let next = raw.a + 1",
    "let label = \"n=\" + raw.name",
    "let negated = -raw.a",
    "let small = raw.a < 2",
    "let off = !raw.ok",
    "let both = raw.ok and true",
    "if raw.ok { print yes }",
    "while raw.ok { break }",
    "assert raw.ok",
    "let kept = [x for x in [1] if raw.ok]",
    r"""let text = f"{raw.a}" """,
    "print $raw",
    r"""run echo ${raw.a}""",
    "let found = 1 in raw.xs",
    "let found = raw.name in \"name\"",
    "let picked = [1, 2][raw.a]",
    "let trimmed = raw.name.trim(chars: \" \")",
    "let sorted = [raw] |> sort-by .a",
    "let counted = [raw] |> count { |row| row.name }",
    "let largest = raw.xs |> max",
  ] {
    let output = test.run_script(ctx, prelude + snippet + "\n")?
    assert output.status == 2, snippet
    assert output.stdout == "", snippet
    assert "check.dynamic-boundary" in output.stderr, snippet + ": " + output.stderr
  }
}

# Navigation, `Any` destinations, equality, list membership, and validation
# need no `.require`: each is defined for every value or yields `Any`.
test test_dynamic_boundary_keeps_navigation_equality_and_validation { |ctx|
  let output = test.expect(
    ctx,
    r"""type Row = {a: Int, xs: List[Int]}
let raw: Any = {a: 1, ok: true, xs: [1, 2], name: "n", nested: {b: 2}}
pure keep(value: Any) -> Any { value }
let field = raw.nested.b
let item = raw.xs[0]
let part = raw.xs[0..1]
let size = raw.xs.len()
let seen = [x for x in raw.xs]
let same = raw.a == 1
let listed = raw.a in [1, 2]
let kept = keep(raw.nested)
let groups = [raw, raw] |> group-by .name |> count()
print ${json.encode([field, item, part, size, seen, same, listed, kept, groups])?}
let count = raw.a.require(Int)? + 1
print $count
match raw.name {
  name is Str => print $name
  _ => print other
}
let row = raw.require(Row)?
let back: Any = row
print ${back == raw}
print ${row.xs.len()}
""",
    status: 0,
  )?
  assert output.stdout == """[2,1,[1],2,[1,2],true,true,{"b":2},1]
2
n
true
2
"""
}

# The fix writes the target only where a bare `.require()` cannot infer it,
# as `lint.inferred-require-target` asks.
test test_dynamic_boundary_fix_inserts_require_for_the_contextual_type { |ctx|
  let source = r"""let raw: Any = {a: 1, ok: true, name: "n", xs: [3]}
pure takes(name: Str) -> Str { name }
let count: Int = raw.a
let next = raw.a + 1
if raw.ok { print $next }
print ${takes(raw.name)}
let first = [raw.xs][0]
let items: List[Int] = raw.xs
print ${items.len() + count}
"""
  let candidate = test.temp_file(ctx, name: "dynamic-boundary-fix.xsh", contents: bytes.from_text(source))?
  let fixed = run.capture --text "xsht" lint --only check.dynamic-boundary --fix $candidate ?
  assert fixed.status.exited_with(0), fixed.stderr
  assert candidate.read_text()? == r"""let raw: Any = {a: 1, ok: true, name: "n", xs: [3]}
pure takes(name: Str) -> Str { name }
let count: Int = raw.a.require()?
let next = raw.a.require(Int)? + 1
if raw.ok.require(Bool)? { print $next }
print ${takes(raw.name.require()?)}
let first = [raw.xs][0]
let items: List[Int] = raw.xs.require()?
print ${items.len() + count}
"""
  let executed = test.expect(ctx, candidate.read_text()?, status: 0)?
  assert executed.stdout == """2
n
2
"""
}

# Without one target type, or where `?` cannot propagate, the fix would
# guess or add a new error, so the source is left for a hand edit.
test test_dynamic_boundary_fix_leaves_untargeted_uses_unchanged { |ctx|
  for source in [
    r"""let raw: Any = {a: 1}
let text = f"{raw.a}"
""",
    r"""let raw: Any = {a: 1, b: 2}
let sum = raw.a + raw.b
""",
    r"""pure count(raw: Any) -> Int {
  let value: Int = raw.a
  value
}
""",
  ] {
    let candidate = test.temp_file(ctx, name: "dynamic-boundary-no-fix.xsh", contents: bytes.from_text(source))?
    let fixed = run.capture --text "xsht" lint --only check.dynamic-boundary --fix $candidate ?
    assert ! fixed.status.exited_with(0), source
    assert "check.dynamic-boundary" in fixed.stderr, fixed.stderr
    assert candidate.read_text()? == source
  }
}
