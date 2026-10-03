type JsonFloatMetric = {ratio: Float, samples: List[Float]}

type JsonNestedRow = {cpu: Int, online: Bool}

type JsonNestedRows = {cpus: List[JsonNestedRow]}

type JsonRequireConfig = {jobs: UInt}

type JsonRequireEnvelope = {cpus: List[JsonNestedRow], config: JsonRequireConfig}

test test_json_require_preserves_extra_and_nested_fields_roundtrip {
  let raw = json.decode(
    "{\"aardvark\":17,\"config\":{\"jobs\":3,\"extra\":\"kept\"},\"cpus\":[{\"cpu\":2,\"online\":true,\"extra\":false}],\"tail\":{\"kept\":true}}",
  )?
  let checked = raw.require(JsonRequireEnvelope)?
  assert checked.cpus[0].cpu == 2
  assert checked.cpus[0].online == true
  assert checked.config.jobs == 3
  assert json.get(checked, ["tail", "kept"])? == true
  assert json.encode(checked)? == "{\"aardvark\":17,\"config\":{\"extra\":\"kept\",\"jobs\":3},\"cpus\":[{\"cpu\":2,\"extra\":false,\"online\":true}],\"tail\":{\"kept\":true}}"
}

test test_json_require_checks_nested_named_record_fields {
  let valid = json.decode("{\"cpus\":[{\"cpu\":0,\"online\":true}]}")?
  assert valid.require(JsonNestedRows)?.cpus[0].online == true
  let wrong_type = json.decode("{\"cpus\":[{\"cpu\":0,\"online\":\"yes\"}]}")?
  test.error_kind(wrong_type.require(JsonNestedRows), "schema")?
  let missing_field = json.decode("{\"cpus\":[{\"cpu\":0}]}")?
  test.error_kind(missing_field.require(JsonNestedRows), "schema")?
}

test test_float_arithmetic_and_json_record_boundary {
  let ratio = 5.float() / 2.0
  var adjusted: Float = ratio
  adjusted += 0.25
  let metric = json.decode("{\"ratio\":1.5,\"samples\":[0.25,1.25]}")?.require(JsonFloatMetric)?
  let encoded = json.encode({ratio: metric.ratio, value: adjusted})?
  assert ratio.format(precision: 2) == "2.50"
  assert adjusted.floor()? == 2
  assert encoded == "{\"ratio\":1.5,\"value\":2.75}"
}

test test_json_read_write_lines_and_paths { |ctx|
  let root = test.temp_dir(ctx, name: "json")?
  let value = json.decode("{\"name\":\"pkg\",\"items\":[1,2],\"meta\":{\"ok\":true}}")?
  assert json.get(value, ["name"])? == "pkg"
  assert json.get(value, ["missing"], "fallback") == "fallback"
  let updated = json.set(value, ["meta", "status"], "ready")?
  assert json.get(updated, ["meta", "status"])? == "ready"
  let removed = json.remove(updated, ["items", 0])?
  assert json.get(removed, ["items", 0])? == 2
  assert "\"status\"" in json.encode(updated, pretty: true)?
  assert "{\"a\":1}" in json.encode_lines([{a: 1}, {a: 2}])?
  let json_path = fp"${root}/data.json"
  json.write(json_path, updated, pretty: false)?
  assert json.read(json_path)?["name"] == "pkg"
  let lines_path = fp"${root}/lines.jsonl"
  json.write_lines(lines_path, [{a: 1}, {a: 2}])?
  assert lines_path.read_text()?.count_lines() == 2
  test.error_kind(json.decode("{"), "json")?
  test.error_kind(json.get(value, ["items", "bad"]), "json-path")?
}

test test_json_decode_type_patterns_and_public_boundaries {
  let decoded = json.decode("{\"quote\":\"\\\"\",\"line\":\"a\\nb\",\"snow\":\"\\u2603\",\"music\":\"\\uD834\\uDD1E\"}")?
  assert decoded.quote == "\""

  assert decoded.line == """a
b"""

  assert decoded.snow == "\u{2603}"
  assert decoded.music == "\u{1d11e}"
  assert json.decode("1.25")?.require(Float)?.format(precision: 2) == "1.25"
  test.error_kind(json.decode("9223372036854775808"), "json")?
  assert json_label(json.decode("1")?)? == "int 1.0"
  assert json_label(json.decode("1.25")?)? == "float 1.25"
  assert json_label(json.decode("\"x\"")?)? == "str x"
  assert json_label(json.decode("null")?)? == "null"
  assert json_label(json.decode("[1,2]")?)? == "int-list"
  assert json_label(json.decode("[1,\"x\"]")?)? == "other"

  let rows = """
{"b":2}

{"a":1}
""" |> json.lines

  let encoded = json.encode({z: 1, a: 2, nested: {b: 1, a: 2}})?
  assert rows[0].b == 2
  assert rows[1].a == 1
  assert encoded == "{\"a\":2,\"nested\":{\"a\":2,\"b\":1},\"z\":1}"
  test.error_kind(json.decode("not json"), "json")?
  let data = {path: p"src"}
  let path_value: Any = data["path"]
  test.error_kind(json.encode(path_value), "json-compatible")?
}

