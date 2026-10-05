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
  test.error_kind(json.encode(number), "json-compatible")
  test.error_kind(json.encode(flag), "json-compatible")
  test.error_kind(json.encode(data), "json-compatible")
  test.error_kind(json.encode(path_keys), "json-compatible")
  test.error_kind(json.encode(duration), "json-compatible")
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

test test_typed_map_erased_updates_reject_mixed_domains { |ctx|
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
      let assertion_condition = "type-error" in output.stderr
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
