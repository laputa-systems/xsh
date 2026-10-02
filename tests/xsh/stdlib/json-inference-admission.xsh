test test_json_inference_admission_mixed_declared_list [error] {
  json.encode_lines([1, "two", null, true])? == "1\n\"two\"\nnull\ntrue\n"
  json.encode_lines(values: [1, {name: "two"}, [true], null])? == "1\n{\"name\":\"two\"}\n[true]\nnull\n"
  let words = ["two", "three"]
  json.encode_lines([1, @words, true])? == "1\n\"two\"\n\"three\"\ntrue\n"
}

test test_json_inference_admission_dynamic_child_is_checked_at_encoding [error] {
  let dynamic_path: Any = p"item"
  test.error_kind(json.encode_lines([1, dynamic_path]), "json-compatible")?
}

test test_json_inference_admission_refuses_static_incompatible_children [error] { |ctx|
  let output = test.run_script(ctx, r"""
let lines = json.encode_lines([1, Path("item")])?
print lines
""")?
  output.status == 2
  "check.json-compatible" in output.stderr
  output.stdout == ""
}

test test_json_inference_admission_refuses_non_list_dynamic_values_before_effects [error] { |ctx|
  let output = test.run_script(ctx, r"""
let values: Any = 1
print "executed"
let encoded = json.encode_lines(values)?
""")?
  output.status == 2
  "check.type-mismatch" in output.stderr
  output.stdout == ""
}

test test_json_inference_admission_refuses_stream_values_before_effects [error] { |ctx|
  for expression in ["json.encode(values())", "json.encode_lines([values()])", "json.encode_lines([{payload: values()}])"] {
    let source = "stream values() [] -> Stream[Int] { yield 1 }\nprint \"executed\"\nlet encoded = " + expression + "?\n"
    let output = test.run_script(ctx, source)?
    output.status == 2
    "check.json-compatible" in output.stderr
    output.stdout == ""
  }
}

test test_json_inference_admission_refuses_ordinary_enum_values [error] { |ctx|
  let output = test.run_script(ctx, r"""
enum State { Ready }
let value: State = Ready
print "executed"
let encoded = json.encode_lines([value])?
""")?
  output.status == 2
  "check.json-compatible" in output.stderr
  output.stdout == ""
}

test test_json_inference_admission_keeps_ordinary_list_inference [error] { |ctx|
  let output = test.run_script(ctx, r"""
let values = [1, "two", null, true]
let lines = json.encode_lines(values)?
print lines
""")?
  output.status == 2
  "check.type-mismatch" in output.stderr
  output.stdout == ""
}

test test_json_inference_admission_keeps_explicit_list_item_contract [error] { |ctx|
  let output = test.run_script(ctx, r"""
let values: List[Int] = [1, "two"]
let lines = json.encode_lines(values)?
print lines
""")?
  output.status == 2
  "check.type-mismatch" in output.stderr
  output.stdout == ""
}

test test_json_inference_admission_keeps_finite_splice_source_contracts [error] { |ctx|
  let ordinary = test.run_script(ctx, r"""
let words = ["two"]
print "executed"
let values = [1, @words]
""")?
  ordinary.status == 2
  "check.type-mismatch" in ordinary.stderr
  ordinary.stdout == ""
  let dynamic = test.run_script(ctx, r"""
let values: Any = 1
print "executed"
let items: List[Any] = [@values]
""")?
  dynamic.status == 2
  "check.list-splice-type" in dynamic.stderr
  dynamic.stdout == ""
}