pure json_label(value: Any) -> Result[Str] {
  match value {
    i is Int => Ok(f"int ${i.float().format(precision: 1)}")
    f is Float => Ok(f"float ${f.format(precision: 2)}")
    s is Str => Ok(f"str ${s}")
    _ is Null => Ok("null")
    _ is List[Int] => Ok("int-list")
    _ => Ok("other")
  }
}

test test_json_path_helpers_report_invalid_paths {
  let data = {items: [1]}
  test.error_kind(json.get(data, ["items", 4]), "json-path")?
  test.error_kind(json.set(data, ["items", 2], 3), "json-path")?
  test.error_kind(json.remove(data, ["missing"]), "json-path")?
  test.error_kind(json.get(data, [-1]), "json-path")?
}

test test_json_rejection_is_trace_visible { |ctx|
  let output = test.run_xsht_trace(
    ctx,
    """
let data = {path: Path("src")}
let value: Any = data["path"]
let _encoded = json.encode(value) ?
""",
    ["--trace", "--raw"],
  )?

  assert output.status == 3
  assert "kind=result.propagate" in output.stderr
  assert "json-compatible" in output.stderr
  assert "Path is not JSON-compatible" in output.stderr
  assert "traceback" in output.stderr
}

# Reads the message out of a rejected `json.*` path entry.
#
# The entries that answer with `Result[Any]` cannot report a rejection through
# `Err`: their payload type lowers to `Any`, and a lowered return whose value
# matches an `Any` payload is wrapped back into `Ok`, so a rejection arrives as
# a value rather than as the `Err` the body produced. Reading the message out of
# either shape is how these tests compare the baseline spellings from
# `src/modules/json.rs`.
pure rejection_message(outcome: Result[Any]) -> Result[Str] {
  match outcome {
    Ok(value) => {
      match value {
        Ok(inner) => Ok(inner.message)
        Err(failure) => Ok(failure.message)
        _ => Ok("no rejection")
      }
    }
    Err(failure) => Ok(failure.message)
  }
}

# An update must rebuild the container it walked into: a `Map` stays a `Map`
# and a `Record` stays a `Record`, with no conversion and no JSON round-trip.
proc expect_map(label: Str, value: Any) [error] {
  match value {
    _ is Map[Any] => return test.ok(true, label)
    _ => return test.fail(f"${label}: a Map came back as another container")
  }
}

proc expect_record(label: Str, value: Any) [error] {
  match value {
    _ is Record => return test.ok(true, label)
    _ => return test.fail(f"${label}: a Record came back as another container")
  }
}

test test_json_path_get_walks_records_and_lists {
  let value = json.decode(
    "{\"name\":\"pkg\",\"items\":[1,2],\"meta\":{\"ok\":true},\"nil\":null,\"deep\":{\"rows\":[{\"cell\":7}]}}",
  )?

  # An empty path reads the value itself: nothing is traversed, nothing is
  # copied.
  assert json.get(value, [])? == value

  # A key step reads a field, an index step reads an element.
  assert json.get(value, ["name"])? == "pkg"
  assert json.get(value, ["meta", "ok"])? == true
  assert json.get(value, ["items", 0])? == 1
  assert json.get(value, ["items", 1])? == 2

  # A mixed path descends through a record, a list, and a record.
  assert json.get(value, ["deep", "rows", 0, "cell"])? == 7

  # A missing key is reported by the step that needed it.
  assert rejection_message(json.get(value, ["absent"]))? == "missing object key `absent`"
  assert rejection_message(json.get(value, ["absent", "deeper"]))? == "missing object key `absent`"

  # A `null` member is a value, not an absence.
  assert json.get(value, ["nil"])? == null
  assert json.get(value, ["nil"], "fallback") == null

  # An out-of-range index is rejected; the last index is not.
  assert rejection_message(json.get(value, ["items", 2]))? == "list index 2 out of bounds"

  # The step decides which shape is required of the value it lands on.
  assert rejection_message(json.get(value, ["name", "x"]))? == "expected object at key `x`, found Str"
  assert rejection_message(json.get(value, ["name", 0]))? == "expected list at index 0, found Str"
  assert rejection_message(json.get(value, ["items", "x"]))? == "expected object at key `x`, found List"
}

