pure typed_unsigned_keys(keys: List[UInt]) -> List[UInt] { keys }

test test_typed_map_integer_keys_order_and_lookup [error] { |ctx|
  let output = test.run_script(ctx, r"""
var numbers: Map[Int, Str] = {}
numbers[20] = "twenty"
numbers[3] = "three"
for {key, value} in numbers {
  print $key $value
}
print (numbers.get(3)?)
print (numbers.get(99) ?? "missing")
""")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  (output.stdout) == ("3 three\n20 twenty\nthree\nmissing\n")
}

type Identifier = Int

test test_typed_map_scalar_domains_aliases_and_updates [error] {
  var numbers: Map[Identifier, Str] = { [20]: "twenty", [3]: "three", [-1]: "negative"}
  let snapshot = numbers
  numbers[3] = "changed"
  (numbers.keys()) == ([-1, 3, 20])
  (numbers.values()) == (["negative", "changed", "twenty"])
  (snapshot[3]) == ("three")
  (3 in numbers)
  (99 not in numbers)
  (numbers.remove(3).keys()) == ([-1, 20])
  let flags: Map[Bool, Int] = { [true]: 1, [false]: 0}
  (flags.keys()) == ([false, true])
  let data: Map[Bytes, Int] = { [b"z"]: 2, [b"a"]: 1}
  (data.keys()) == ([b"a", b"z"])
  (data.get(b"a")?) == (1)
  let paths: Map[Path, Str] = { [p"z"]: "last", [p"a"]: "first"}
  (paths.keys()) == ([p"a", p"z"])
  (paths[p"a"]) == ("first")
  let delays: Map[Duration, Int] = { [20ms]: 2, [3ms]: 1}
  (delays.keys()) == ([3ms, 20ms])
  (delays[3ms]) == (1)
  let unsigned: Map[UInt, Str] = { [20]: "twenty", [3]: "three"}
  let expected_unsigned = typed_unsigned_keys([3, 20])
  unsigned.keys() == expected_unsigned
  (unsigned.keys()[0] + 1) == (4)
  let strings: Map[Str] = {a: "first", z: "last"}
  (strings.keys()) == (["a", "z"])
}

test test_typed_map_inference_comprehension_null_and_empty [error] {
  let inferred = { [20]: "twenty", [3]: "three"}
  let first_key = inferred.keys()[0]
  let first_value = inferred.values()[0]
  (first_key) == (3)
  (first_value) == ("three")
  let comprehension = {item: f"$item" for item in [20, 3]}
  (comprehension.keys()) == ([3, 20])
  let entries = [key for {key, value} in comprehension if value != "20"]
  (entries) == ([3])
  let nullable: Map[Int, Str?] = { [1]: null}
  (nullable.get(1)?) == (null)
  test.error_kind(nullable.get(2), "map-missing")?
  let empty: Map[Int, Str] = {}
  (empty.keys()) == ([])
  (empty.set(3, "three").get(3)?) == ("three")
  let nested: Map[Int, List[Int]] = { [1]: [2, 3]}
  var changed = nested
  changed[1][0] = 9
  (changed[1]) == ([9, 3])
  (nested[1]) == ([2, 3])
}

test test_typed_map_rejects_mixed_and_unsupported_keys [error] { |ctx|
  for source in [
    "let mixed = {[1]: 1, [\"one\"]: 2}\n",
    "let invalid: Map[Float, Int] = {}\n",
    "let invalid: Map[Any, Int] = {}\n",
    "let invalid: Map[List[Int], Int] = {}\n",
    "let values: Map[Int, Str] = {}\nlet invalid = values.get(\"one\")\n",
    "let values: Map[Int, Str] = {}\nlet invalid = values.set(1, false)\n",
  ] {
    let output = test.run_script(ctx, source)?
    (!output.success)
    (("check." in output.stderr))
  }
}

