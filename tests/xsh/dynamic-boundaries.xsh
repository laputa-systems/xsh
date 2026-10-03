test test_dynamic_boundary_rejects_unchecked_scalar { |ctx|
  let output = test.run_xsh(ctx, "let raw: Any = 7\nlet count: Int = raw\nprint reached\n")?
  output.status == 2
  output.stdout == ""
  ("check.dynamic-boundary" in output.stderr) == true
}

test test_dynamic_boundary_rejects_unchecked_nested_container { |ctx|
  let output = test.run_xsh(ctx, "let raw: List[Any] = [7]\nlet values: List[Int] = raw\nprint reached\n")?
  output.status == 2
  output.stdout == ""
  ("check.dynamic-boundary" in output.stderr) == true
}

test test_dynamic_boundary_rejects_unchecked_optional { |ctx|
  let output = test.run_xsh(ctx, "let raw: Any = null\nlet count: Int? = raw\nprint reached\n")?
  output.status == 2
  output.stdout == ""
  ("check.dynamic-boundary" in output.stderr) == true
}

test test_dynamic_boundary_rejects_erased_record { |ctx|
  let output = test.run_xsh(ctx, "type Row = {name: Str}\nlet raw: Record = {name: \"demo\"}\nlet row: Row = raw\nprint reached\n")?
  output.status == 2
  output.stdout == ""
  ("check.dynamic-boundary" in output.stderr) == true
}

test test_dynamic_boundary_rejects_exact_empty_record { |ctx|
  let output = test.run_xsh(ctx, "type Row = {name: Str}\nlet row: Row = {}\nprint reached\n")?
  output.status == 2
  output.stdout == ""
  ("check.type-mismatch" in output.stderr) == true
}

test test_dynamic_boundary_rejects_host_data_without_validation { |ctx|
  let output = test.run_xsh(ctx, "type Row = {name: Str}\nlet row: Row = json.decode(\"{\\\"name\\\":\\\"demo\\\"}\")?\nprint reached\n")?
  output.status == 2
  output.stdout == ""
  ("check.dynamic-boundary" in output.stderr) == true
}

test test_dynamic_boundary_rejects_unknown_known_record_field { |ctx|
  let output = test.run_xsh(ctx, "let row = {name: \"demo\"}\nlet missing = row.version\nprint reached\n")?
  output.status == 2
  output.stdout == ""
  ("check.unknown-field" in output.stderr) == true
}

test test_dynamic_boundary_keeps_explicit_validation { |ctx|
  let output = test.run_xsh(ctx, "type Row = {name: Str}\nlet raw = json.decode(\"{\\\"name\\\":\\\"demo\\\"}\")?\nlet row = raw.require(Row)?\nprint \${row.name}\n")?
  output.status == 0
  output.stdout == "demo\n"
}

test test_dynamic_boundary_keeps_concrete_erasure_and_serialization { |ctx|
  let output = test.run_xsh(ctx, "let raw: Any = {name: \"demo\"}\nprint \${json.encode(raw)?}\n")?
  output.status == 0
  output.stdout == "{\"name\":\"demo\"}\n"
}

test test_dynamic_boundary_keeps_known_record_width { |ctx|
  let output = test.run_xsh(ctx, "type Row = {name: Str}\nlet full = {name: \"demo\", version: 1}\nlet row: Row = full\nprint \${row.name}\n")?
  output.status == 0
  output.stdout == "demo\n"
}

test test_dynamic_boundary_rejects_nested_dynamic_record_field { |ctx|
  let output = test.run_xsh(ctx, "type Row = {count: Int}\nlet count: Any = 7\nlet raw = {count}\nlet row: Row = raw\nprint reached\n")?
  output.status == 2
  output.stdout == ""
  ("check.dynamic-boundary" in output.stderr) == true
}

test test_dynamic_boundary_rejects_erased_callable_result { |ctx|
  let output = test.run_xsh(ctx, "pure answer() -> Int { 7 }\nlet callback: Pure = answer\nlet count: Int = callback.call()\nprint reached\n")?
  output.status == 2
  output.stdout == ""
  ("check." in output.stderr) == true
}

test test_dynamic_boundary_keeps_container_contract_invariant { |ctx|
  let output = test.run_xsh(ctx, "let values = [7]\nlet erased: List[Any] = values\nprint reached\n")?
  output.status == 2
  output.stdout == ""
  ("check." in output.stderr) == true
}

test test_dynamic_boundary_keeps_type_pattern_proof { |ctx|
  let output = test.run_xsh(ctx, "let raw: Any = 7\nmatch raw {\n  count is Int => print \${count + 1}\n  _ => print wrong\n}\n")?
  output.status == 0
  output.stdout == "8\n"
}

