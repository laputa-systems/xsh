pure typed_unsigned_keys(keys: List[UInt]) -> List[UInt] {
  keys
}

test test_typed_map_integer_keys_order_and_lookup { |ctx|
  let output = test.run_script(
    ctx,
    r"""
var numbers: Map[Int, Str] = {}
numbers[20] = "twenty"
numbers[3] = "three"
for {key, value} in numbers {
  print $key $value
}
print (numbers.get(3)?)
print (numbers.get(99) ?? "missing")
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """3 three
20 twenty
three
missing
"""
}

type Identifier = Int

test test_typed_map_scalar_domains_aliases_and_updates {
  var numbers: Map[Identifier, Str] = {[20]: "twenty", [3]: "three", [-1]: "negative"}
  let snapshot = numbers
  numbers[3] = "changed"
  assert numbers.keys() == [-1, 3, 20]
  assert numbers.values() == ["negative", "changed", "twenty"]
  assert snapshot[3] == "three"
  assert 3 in numbers
  assert 99 not in numbers
  assert numbers.remove(3).keys() == [-1, 20]
  let flags: Map[Bool, Int] = {[true]: 1, [false]: 0}
  assert flags.keys() == [false, true]
  let data: Map[Bytes, Int] = {[b"z"]: 2, [b"a"]: 1}
  assert data.keys() == [b"a", b"z"]
  assert data.get(b"a")? == 1
  let paths: Map[Path, Str] = {[p"z"]: "last", [p"a"]: "first"}
  assert paths.keys() == [p"a", p"z"]
  assert paths[p"a"] == "first"
  let delays: Map[Duration, Int] = {[20ms]: 2, [3ms]: 1}
  assert delays.keys() == [3ms, 20ms]
  assert delays[3ms] == 1
  let unsigned: Map[UInt, Str] = {[20]: "twenty", [3]: "three"}
  let expected_unsigned = typed_unsigned_keys([3, 20])
  assert unsigned.keys() == expected_unsigned
  assert unsigned.keys()[0] + 1 == 4
  let strings: Map[Str] = {a: "first", z: "last"}
  assert strings.keys() == ["a", "z"]
}

test test_typed_map_inference_comprehension_null_and_empty {
  let inferred = {[20]: "twenty", [3]: "three"}
  let first_key = inferred.keys()[0]
  let first_value = inferred.values()[0]
  assert first_key == 3
  assert first_value == "three"
  let comprehension = {item: f"{item}" for item in [20, 3]}
  assert comprehension.keys() == [3, 20]
  let entries = [key for {key, value} in comprehension if value != "20"]
  assert entries == [3]
  let nullable: Map[Int, Str?] = {[1]: null}
  assert nullable.get(1)? == null
  test.error_kind(nullable.get(2), "map-missing")
  let empty: Map[Int, Str] = {}
  assert empty.keys() == []
  assert empty.set(3, "three").get(3)? == "three"
  let nested: Map[Int, List[Int]] = {[1]: [2, 3]}
  var changed = nested
  changed[1][0] = 9
  assert changed[1] == [9, 3]
  assert nested[1] == [2, 3]
}

test test_typed_map_rejects_mixed_and_unsupported_keys { |ctx|
  for source in [
    """let mixed = {[1]: 1, ["one"]: 2}
""",
    """let invalid: Map[Float, Int] = {}
""",
    """let invalid: Map[Any, Int] = {}
""",
    """let invalid: Map[List[Int], Int] = {}
""",
    """let values: Map[Int, Str] = {}
let invalid = values.get("one")
""",
    """let values: Map[Int, Str] = {}
let invalid = values.set(1, false)
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert ! output.success
    assert "check." in output.stderr
  }
}