test test_typed_map_json_rejects_non_string_keys [error] {
  let number: Any = {[1]: 1}
  let flag: Any = {[false]: 1}
  let data: Any = {[b"a"]: 1}
  let path_keys: Any = {[p"a"]: 1}
  let duration: Any = {[3ms]: 1}
  test.error_kind(json.encode(number), "json-compatible")?
  test.error_kind(json.encode(flag), "json-compatible")?
  test.error_kind(json.encode(data), "json-compatible")?
  test.error_kind(json.encode(path_keys), "json-compatible")?
  test.error_kind(json.encode(duration), "json-compatible")?
  let encoded = {[f"${key}"]: value for {key, value} in {[1]: 2}}
  (json.encode(encoded)?) == ("{\"1\":2}")
}

type UnsignedIdentifier = UInt

test test_typed_map_unsigned_keys_reject_negative_boundaries [error] { |ctx|
  for source in [
    "let values: Map[UInt, Str] = {[-1]: \"bad\"}\n",
    "var values: Map[UInt, Str] = {}\nvalues[-1] = \"bad\"\n",
    "let values: Map[UInt, Str] = {}\nprint (values.set(-1, \"bad\").len())\n",
    "let values: Map[UInt, Str] = {}\nprint (values.get(-1) ?? \"missing\")\n",
    "type Identifier = UInt\nvar values: Map[Identifier, Str] = {}\nvalues[-1] = \"bad\"\n",
    "let values: Map[UInt, Str] = {}\nprint (-1 in values)\n",
    "let values: Map[UInt, Str] = {key: \"bad\" for key in [-1]}\n",
    "var values: Map[UInt, Str] = {}\nlet bad: Map[Int, Str] = {[-1]: \"bad\"}\nvalues = bad\n",
  ] {
    let output = test.run_script(ctx, source)?
    {
      let assertion_condition = !output.success
      let assertion_message = source
      assert assertion_condition, assertion_message
    }
    {
      let assertion_condition = ("UInt" in output.stderr)
      let assertion_message = output.stderr
      assert assertion_condition, assertion_message
    }
  }
  let values: Map[UnsignedIdentifier, Str] = {[3]: "three"}
  (values.get(3)?) == ("three")
}

test test_typed_map_path_and_bytes_keep_native_identity [error] {
  let first = Path.parse_bytes(b"\xff")?
  let second = Path.parse_bytes(b"\xfe")?
  let paths: Map[Path, Int] = {[first]: 1, [second]: 2}
  (paths.len()) == (2)
  (paths.keys()) == ([second, first])
  (paths.get(first)?) == (1)
  (paths.get(second)?) == (2)
  let data: Map[Bytes, Int] = {[b"\xff"]: 1, [b"\xfe"]: 2}
  (data.keys()) == ([b"\xfe", b"\xff"])
  (data.get(b"\xff")?) == (1)
}

test test_typed_map_erased_updates_reject_mixed_domains [error] { |ctx|
  for source in [
    "let values: Any = {[1]: 2}\nprint (values.set(\"one\", 3))\n",
  ] {
    let output = test.run_script(ctx, source)?
    {
      let assertion_condition = !output.success
      let assertion_message = source
      assert assertion_condition, assertion_message
    }
    {
      let assertion_condition = ("type-error" in output.stderr)
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

test test_typed_map_prepared_constants_keep_scalar_keys [error] {
  (prepared_numeric_keys.keys()) == ([3, 20])
  (prepared_numeric_keys.get(3)?) == ("three")
  (prepared_flag_keys.keys()) == ([false, true])
  (prepared_byte_keys.keys()) == ([b"\xfe", b"\xff"])
  (prepared_path_keys.keys()) == ([p"a", p"z"])
  (prepared_duration_keys.keys()) == ([3ms, 20ms])
  (prepared_unsigned_keys.get(3)?) == ("three")
}

test test_typed_map_prepared_constants_reject_mixed_and_unsigned_negative_keys [error] { |ctx|
  for source in [
    "const values = {[1]: 1, [\"one\"]: 2}\n",
    "const values: Map[UInt, Str] = {[-1]: \"bad\"}\n",
    "type Key = UInt\nconst values: Map[Key, Str] = {[-1]: \"bad\"}\n",
  ] {
    let result = test.run_script(ctx, source)?
    {
      let assertion_condition = !result.success
      let assertion_message = source
      assert assertion_condition, assertion_message
    }
    {
      let assertion_condition = ("check." in result.stderr)
      let assertion_message = result.stderr
      assert assertion_condition, assertion_message
    }
  }
}