test test_dynamic_boundary_keeps_null_optional_and_dynamic_equality { |ctx|
  let output = test.run_xsh(ctx, "let count: Int? = null\nlet raw: Any = 7\nprint \${count == null}\nprint \${raw == 7}\n")?
  output.status == 0
  output.stdout == "true\ntrue\n"
}

test test_dynamic_boundary_keeps_explicit_erased_record_validation { |ctx|
  let output = test.run_xsh(ctx, "type Row = {name: Str}\nlet raw: Record = {name: \"demo\"}\nlet row = raw.require(Row)?\nprint \${row.name}\n")?
  output.status == 0
  output.stdout == "demo\n"
}

test test_dynamic_boundary_agrees_across_runner_preparation { |ctx|
  let source = p"tests/fixtures/sema/invalid/unchecked-json-boundary.xsh".read_text()?
  let in_process = test.run_script(ctx, source)?
  let product = test.run_xsh(ctx, source)?
  let traced = test.run_xsht_trace(ctx, source)?
  in_process.status == 2
  product.status == 2
  traced.status == 2
  in_process.stdout == ""
  product.stdout == ""
  traced.stdout == ""
  ("check.dynamic-boundary" in in_process.stderr) == true
  ("check.dynamic-boundary" in product.stderr) == true
  ("check.dynamic-boundary" in traced.stderr) == true
}

test test_dynamic_boundary_rejects_unchecked_dynamic_module { |ctx|
  let output = test.run_xsh(ctx, "type Plugin = module { export let name: Str }\nlet plugin: Plugin = module.load(p\"missing.xsh\")?\nprint reached\n")?
  output.status == 2
  output.stdout == ""
  ("check.dynamic-boundary" in output.stderr) == true
}

test test_dynamic_boundary_keeps_explicit_dynamic_get { |ctx|
  let output = test.run_xsh(ctx, "let raw: Record = {count: 7}\nlet count = raw.get(\"count\")?.require(Int)?\nprint \${count}\n")?
  output.status == 0
  output.stdout == "7\n"
}

test test_dynamic_boundary_keeps_dynamic_arithmetic_and_symmetric_equality { |ctx|
  let output = test.run_xsh(ctx, "let raw: Any = 7\nlet computed = raw + 1\nprint \${json.encode(computed)?}\nprint \${7 == raw}\nprint \${raw == 7}\n")?
  output.status == 0
  output.stdout == "8\ntrue\ntrue\n"
}

test test_dynamic_boundary_does_not_certify_dynamic_arithmetic { |ctx|
  let output = test.run_xsh(ctx, "let raw: Any = 7\nlet computed: Int = raw + 1\nprint reached\n")?
  output.status == 2
  output.stdout == ""
  ("check.dynamic-boundary" in output.stderr) == true
}

test test_dynamic_boundary_rejects_unchecked_mutable_rebinding { |ctx|
  let output = test.run_xsh(ctx, "var count: Int = 1\nlet raw: Any = 7\ncount = raw\nprint reached\n")?
  output.status == 2
  output.stdout == ""
  ("check.dynamic-boundary" in output.stderr) == true
}

test test_dynamic_boundary_rejects_unchecked_parameter { |ctx|
  let output = test.run_xsh(ctx, "pure consume(count: Int) -> Int { count }\nlet raw: Any = 7\nlet count = consume(raw)\nprint reached\n")?
  output.status == 2
  output.stdout == ""
  ("check.dynamic-boundary" in output.stderr) == true
}

test test_dynamic_boundary_rejects_unchecked_return { |ctx|
  let output = test.run_xsh(ctx, "pure answer() -> Int { let raw: Any = 7; raw }\nprint reached\n")?
  output.status == 2
  output.stdout == ""
  ("check.dynamic-boundary" in output.stderr) == true
}

test test_dynamic_boundary_rejects_unchecked_map_values { |ctx|
  let output = test.run_xsh(ctx, "let raw: Map[Any] = {count: 7}\nlet counts: Map[Int] = raw\nprint reached\n")?
  output.status == 2
  output.stdout == ""
  ("check.dynamic-boundary" in output.stderr) == true
}

test test_dynamic_boundary_keeps_dynamic_membership_comparison { |ctx|
  let output = test.run_xsh(ctx, "let raw: Any = 7\nprint \${raw in [7]}\n")?
  output.status == 0
  output.stdout == "true\n"
}

test test_dynamic_boundary_keeps_explicit_dynamic_index_validation { |ctx|
  let output = test.run_xsh(ctx, "let raw: Record = {count: 7}\nlet count = raw[\"count\"].require(Int)?\nprint \${count}\n")?
  output.status == 0
  output.stdout == "7\n"
}