test test_typed_map_json_rejects_non_string_keys {
  let number: Any = {[1]: 1}
  let flag: Any = {[false]: 1}
  let data: Any = {[b"a"]: 1}
  let path_keys: Any = {[p"a"]: 1}
  let duration: Any = {[3ms]: 1}
  test.error_kind(json.encode(number), "CodecError.UnsupportedValue")
  test.error_kind(json.encode(flag), "CodecError.UnsupportedValue")
  test.error_kind(json.encode(data), "CodecError.UnsupportedValue")
  test.error_kind(json.encode(path_keys), "CodecError.UnsupportedValue")
  test.error_kind(json.encode(duration), "CodecError.UnsupportedValue")
  let encoded = {[f"{key}"]: value for {key, value} in {[1]: 2}}
  assert json.encode(encoded)? == "{\"1\":2}"
}

type UnsignedIdentifier = UInt

test test_typed_map_unsigned_keys_reject_negative_boundaries { |ctx|
  for source in [
    """let values: Map[UInt, Str] = {[-1]: "bad"}
""",
    """var values: Map[UInt, Str] = {}
values[-1] = "bad"
""",
    """let values: Map[UInt, Str] = {}
print (values.set(-1, "bad").len())
""",
    """let values: Map[UInt, Str] = {}
print (values.get(-1) ?? "missing")
""",
    """type Identifier = UInt
var values: Map[Identifier, Str] = {}
values[-1] = "bad"
""",
    """let values: Map[UInt, Str] = {}
print (-1 in values)
""",
    """let values: Map[UInt, Str] = {key: "bad" for key in [-1]}
""",
    """var values: Map[UInt, Str] = {}
let bad: Map[Int, Str] = {[-1]: "bad"}
values = bad
""",
  ] {
    let output = test.run_script(ctx, source)?
    {
      let assertion_condition = ! output.success
      let assertion_message = source
      assert assertion_condition, assertion_message
    }
    {
      let assertion_condition = "UInt" in output.stderr
      let assertion_message = output.stderr
      assert assertion_condition, assertion_message
    }
  }

  let values: Map[UnsignedIdentifier, Str] = {[3]: "three"}
  assert values.get(3)? == "three"
}

test test_typed_map_path_and_bytes_keep_native_identity {
  let first = b"\xff" as Path
  let second = b"\xfe" as Path
  let paths: Map[Path, Int] = {[first]: 1, [second]: 2}
  assert paths.len() == 2
  assert paths.keys() == [second, first]
  assert paths.get(first)? == 1
  assert paths.get(second)? == 2
  let data: Map[Bytes, Int] = {[b"\xff"]: 1, [b"\xfe"]: 2}
  assert data.keys() == [b"\xfe", b"\xff"]
  assert data.get(b"\xff")? == 1
}

# This case used to observe a map-type runtime error through `Any.set`; the
# unchecked receiver now fails at the language boundary before mutation.
test test_typed_map_erased_updates_require_validation { |ctx|
  for source in [
    """let values: Any = {[1]: 2}
let _ = values.set("one", 3)
""",
  ] {
    let output = test.run_script(ctx, source)?
    {
      let assertion_condition = ! output.success
      let assertion_message = source
      assert assertion_condition, assertion_message
    }
    {
      let assertion_condition = "check.dynamic-boundary" in output.stderr
      let assertion_message = output.stderr
      assert assertion_condition, assertion_message
    }
  }
}

const prepared_numeric_keys: Map[Int, Str] = {[20]: "twenty", [3]: "three"}
const prepared_flag_keys = {[true]: 1, [false]: 0}
const prepared_byte_keys = {[b"\xff"]: 1, [b"\xfe"]: 2}
const prepared_path_keys: Map[Path, Int] = {[p"z"]: 2, [p"a"]: 1}
const prepared_duration_keys = {[20ms]: 2, [3ms]: 1}
const prepared_unsigned_keys: Map[UInt, Str] = {[3]: "three"}

test test_typed_map_prepared_constants_keep_scalar_keys {
  assert prepared_numeric_keys.keys() == [3, 20]
  assert prepared_numeric_keys.get(3)? == "three"
  assert prepared_flag_keys.keys() == [false, true]
  assert prepared_byte_keys.keys() == [b"\xfe", b"\xff"]
  assert prepared_path_keys.keys() == [p"a", p"z"]
  assert prepared_duration_keys.keys() == [3ms, 20ms]
  assert prepared_unsigned_keys.get(3)? == "three"
}

