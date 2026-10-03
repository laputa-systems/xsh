test test_builtin_templates_keep_positional_defaults_errors_and_value_semantics { |ctx|
  let output = test.run_script(ctx, r"""
proc main() [error] {
  let words = ["one", "two"]
  print words.join()
  print words.join(":")
  let values = [1, 2]
  print (values.get(9) ?? 7)
  print ${values.get(9) is Err(_)}
  print ${2 in values}
  let original: Map[Int] = {}
  let updated = original.set("one", 1).set("two", 2)
  print original.len()
  print (updated.get("absent") ?? 9)
  print updated.remove("one").keys().join()
  let empty: Map[List[Str]] = {}
  let groups = empty.push("group", "one").push("group", "two")
  print groups.get("group")?.join(":")
  print empty.len()
}
""")?
  assert output.success, output.stderr
  assert output.stdout == "onetwo\none:two\n7\ntrue\ntrue\n0\n9\ntwo\none:two\n0\n"
}

test test_builtin_templates_preserve_nested_values_across_call_spellings { |ctx|
  let output = test.run_script(ctx, r"""
error NestedError = Missing(code: Int)
pure wrap(value: Int) -> Result[List[Int], NestedError] { return [value] }
proc main() [error] {
  let values: List[Result[List[Int], NestedError]] = [wrap(7)]
  let direct: Result[List[Int], NestedError] = values.get(0)?
  let named: Result[List[Int], NestedError] = values.get(index: 0)?
  let options = {index: 0}
  let spread: Result[List[Int], NestedError] = values.get(...options)?
  print direct?[0]
  print named?[0]
  print spread?[0]
  let repeated = values.push(item: wrap(8)).extend(other: values)
  print repeated.len()
  let table: Map[List[Int]] = {one: [1]}
  let updated: Map[List[Int]] = table.set(value: [2], key: "two")
  let fallback: List[Int] = updated.get(key: "absent") ?? [9]
  print fallback[0]
  print updated.values().len()
  print updated.keys().join(separator: ",")
}
""")?
  assert output.success, output.stderr
  assert output.stdout == "7\n7\n7\n3\n9\n2\none,two\n"
}

test test_builtin_templates_evaluate_named_arguments_in_source_order { |ctx|
  let output = test.run_script(ctx, r"""
proc key() [] -> Str { print "key"; return "two" }
proc value() [] -> Int { print "value"; return 2 }
proc receiver() [] -> Map[Int] { print "receiver"; return {one: 1} }
proc main() [] {
  let table: Map[Int] = {one: 1}
  let updated = receiver().set(value: value(), key: key())
  print (updated.get("two") ?? 0)
  print ${"two" in table}
  let absent: Map[Int]? = null
  let untouched = absent?.set(value: value(), key: key())
  print ${untouched == null}
}
""")?
  assert output.success, output.stderr
  assert output.stdout == "receiver\nvalue\nkey\n2\nfalse\ntrue\n"
}

test test_builtin_templates_reject_incompatible_concrete_operands { |ctx|
  for source in [
    r"""let values: List[Int] = [1]; let _ = values.push("bad")""",
    r"""let values: List[Int] = [1]; let _ = (values.get(9) ?? "bad")""",
    r"""let values: List[Int] = [1]; let _ = values.extend(["bad"])""",
    r"""let values: List[Int] = [1]; let _ = values.join()""",
    r"""let values: List[Str] = ["one"]; let _ = values.push(Path("two"))""",
    r"""let values: Map[Str] = {one: "first"}; let _ = values.set("two", Path("second"))""",
    r"""let values: Map[Int] = {one: 1}; let _ = values.set("two", "bad")""",
    r"""let values: Map[Int] = {one: 1}; let _ = (values.get("absent") ?? "bad")""",
    r"""let values: Map[Int] = {one: 1}; let _ = values.push("one", 2)""",
  ] {
    let output = test.run_script(ctx, source)?
    assert !output.success, source
    assert "check.type-mismatch" in output.stderr, output.stderr
  }
}