test test_dynamic_boundary_keeps_stream_numeric_domain_invariant { |ctx|
  let output = test.run_xsh(ctx, "stream numbers() [] -> Stream[Int] { yield -1 }\nlet narrowed: Stream[UInt] = numbers()\nprint reached\n")?
  output.status == 2
  output.stdout == ""
  ("check.type-mismatch" in output.stderr) == true
}

test test_dynamic_boundary_rejects_unchecked_json_adapter_rows { |ctx|
  let output = test.run_xsh(ctx, "type Row = {name: Str}\nlet rows = \"{\\\"name\\\":\\\"demo\\\"}\\n\" |> json.lines()\nlet row: Row = rows[0]\nprint reached\n")?
  output.status == 2
  output.stdout == ""
  ("check.dynamic-boundary" in output.stderr) == true
}

test test_dynamic_boundary_keeps_contextual_collection_construction { |ctx|
  let output = test.run_xsh(ctx, r"""
let rows: List[Record] = [{count: n} for n in range(2)]
let values: Map[Any] = {row.name: row.count for row in [{name: "first", count: 0}, {name: "second", count: 1}]}
print ${rows.len()}
print ${values.len()}
""")?
  output.status == 0
  output.stdout == "2\n2\n"
}

test test_dynamic_boundary_keeps_concrete_builtin_results_in_wider_destinations { |ctx|
  let output = test.run_xsh(ctx, "let label: Str? = \"demo\".trim()\nlet dynamic: Any = \"demo\".trim()\nprint \${label ?? \"missing\"}\n")?
  output.status == 0
  output.stdout == "demo\n"
}

test test_dynamic_boundary_keeps_concrete_result_in_dynamic_inspection {
  let result: Result[Any] = "7".parse_int()
  result is Ok(_)
}

test test_dynamic_boundary_keeps_nullable_equality_in_both_operand_orders { |ctx|
  let output = test.run_xsh(ctx, "let nullable: Int? = 7\nlet concrete = 7\nprint \${concrete == nullable}\nprint \${nullable == concrete}\nprint \${concrete != nullable}\n")?
  output.status == 0
  output.stdout == "true\ntrue\nfalse\n"
}

test test_dynamic_boundary_rejects_unchecked_compound_rebinding { |ctx|
  let output = test.run_xsh(ctx, "var values: List[Int] = [1]\nlet raw: Any = [2]\nvalues += raw\nprint reached\n")?
  output.status == 2
  output.stdout == ""
  ("check.dynamic-boundary" in output.stderr) == true
}

test test_dynamic_boundary_rejects_known_non_json_value { |ctx|
  let output = test.run_xsh(ctx, "let path = p\"demo\"\nlet encoded = json.encode(path)?\nprint reached\n")?
  output.status == 2
  output.stdout == ""
  ("check.json-compatible" in output.stderr) == true
}

test test_dynamic_boundary_rejects_unchecked_result_return_payloads { |ctx|
  for body in ["values.get(0)", "return values.get(0)"] {
    let source = f"""
      pure selected(values: List[Any]) -> Result[Int] { ${body} }
      let input: List[Any] = [7]
      let count = selected(input)?
      print reached

      """
    let output = test.run_xsh(ctx, source)?
    output.status == 2
    output.stdout == ""
    ("check.dynamic-boundary" in output.stderr) == true
  }
}

test test_dynamic_boundary_rejects_erased_requirement_target { |ctx|
  let output = test.run_xsh(ctx, "let raw: Any = {}\nlet erased: Record = raw.require()?\nprint reached\n")?
  output.status == 2
  output.stdout == ""
  ("check.require-target" in output.stderr) == true
}

test test_dynamic_boundary_keeps_nominal_error_in_contextual_success { |ctx|
  let output = test.run_xsh(ctx, "error Failure = Bad(message: Str)\npure selected() -> Result[Int, Failure] { Ok(4) }\nprint \${selected()?}\n")?
  output.status == 0
  output.stdout == "4\n"
}

test test_dynamic_boundary_serializes_concrete_record_lists_without_erasing_them { |ctx|
  let output = test.run_xsh(ctx, "type Row = {name: Str}\nlet root = fs.tempdir()?\ndefer root.close()?\nlet row_path = fp\"\${root.host_path()?}/rows.jsonl\"\nlet rows: List[Row] = [{name: \"demo\"}]\njson.write_lines(row_path, rows)?\nprint row_path.read_text()?\n")?
  output.status == 0
  output.stdout == "{\"name\":\"demo\"}\n\n"
}