test test_typed_map_prepared_constants_reject_mixed_and_unsigned_negative_keys { |ctx|
  for source in [
    """const values = {[1]: 1, ["one"]: 2}
""",
    """const values: Map[UInt, Str] = {[-1]: "bad"}
""",
    """type Key = UInt
const values: Map[Key, Str] = {[-1]: "bad"}
""",
  ] {
    let result = test.run_script(ctx, source)?
    {
      let assertion_condition = ! result.success
      let assertion_message = source
      assert assertion_condition, assertion_message
    }
    {
      let assertion_condition = "check." in result.stderr
      let assertion_message = result.stderr
      assert assertion_condition, assertion_message
    }
  }
}

test test_typed_map_unsigned_key_failure_is_fatal_in_a_proc_without_error { |ctx|
  let caller = """
proc main() [io, error] {
  let values: Map[UInt, Str] = {[1]: "one"}
  print (try { keyed(values, 1) } ?? "unexpected")
  print (try { keyed(values, -1) } ?? "captured")
}
"""
  for site in [
    """proc keyed(values: Map[UInt, Str], key: Int) [io] -> Str { values[key] }""",
    """proc keyed(values: Map[UInt, Str], key: Int) -> Str { values[key] }""",
    """proc keyed(values: Map[UInt, Str], key: Int) [io] -> Str { values.get(key) ?? "missing" }""",
    """proc keyed(values: Map[UInt, Str], key: Int) [io] -> Str { if key in values { "present" } else { "absent" } }""",
    """proc keyed(values: Map[UInt, Str], key: Int) [io] -> Str { var copy = values; copy[key] = "set"; copy[key] }""",
    """proc keyed(values: Map[UInt, Str], key: Int) [io] -> Str { let copy: Map[UInt, Str] = {[key]: "literal"}; copy.values()[0] }""",
    """proc keyed(values: Map[UInt, Str], key: Int) [io] -> Str { let copy: Map[UInt, Str] = {item: "built" for item in [key]}; copy.values()[0] }""",
  ] {
    let output = test.run_script(ctx, f"{site}\n{caller}")?
    assert ! output.success, site
    assert "schema check failed" in output.stderr, f"{site}: {output.stderr}"
    assert ! ("captured" in output.stdout), f"{site}: {output.stdout}"
    assert ! ("unexpected" in output.stdout), f"{site}: {output.stdout}"
  }
  let propagated = test.run_script(ctx, """proc keyed(values: Map[UInt, Str], key: Int) [io, error] -> Str { values[key] }""" + caller)?
  assert propagated.success, propagated.stderr
  assert propagated.stdout == "one\ncaptured\n", propagated.stdout
}