test test_json_path_get_keeps_maps_and_passes_values_through {
  let empty: Map[Any] = {}
  let tree: Any = empty.set("inner", empty.set("leaf", 0)).set("nil", null)
  assert json.get(tree, ["inner", "leaf"])? == 0
  assert json.get(tree, ["nil"])? == null
  assert rejection_message(json.get(tree, ["inner", "absent"]))? == "missing object key `absent`"
  expect_map("get over a Map keeps it a Map", json.get(tree, [])?)?

  # Values that are not JSON at all are ordinary members: the walk returns them
  # and never inspects or converts them.
  let holder: Any = {where: p"src", raw: b"raw", count: 1}
  assert json.get(holder, ["where"])? == p"src"
  assert json.get(holder, ["raw"])? == b"raw"
  assert json.get(holder, [])? == holder

  # A path may cross a record, a list, and a map in one walk, and reading,
  # updating, and removing each keep the container they visited.
  let branch: Any = empty.set("cells", [1, 2])
  let mixed: Any = {rows: [branch]}
  assert json.get(mixed, ["rows", 0, "cells", 1])? == 2
  assert json.get(json.set(mixed, ["rows", 0, "cells", 1], 9)?, ["rows", 0, "cells", 1])? == 9
  assert json.get(json.remove(mixed, ["rows", 0, "cells", 0])?, ["rows", 0, "cells", 0])? == 2
  expect_map("a mixed path keeps the Map", json.get(mixed, ["rows", 0])?)?
}

test test_json_path_is_interpreted_before_traversal {
  let value = json.decode("{\"name\":\"pkg\",\"items\":[1,2]}")?

  # Indexes are positions, so a negative one is rejected outright.
  assert rejection_message(json.get(value, [-1]))? == "path list indexes must be non-negative"
  assert rejection_message(json.get(value, ["items", -1]))? == "path list indexes must be non-negative"

  # Only keys and indexes are segments.
  assert rejection_message(json.get(value, [1.5]))? == "path segments must be Str or Int, found Float"
  assert rejection_message(json.get(value, [null]))? == "path segments must be Str or Int, found Null"

  # The whole path is interpreted before it is walked: `name` is a `Str`, so
  # traversal would fail at the second step, yet the invalid segment is what all
  # three entries report.
  assert json.get(value, ["name", "x"], "fallback") == "fallback"
  assert rejection_message(json.get(value, ["name", 1.5]))? == "path segments must be Str or Int, found Float"
  assert rejection_message(json.set(value, ["name", 1.5], 1))? == "path segments must be Str or Int, found Float"
  assert rejection_message(json.remove(value, ["name", 1.5]))? == "path segments must be Str or Int, found Float"
  assert rejection_message(json.set(value, [-1], 1))? == "path list indexes must be non-negative"
  assert rejection_message(json.remove(value, [null]))? == "path segments must be Str or Int, found Null"

  # The rejections carry the `json-path` kind as well as the message.
  test.error_kind(json.get(value, [-1]), "json-path")?
  test.error_kind(json.set(value, [1.5], 1), "json-path")?
  test.error_kind(json.remove(value, [null]), "json-path")?
}