test test_builtin_templates_instantiate_empty_maps_from_independent_contexts { |ctx|
  let output = test.run_script(ctx, r"""
proc discard() [] -> Unit { let _ = map.empty(); map.empty() }
proc main() [error] {
  discard()
  let names: Map[Int, Str] = map.empty()
  let values = map.empty().set(key: 1, value: [7])
  let labels = map.empty().set(value: "two", key: "second")
  let dynamic: Map[Str, Any] = map.empty()
  print names.len()
  print values.get(key: 1)?[0]
  print labels.get("second")?
  print dynamic.len()
  let _ = map.empty()
}
""")?
  assert output.success, output.stderr
  assert output.stdout == "0\n7\ntwo\n0\n"
}

test test_builtin_templates_keep_result_return_contracts_under_success_contexts { |ctx|
  let output = test.run_script(ctx, r"""
pure parsed_tail(value: Str) -> Result[Int] { value.parse_int() }
pure parsed_return(value: Str) -> Result[Int] { return value.parse_int() }
pure encoded_tail(value: Int) -> Result[Str] { json.encode(value) }
pure encoded_return(value: Int) -> Result[Str] { return json.encode(value) }
proc main() [error] {
  print parsed_tail("7")?
  print parsed_return("8")?
  print encoded_tail(9)?
  print encoded_return(10)?
}
""")?
  assert output.success, output.stderr
  assert output.stdout == "7\n8\n9\n10\n"
  let invalid = test.run_script(ctx, r"""let text: Str = json.encode(1)""")?
  assert !invalid.success, invalid.stderr
  assert "check.type-mismatch" in invalid.stderr, invalid.stderr
}

test test_builtin_templates_match_materialized_line_and_collection_values { |ctx|
  let output = test.run_script(ctx, r"""
pure text_lines(value: Str) -> Int {
  let lines: List[Str] = value.lines()
  let retained: List[Str] = lines.collect()
  retained.len()
}
pure byte_lines(value: Bytes) -> Int {
  let lines: List[Bytes] = value.lines()
  lines.collect().len()
}
pure stream_values() -> Int {
  let values: List[Int] = range(3).collect()
  values.len()
}
proc main() [] {
  print text_lines("one\ntwo\n")
  print byte_lines(b"one\n")
  print stream_values()
  print "one\n".lines().len()
}
""")?
  assert output.success, output.stderr
  assert output.stdout == "2\n1\n3\n1\n"
}

test test_builtin_templates_do_not_certify_dynamic_receiver_domains { |ctx|
  for source in [
    r"""let values: List[Any] = [1]; let selected: Result[Int] = values.get(0)""",
    r"""let values: Map[Str, Any] = {one: 1}; let selected: Result[Int] = values.get("one")""",
  ] {
    let candidate = test.temp_file(ctx, name: "dynamic-receiver.xsh", contents: bytes.from_text(source))?
    let output = run.capture --text "xsht" check $candidate ?
    assert !output.status.exited_with(0), source
    assert "check.dynamic-boundary" in output.stderr, output.stderr
  }
}

test test_builtin_templates_carry_typed_map_key_and_value_parameters { |ctx|
  let declaration = "let table: Map[Int, Str] = {[1]: \"one\"}\n"
  let accepted = test.run_script(ctx, declaration + r"""let keys: List[Int] = table.keys()
let values: List[Str] = table.values()
let found: Result[Str] = table.get(1)
let updated: Map[Int, Str] = table.set(2, "two")
let removed: Map[Int, Str] = updated.remove(1)
print ${keys[0]} ${values[0]} ${found?} ${updated.len()} ${removed.keys()[0]}
""")?
  assert accepted.success, accepted.stderr
  assert accepted.stdout == "1 one one 2 2\n"
  for source in [
    "let wrong: List[Str] = table.keys()\n",
    "let wrong: List[Int] = table.values()\n",
    "for {key, value} in table { let wrong: Str = key }\n",
    "let _ = table.remove(\"one\")\n",
    "let _ = \"one\" in table\n",
  ] {
    let rejected = test.run_script(ctx, declaration + source)?
    assert !rejected.success, source
    assert "check.type-mismatch" in rejected.stderr, rejected.stderr
  }
}