test test_typed_map_unsigned_key_failure_is_the_completion_of_a_proc_with_an_inferred_return { |ctx|
  let caller = """
proc main() [io] {
  let values: Map[UInt, Str] = {[1]: "one"}
  for key in [1, -1] {
    match keyed(values, key) {
      Ok(_) => print "ok"
      Err(error) => print f"err {error.message}"
    }
  }
}
"""
  for site in [
    """proc keyed(values: Map[UInt, Str], key: Int) { values[key] }""",
    """proc keyed(values, key: Int) { values[key] }""",
    """proc keyed(values: Map[UInt, Str], key: Int) { if key in values { "present" } else { "absent" } }""",
    """proc keyed(values: Map[UInt, Str], key: Int) { values.get(key) ?? "missing" }""",
    """proc keyed(values: Map[UInt, Str], key: Int) { var copy = values; copy[key] = "one"; copy[key] }""",
    """proc keyed(values: Map[UInt, Str], key: Int) { let copy: Map[UInt, Str] = {[key]: "one"}; copy.values()[0] }""",
    """proc keyed(values: Map[UInt, Str], key: Int) { let copy: Map[UInt, Str] = {item: "one" for item in [key]}; copy.values()[0] }""",
  ] {
    let output = test.run_script(ctx, f"{site}\n{caller}")?
    assert output.success, f"{site}: {output.stderr}"
    assert output.stdout == "ok\nerr schema check failed: expected UInt, found Int\n", f"{site}: {output.stdout}"
  }

  let captured = test.run_script(ctx, r"""proc captured(values: Map[UInt, Str], key: Int) {
  match try { values[key] } {
    Ok(value) => value
    Err(_) => "caught"
  }
}
proc retried(values: Map[UInt, Str], key: Int) {
  match retry [] { values[key] } {
    Ok(value) => value
    Err(_) => "caught"
  }
}
proc main() [io, error] {
  let values: Map[UInt, Str] = {[1]: "one"}
  assert captured(values, 1) == Ok("one")
  assert captured(values, -1) == Ok("caught")
  assert retried(values, 1) == Ok("one")
  assert retried(values, -1) == Ok("caught")
}
""")?
  assert captured.success, captured.stderr
  assert captured.stdout == "", captured.stdout
}

test test_typed_map_unsigned_key_failure_is_fatal_in_pure_non_result_calls { |ctx|
  let caller = """
proc main() [io, error] {
  let values: Map[UInt, Str] = {[1]: "one"}
  print (try { keyed(values, 1) } ?? "unexpected")
  print (try { keyed(values, -1) } ?? "captured")
}
"""
  for site in [
    """pure keyed(values: Map[UInt, Str], key: Int) -> Str { values[key] }""",
    """pure keyed(values, key: Int) { values[key] }""",
    """pure keyed(values: Map[UInt, Str], key: Int) -> Str { values.get(key) ?? "missing" }""",
    """pure keyed(values: Map[UInt, Str], key: Int) -> Str { values.get(key: key) ?? "missing" }""",
    """pure keyed(values: Map[UInt, Str], key: Int) -> Str { values.get(...{key: key}) ?? "missing" }""",
    """pure keyed(values: Map[UInt, Str], key: Int) -> Str { if key in values { "one" } else { "missing" } }""",
    """pure keyed(values, key: Int) { if key in values { "one" } else { "missing" } }""",
    """pure keyed(values: Map[UInt, Str], key: Int) -> Str { let copy: Map[UInt, Str] = {[key]: "one"}; copy.values()[0] }""",
    """pure keyed(values: Map[UInt, Str], key: Int) -> Str { let copy: Map[UInt, Str] = {item: "one" for item in [key]}; copy.values()[0] }""",
  ] {
    let output = test.run_script(ctx, f"{site}\n{caller}")?
    assert ! output.success, site
    assert "schema check failed" in output.stderr, f"{site}: {output.stderr}"
    assert output.stdout == "one\n", f"{site}: {output.stdout}"
  }
  let generic = test.run_script(ctx, r"""pure keyed(values: Map[UInt, Int], key: Int, value) { let _ = values[key]; value }
proc main() [io, error] {
  let values: Map[UInt, Int] = {[1]: 4}
  print (try { keyed(values, 1, "one") } ?? "unexpected")
  print (try { keyed(values, -1, "one") } ?? "captured")
}
""")?
  assert ! generic.success, generic.stderr
  assert "schema check failed" in generic.stderr, generic.stderr
  assert generic.stdout == "one\n", generic.stdout
}