test test_json_set_updates_the_named_position {
  let value = json.decode("{\"name\":\"pkg\",\"items\":[1,2],\"meta\":{\"ok\":true},\"nil\":null}")?

  # An empty path replaces the value itself.
  assert json.set(value, [], "whole")? == "whole"

  # A leaf key is replaced in place, and a missing leaf key is added.
  assert json.get(json.set(value, ["nil"], 5)?, ["nil"])? == 5
  assert json.get(json.set(value, ["added"], 1)?, ["added"])? == 1
  assert json.get(json.set(value, ["meta", "ok"], false)?, ["meta", "ok"])? == false

  # A list index is replaced in place; a list is neither grown nor shrunk.
  assert json.get(json.set(value, ["items", 0], 9)?, ["items", 0])? == 9
  assert json.get(json.set(value, ["items", 0], 9)?, ["items", 1])? == 2
  assert rejection_message(json.set(value, ["items", 2], 9))? == "list index 2 out of bounds"

  # A missing intermediate key cannot be created, whatever the next step is.
  assert rejection_message(json.set(value, ["absent", "deep"], 1))? == "missing intermediate object key `absent`"
  assert rejection_message(json.set(value, ["absent", 0], 1))? == "missing intermediate object key `absent`"

  # The step decides which shape is required of the value it lands on.
  assert rejection_message(json.set(value, ["items", "x"], 1))? == "expected object at key `x`, found List"
  assert rejection_message(json.set(value, ["name", 0], 1))? == "expected list at index 0, found Str"

  # The result is the same kind of container, rebuilt once per changed field.
  expect_record("set keeps a Record", json.set(value, ["added"], 1)?)?
  assert json.get(json.set(value, ["added"], 1)?, ["name"])? == "pkg"
  expect_record("set keeps a nested Record", json.get(json.set(value, ["meta", "ok"], false)?, ["meta"])?)?
  let empty: Map[Any] = {}
  let tree: Any = empty.set("inner", empty.set("leaf", 0)).set("other", 1)
  expect_map("set keeps a Map", json.set(tree, ["other"], 2)?)?
  expect_map("set adds to a Map", json.set(tree, ["fresh"], 3)?)?
  expect_map("set keeps a nested Map", json.set(tree, ["inner", "leaf"], 5)?.require(Map[Any])?.get("inner") ?? null)?
  assert json.get(json.set(tree, ["inner", "leaf"], 5)?, ["inner", "leaf"])? == 5
  assert json.get(json.set(tree, ["inner", "leaf"], 5)?, ["other"])? == 1

  # The value and the replacement must both be JSON-encodable, checked before
  # anything is updated.
  let path_value: Any = p"src"
  test.error_kind(json.set(value, ["added"], path_value), "json-compatible")?
  test.error_kind(json.set(path_value, ["added"], 1), "json-compatible")?
  let holder: Any = {where: p"src"}
  test.error_kind(json.set(holder, ["where"], 1), "json-compatible")?
  test.error_kind(json.set(value, [-1], path_value), "json-compatible")?
}

test test_json_remove_drops_the_named_position {
  let value = json.decode("{\"name\":\"pkg\",\"items\":[1,2],\"meta\":{\"ok\":true},\"nil\":null}")?

  # An empty path removes nothing and answers `null`.
  assert json.remove(value, [])? == null

  # A leaf key is dropped, at any depth.
  assert json.get(json.remove(value, ["name"])?, ["name"], "gone") == "gone"
  assert json.get(json.remove(value, ["nil"])?, ["nil"], "gone") == "gone"
  assert json.get(json.remove(value, ["meta", "ok"])?, ["meta", "ok"], "gone") == "gone"
  assert json.get(json.remove(value, ["meta", "ok"])?, ["name"])? == "pkg"

  # A list element is dropped and the list shifts left.
  assert json.get(json.remove(value, ["items", 0])?, ["items", 0])? == 2
  assert rejection_message(json.remove(value, ["items", 2]))? == "list index 2 out of bounds"

  # A missing leaf key and a missing intermediate key are distinct, and removal
  # never creates one.
  assert rejection_message(json.remove(value, ["absent"]))? == "missing object key `absent`"
  assert rejection_message(json.remove(value, ["absent", "deep"]))? == "missing intermediate object key `absent`"
  assert rejection_message(json.remove(value, ["meta", "absent"]))? == "missing object key `absent`"
  assert rejection_message(json.remove(value, ["absent", 0]))? == "missing intermediate object key `absent`"

  # The step decides which shape is required of the value it lands on.
  assert rejection_message(json.remove(value, ["name", 0]))? == "expected list at index 0, found Str"
  assert rejection_message(json.remove(value, ["items", "x"]))? == "expected object at key `x`, found List"

  # The result is the same kind of container.
  expect_record("remove keeps a Record", json.remove(value, ["name"])?)?
  let empty: Map[Any] = {}
  let tree: Any = empty.set("inner", empty.set("leaf", 0)).set("other", 1)
  expect_map("remove keeps a Map", json.remove(tree, ["other"])?)?

  # The path operation has a dynamic result. Validate its Map identity before
  # calling Map methods; nested values remain dynamic until checked separately.
  let pruned = json.remove(tree, ["inner", "leaf"])?.require(Map[Any])?
  expect_map("remove keeps a nested Map", pruned.get("inner") ?? null)?
  assert "leaf" not in (pruned.get("inner") ?? null).require(Map[Any])?
  assert json.get(pruned, ["inner", "leaf"], "gone") == "gone"

  # Members the JSON codec would reject are ordinary values: they survive a
  # removal that does not name them.
  let holder: Any = {where: p"src", raw: b"raw", count: 1}
  let kept = json.remove(holder, ["count"])?
  assert json.get(kept, ["where"])? == p"src"
  assert json.get(kept, ["raw"])? == b"raw"
}