test test_typed_map_pure_result_and_local_error_boundaries_remain_catchable { |ctx|
  let output = test.run_script(ctx, r"""pure ground(values: Map[UInt, Result[Str]], key: Int) -> Result[Str] { values[key] }
pure optional(values: Map[UInt, Result[Int]?], key: Int) -> Result[Int]? { values[key] }
pure generic(values, key: Int) { values[key] }
pure explicit(value: Int) -> Result[UInt] { value.require(UInt) }
pure relay(values: Map[UInt, Int], key: Int, value) { let _ = values[key]; value }
pure captured(values: Map[UInt, Str], key: Int) -> Str {
  match try { values[key] } { Ok(value) => value; Err(_) => "caught" }
}
pure retried(values: Map[UInt, Str], key: Int) -> Str {
  match retry [] { values[key] } { Ok(value) => value; Err(_) => "caught" }
}
proc main() [error] {
  let results: Map[UInt, Result[Str]] = {[1]: Ok("one")}
  let values: Map[UInt, Str] = {[1]: "one"}
  let numbers: Map[UInt, Int] = {[1]: 4}
  let optional_results: Map[UInt, Result[Int]?] = {[1]: Ok(7)}
  assert optional(optional_results, 1) == Ok(7)
  assert optional(optional_results, -1) != null
  assert generic(optional_results, 1) == Ok(7)
  assert generic(optional_results, -1) != null
  assert explicit(1) == Ok(1)
  match explicit(-1) { Ok(_) => assert false; Err(_) => {} }
  assert relay(numbers, 1, Ok("one")) == Ok("one")
  match relay(numbers, -1, Ok("one")) { Ok(_) => assert false; Err(_) => {} }
  assert ground(results, 1) == Ok("one")
  assert generic(results, 1) == Ok("one")
  match ground(results, -1) { Ok(_) => assert false; Err(_) => {} }
  match generic(results, -1) { Ok(_) => assert false; Err(_) => {} }
  assert captured(values, 1) == "one"
  assert captured(values, -1) == "caught"
  assert retried(values, 1) == "one"
  assert retried(values, -1) == "caught"
}
""")?
  assert output.success, output.stderr
  assert output.stdout == "", output.stdout
}

test test_pure_runtime_faults_remain_fatal_to_caller_try { |ctx|
  for item in [
    {source: r"""pure f(value: Int) -> Int { 10 / value }
proc main() [io, error] { print (try { f(0) } ?? 99) }
""", message: "division by zero"},
    {source: r"""pure f(value: Int) -> Int { value + 1 }
proc main() [io, error] { print (try { f(9223372036854775807) } ?? 99) }
""", message: "integer overflow"},
    {source: r"""pure f(values: Map[Str, Int]) -> Int { values["absent"] }
proc main() [io, error] { print (try { f({}) } ?? 99) }
""", message: "map has no key"},
  ] {
    let output = test.run_script(ctx, item.source)?
    assert ! output.success, item.source
    assert item.message in output.stderr, output.stderr
    assert output.stdout == "", output.stdout
  }
}