test test_json_get_overloads_and_encoded_lines {
  let value = json.decode("{\"name\":\"pkg\",\"items\":[1,2],\"nil\":null}")?

  # The two-argument overload answers with the read or with the rejection.
  assert json.get(value, ["name"])? == "pkg"
  assert rejection_message(json.get(value, ["absent"]))? == "missing object key `absent`"
  assert rejection_message(json.get(value, ["items", 2]))? == "list index 2 out of bounds"

  # The three-argument overload answers with the fallback whenever the path is
  # rejected, whatever the step was.
  assert json.get(value, ["absent"], "fallback") == "fallback"
  assert json.get(value, ["absent", "deep"], "fallback") == "fallback"
  assert json.get(value, ["items", 2], "fallback") == "fallback"
  assert json.get(value, ["items", "x"], "fallback") == "fallback"
  assert json.get(value, [-1], "fallback") == "fallback"
  assert json.get(value, [1.5], "fallback") == "fallback"
  assert json.get(value, [], "fallback") == value
  assert json.get(value, ["name"], "fallback") == "pkg"
  assert json.get(value, ["nil"], "fallback") == null

  # Every item is encoded compactly and each is followed by a newline.
  assert json.encode_lines([])? == ""
  assert json.encode_lines([1])? == """1
"""
  assert json.encode_lines([1, "two", null, true])? == """1
"two"
null
true
"""
  assert json.encode_lines(["a\"b"])? == """"a\\"b"
"""
  assert json.encode_lines(
    [
  """a
b""",
],
  )? == """"a\\nb"
"""
  assert json.encode_lines([value, value])? == """{"items":[1,2],"name":"pkg","nil":null}
{"items":[1,2],"name":"pkg","nil":null}
"""

  # An item that cannot be encoded fails the whole composition, whichever item
  # it is; the message and the empty output are checked in the trace test.
  let path_value: Any = p"src"
  test.error_kind(json.encode_lines([path_value]), "json-compatible")?
  test.error_kind(json.encode_lines([1, path_value]), "json-compatible")?
  test.error_kind(json.encode_lines([path_value, 1]), "json-compatible")?
}

test test_json_path_rejects_a_non_list_path { |ctx|
  # The public path parameter requires a checked List shape. An unchecked
  # dynamic path is rejected before process execution or runtime path traversal.
  let read = test.run_xsht_trace(
    ctx,
    """
let value: Any = {a: 1}
let where: Any = "a"
let _read = json.get(value, where) ?
""",
    ["--trace", "--raw"],
  )?
  assert read.status == 2
  assert "check.dynamic-boundary" in read.stderr
  assert "List[Any]" in read.stderr

  # A fallback value does not validate an unchecked path argument.
  let with_fallback = test.run_xsht_trace(
    ctx,
    """
let value: Any = {a: 1}
let where: Any = "a"
let _read = json.get(value, where, "fallback") ?
""",
    ["--trace", "--raw"],
  )?
  assert with_fallback.status == 2
  assert "check.dynamic-boundary" in with_fallback.stderr
  assert "List[Any]" in with_fallback.stderr

  # An item that cannot be encoded fails the whole composition, and the failure
  # is reported before any output exists.
  let lines = test.run_xsht_trace(
    ctx,
    """
let bad: Any = Path("src")
let items: List[Any] = [1, bad]
let _lines = json.encode_lines(items) ?
""",
    ["--trace", "--raw"],
  )?
  assert lines.status == 3
  assert lines.stdout == ""
  assert "json-compatible" in lines.stderr
  assert "Path is not JSON-compatible" in lines.stderr
}

test test_json_lines_result_retains_list_type_in_record_fields { |ctx|
  let output = test.run_script(
    ctx,
    r"""
type Event = {event: Str}
let decoded_events = "{\"event\":\"start\"}\n{\"event\":\"stop\"}\n" |> json.lines
let second = decoded_events[1].require(Event)?
let summary = {events: decoded_events.len(), complete: second.event == "stop"}
print f"${summary.events}:${summary.complete}"
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """2:true
"""
}

test test_json_set_keeps_path_segments_and_runtime_validation {
  let input = {rows: [{name: "first"}]}
  assert json.encode(json.set(input, ["rows", 0, "name"], "second")?)? == "{\"rows\":[{\"name\":\"second\"}]}"
  test.error_kind(json.set(input, ["rows", 1.5], "second"), "json-path")?
}