test test_generic_map_key_validation_preserves_return_contract { |ctx|
  for operation in [
    "let _ = values.get(key)",
    "let _ = values.get(key: key)",
    "let _ = values.get(...{key: key})",
    "let copy: Map[UInt, Str] = {[key]: \"one\"}; let _ = copy",
    "let copy: Map[UInt, Str] = {item: \"one\" for item in [key]}; let _ = copy",
    "var copy: Map[UInt, Str] = {}; copy[key] = \"one\"",
  ] {
    let rejected = test.run_script(ctx, f"""pure keyed(values: Map[UInt, Str], key: Int, marker) {{ {operation}; marker }}
let values: Map[UInt, Str] = {{[1]: "one"}}
let payload: Result[Int, Str] = Err("payload")
let result = keyed(values, -1, payload)
print "executed"
""")?
    assert ! rejected.success, operation
    assert "UInt map key validation error is outside the callable Result contract" in rejected.stderr, rejected.stderr
    assert rejected.stdout == "", rejected.stdout
    let accepted = test.run_script(ctx, f"""pure keyed(values: Map[UInt, Str], key: UInt, marker) {{ {operation}; marker }}
let values: Map[UInt, Str] = {{[1]: "one"}}
let payload: Result[Int, Str] = Err("payload")
assert keyed(values, 1, payload) == payload
""")?
    assert accepted.success, accepted.stderr
  }
  for operation in ["values.get(key: key)", "values.get(...{key: key})"] {
    let fatal = test.run_script(ctx, f"""pure keyed(values: Map[UInt, Str], key: Int, marker) {{ let _ = {operation}; marker }}
let values: Map[UInt, Str] = {{[1]: "one"}}
print (try {{ keyed(values, -1, "one") }} ?? "captured")
""")?
    assert ! fatal.success, operation
    assert "schema check failed: expected UInt, found Int" in fatal.stderr, fatal.stderr
    assert fatal.stdout == "", fatal.stdout
  }
  let valid = test.run_script(ctx, r"""pure keyed(values: Map[UInt, Str], key: UInt, marker) { let _ = values.get(key); marker }
pure captured(values: Map[UInt, Str], key: Int, marker) { let _ = try { values.get(key) }; marker }
pure propagated(values: Map[UInt, Str], key: Int, marker) { let _ = values.get(key); marker }
pure late_list(values, key: Int, marker) { let _ = values.get(key); let typed: List[Str] = values; marker }
pure late_signed(values, key: Int, marker) { let _ = values.get(key); let typed: Map[Int, Str] = values; marker }
pure late_capture(values, key: Int, marker) { let caught = try { values.get(key) }; let typed: Map[UInt, Str] = values; {marker: marker, captured: caught is Err(_)} }
let values: Map[UInt, Str] = {[1]: "one"}
let payload: Result[Int, Str] = Err("payload")
let errors: Result[Int] = Ok(7)
assert keyed(values, 1, payload) == payload
assert captured(values, -1, payload) == payload
assert late_list(["one"], -1, payload) == payload
let signed: Map[Int, Str] = {[-1]: "one"}
assert late_signed(signed, -1, payload) == payload
let caught = late_capture(values, -1, payload)
assert caught.marker == payload
assert caught.captured
assert match propagated(values, -1, errors) { Err(_) => true; Ok(_) => false }
""")?
  assert valid.success, valid.stderr
  let late = test.run_script(ctx, r"""pure keyed(values, key: Int, marker) {
  let _ = values.get(key)
  let typed: Map[UInt, Str] = values
  marker
}
let values: Map[UInt, Str] = {[1]: "one"}
let payload: Result[Int, Str] = Err("payload")
let result = keyed(values, -1, payload)
print "executed"
""")?
  assert ! late.success, late.stderr
  assert "UInt map key validation error is outside the callable Result contract" in late.stderr, late.stderr
  assert late.stdout == "", late.stdout
}

test test_generic_map_key_capture_preserves_nested_guard_scope { |ctx|
  let output = test.run_script(ctx, r"""pure guarded(values: Map[UInt, Str]?, key: Int, marker) {
  let caught = try { if let present = values { let _ = present.get(...{key: key}) }; assert true }
  {marker: marker, captured: match caught { Err(error) => error.message == "schema check failed: expected UInt, found Int"; Ok(_) => false }}
}
let values: Map[UInt, Str] = {[1]: "one"}
let payload: Result[Int, Str] = Err("payload")
let absent = guarded(null, -1, payload)
let invalid = guarded(values, -1, payload)
let valid = guarded(values, 1, payload)
assert ! absent.captured
assert invalid.captured
assert ! valid.captured
assert absent.marker == payload
assert invalid.marker == payload
assert valid.marker == payload
""")?
  assert output.success, output.stderr
}

test test_late_map_method_key_validation_keeps_proc_completion { |ctx|
  let output = test.run_script(ctx, r"""proc map_key(values, key: Int) {
  let _ = values.get(key)
  let typed: Map[UInt, Str] = values
  "one"
}
proc list_key(values, key: Int) {
  let _ = values.get(key)
  let typed: List[Str] = values
  "one"
}
proc main() [error] {
  let values: Map[UInt, Str] = {[1]: "one"}
  assert map_key(values, 1)? == "one"
  assert match map_key(values, -1) { Err(error) => error.message == "schema check failed: expected UInt, found Int"; Ok(_) => false }
  assert list_key(["one"], -1)? == "one"
}
""")?
  assert output.success, output.stderr
}
