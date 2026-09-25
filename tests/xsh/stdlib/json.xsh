type JsonFloatMetric = {ratio: Float, samples: List[Float]}

proc test_float_arithmetic_and_json_record_boundary() [error] {
  let ratio = 5.float() / 2.0
  var adjusted: Float = ratio
  adjusted += 0.25
  let metric = json.decode("{\"ratio\":1.5,\"samples\":[0.25,1.25]}")?.require(JsonFloatMetric)?
  let encoded = json.encode({ratio: metric.ratio, value: adjusted})?
  test.eq(ratio.format(precision: 2), "2.50")?
  test.eq(adjusted.floor()?, 2)?
  test.eq(encoded, "{\"ratio\":1.5,\"value\":2.75}")?
}

proc test_json_read_write_lines_and_paths(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "json")?
  let value = json.decode("{\"name\":\"pkg\",\"items\":[1,2],\"meta\":{\"ok\":true}}")?
  test.eq(json.get(value, ["name"])?, "pkg")?
  test.eq(json.get(value, ["missing"], "fallback"), "fallback")?
  let updated = json.set(value, ["meta", "status"], "ready")?
  test.eq(json.get(updated, ["meta", "status"])?, "ready")?
  let removed = json.remove(updated, ["items", 0])?
  test.eq(json.get(removed, ["items", 0])?, 2)?
  test.contains(json.encode(updated, pretty: true)?, "\"status\"")?
  test.contains(json.encode_lines([{a: 1}, {a: 2}])?, "{\"a\":1}")?
  let json_path = fp"${root}/data.json"
  json.write(json_path, updated, pretty: false)?
  test.eq(json.read(json_path)?["name"], "pkg")?
  let lines_path = fp"${root}/lines.jsonl"
  json.write_lines(lines_path, [{a: 1}, {a: 2}])?
  test.eq(lines_path.read_text()?.count_lines(), 2)?
  test.error_kind(json.decode("{"), "json")?
  test.error_kind(json.get(value, ["items", "bad"]), "json-path")?
}

proc test_json_decode_type_patterns_and_public_boundaries() [error] {
  let decoded = json.decode("{\"quote\":\"\\\"\",\"line\":\"a\\nb\",\"snow\":\"\\u2603\",\"music\":\"\\uD834\\uDD1E\"}")?
  test.eq(decoded.quote, "\"")?

  test.eq(
    decoded.line,
    """a
b""",
  )?

  test.eq(decoded.snow, "\u{2603}")?
  test.eq(decoded.music, "\u{1d11e}")?
  test.eq(json.decode("1.25")?.require(Float)?.format(precision: 2), "1.25")?
  test.error_kind(json.decode("9223372036854775808"), "json")?
  test.eq(json_label(json.decode("1")?)?, "int 1.0")?
  test.eq(json_label(json.decode("1.25")?)?, "float 1.25")?
  test.eq(json_label(json.decode("\"x\"")?)?, "str x")?
  test.eq(json_label(json.decode("null")?)?, "null")?
  test.eq(json_label(json.decode("[1,2]")?)?, "int-list")?
  test.eq(json_label(json.decode("[1,\"x\"]")?)?, "other")?

  let rows = """
{"b":2}

{"a":1}
""" |> json.lines

  let encoded = json.encode({z: 1, a: 2, nested: {b: 1, a: 2}})?
  test.eq(rows[0].b, 2)?
  test.eq(rows[1].a, 1)?
  test.eq(encoded, "{\"a\":2,\"nested\":{\"a\":2,\"b\":1},\"z\":1}")?
  test.error_kind(json.decode("not json"), "json")?
  let data = {path: p"src"}
  let path_value = data["path"]
  test.error_kind(json.encode(path_value), "json-compatible")?
}

pure json_label(value: Any) -> Result[Str] {
  match value {
    i is Int => return Ok(f"int ${i.float().format(precision: 1)}")
    f is Float => return Ok(f"float ${f.format(precision: 2)}")
    s is Str => return Ok(f"str ${s}")
    _ is Null => return Ok("null")
    _ is List[Int] => return Ok("int-list")
    _ => return Ok("other")
  }
}

proc test_json_path_helpers_report_invalid_paths() [error] {
  let data = {items: [1]}
  test.error_kind(json.get(data, ["items", 4]), "json-path")?
  test.error_kind(json.set(data, ["items", 2], 3), "json-path")?
  test.error_kind(json.remove(data, ["missing"]), "json-path")?
  test.error_kind(json.get(data, [-1]), "json-path")?
}

proc test_json_rejection_is_trace_visible(ctx: TestContext) [error] {
  let output = test.run_xsht_trace(
    ctx,
    """
let data = {path: Path("src")}
let value = data["path"]
let _encoded = json.encode(value) ?
""",
    ["--trace", "--raw"],
  )?

  test.eq(output.status, 3)?
  test.contains(output.stderr, "kind=result.propagate")?
  test.contains(output.stderr, "json-compatible")?
  test.contains(output.stderr, "Path is not JSON-compatible")?
  test.contains(output.stderr, "traceback")?
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
        Ok(inner) => return Ok(inner.message)
        Err(failure) => return Ok(failure.message)
        _ => return Ok("no rejection")
      }
    }
    Err(failure) => return Ok(failure.message)
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

proc test_json_path_get_walks_records_and_lists() [error] {
  let value = json.decode(
    "{\"name\":\"pkg\",\"items\":[1,2],\"meta\":{\"ok\":true},\"nil\":null,\"deep\":{\"rows\":[{\"cell\":7}]}}",
  )?

  # An empty path reads the value itself: nothing is traversed, nothing is
  # copied.
  test.eq(json.get(value, [])?, value)?

  # A key step reads a field, an index step reads an element.
  test.eq(json.get(value, ["name"])?, "pkg")?
  test.eq(json.get(value, ["meta", "ok"])?, true)?
  test.eq(json.get(value, ["items", 0])?, 1)?
  test.eq(json.get(value, ["items", 1])?, 2)?

  # A mixed path descends through a record, a list, and a record.
  test.eq(json.get(value, ["deep", "rows", 0, "cell"])?, 7)?

  # A missing key is reported by the step that needed it.
  test.eq(rejection_message(json.get(value, ["absent"]))?, "missing object key `absent`")?
  test.eq(rejection_message(json.get(value, ["absent", "deeper"]))?, "missing object key `absent`")?

  # A `null` member is a value, not an absence.
  test.eq(json.get(value, ["nil"])?, null)?
  test.eq(json.get(value, ["nil"], "fallback"), null)?

  # An out-of-range index is rejected; the last index is not.
  test.eq(rejection_message(json.get(value, ["items", 2]))?, "list index 2 out of bounds")?

  # The step decides which shape is required of the value it lands on.
  test.eq(rejection_message(json.get(value, ["name", "x"]))?, "expected object at key `x`, found Str")?
  test.eq(rejection_message(json.get(value, ["name", 0]))?, "expected list at index 0, found Str")?
  test.eq(rejection_message(json.get(value, ["items", "x"]))?, "expected object at key `x`, found List")?
}

proc test_json_path_get_keeps_maps_and_passes_values_through() [error] {
  let empty: Map[Any] = {}
  let tree: Any = empty.set("inner", empty.set("leaf", 0)).set("nil", null)
  test.eq(json.get(tree, ["inner", "leaf"])?, 0)?
  test.eq(json.get(tree, ["nil"])?, null)?
  test.eq(rejection_message(json.get(tree, ["inner", "absent"]))?, "missing object key `absent`")?
  expect_map("get over a Map keeps it a Map", json.get(tree, [])?)?

  # Values that are not JSON at all are ordinary members: the walk returns them
  # and never inspects or converts them.
  let holder: Any = {where: p"src", raw: b"raw", count: 1}
  test.eq(json.get(holder, ["where"])?, p"src")?
  test.eq(json.get(holder, ["raw"])?, b"raw")?
  test.eq(json.get(holder, [])?, holder)?

  # A path may cross a record, a list, and a map in one walk, and reading,
  # updating, and removing each keep the container they visited.
  let branch: Any = empty.set("cells", [1, 2])
  let mixed: Any = {rows: [branch]}
  test.eq(json.get(mixed, ["rows", 0, "cells", 1])?, 2)?
  test.eq(json.get(json.set(mixed, ["rows", 0, "cells", 1], 9)?, ["rows", 0, "cells", 1])?, 9)?
  test.eq(json.get(json.remove(mixed, ["rows", 0, "cells", 0])?, ["rows", 0, "cells", 0])?, 2)?
  expect_map("a mixed path keeps the Map", json.get(mixed, ["rows", 0])?)?
}

proc test_json_path_is_interpreted_before_traversal() [error] {
  let value = json.decode("{\"name\":\"pkg\",\"items\":[1,2]}")?

  # Indexes are positions, so a negative one is rejected outright.
  test.eq(rejection_message(json.get(value, [-1]))?, "path list indexes must be non-negative")?
  test.eq(rejection_message(json.get(value, ["items", -1]))?, "path list indexes must be non-negative")?

  # Only keys and indexes are segments.
  test.eq(rejection_message(json.get(value, [1.5]))?, "path segments must be Str or Int, found Float")?
  test.eq(rejection_message(json.get(value, [null]))?, "path segments must be Str or Int, found Null")?

  # The whole path is interpreted before it is walked: `name` is a `Str`, so
  # traversal would fail at the second step, yet the invalid segment is what all
  # three entries report.
  test.eq(json.get(value, ["name", "x"], "fallback"), "fallback")?
  test.eq(rejection_message(json.get(value, ["name", 1.5]))?, "path segments must be Str or Int, found Float")?
  test.eq(rejection_message(json.set(value, ["name", 1.5], 1))?, "path segments must be Str or Int, found Float")?
  test.eq(rejection_message(json.remove(value, ["name", 1.5]))?, "path segments must be Str or Int, found Float")?
  test.eq(rejection_message(json.set(value, [-1], 1))?, "path list indexes must be non-negative")?
  test.eq(rejection_message(json.remove(value, [null]))?, "path segments must be Str or Int, found Null")?

  # The rejections carry the `json-path` kind as well as the message.
  test.error_kind(json.get(value, [-1]), "json-path")?
  test.error_kind(json.set(value, [1.5], 1), "json-path")?
  test.error_kind(json.remove(value, [null]), "json-path")?
}

proc test_json_set_updates_the_named_position() [error] {
  let value = json.decode("{\"name\":\"pkg\",\"items\":[1,2],\"meta\":{\"ok\":true},\"nil\":null}")?

  # An empty path replaces the value itself.
  test.eq(json.set(value, [], "whole")?, "whole")?

  # A leaf key is replaced in place, and a missing leaf key is added.
  test.eq(json.get(json.set(value, ["nil"], 5)?, ["nil"])?, 5)?
  test.eq(json.get(json.set(value, ["added"], 1)?, ["added"])?, 1)?
  test.eq(json.get(json.set(value, ["meta", "ok"], false)?, ["meta", "ok"])?, false)?

  # A list index is replaced in place; a list is neither grown nor shrunk.
  test.eq(json.get(json.set(value, ["items", 0], 9)?, ["items", 0])?, 9)?
  test.eq(json.get(json.set(value, ["items", 0], 9)?, ["items", 1])?, 2)?
  test.eq(rejection_message(json.set(value, ["items", 2], 9))?, "list index 2 out of bounds")?

  # A missing intermediate key cannot be created, whatever the next step is.
  test.eq(rejection_message(json.set(value, ["absent", "deep"], 1))?, "missing intermediate object key `absent`")?
  test.eq(rejection_message(json.set(value, ["absent", 0], 1))?, "missing intermediate object key `absent`")?

  # The step decides which shape is required of the value it lands on.
  test.eq(rejection_message(json.set(value, ["items", "x"], 1))?, "expected object at key `x`, found List")?
  test.eq(rejection_message(json.set(value, ["name", 0], 1))?, "expected list at index 0, found Str")?

  # The result is the same kind of container, rebuilt once per changed field.
  expect_record("set keeps a Record", json.set(value, ["added"], 1)?)?
  test.eq(json.get(json.set(value, ["added"], 1)?, ["name"])?, "pkg")?
  expect_record("set keeps a nested Record", json.get(json.set(value, ["meta", "ok"], false)?, ["meta"])?)?
  let empty: Map[Any] = {}
  let tree: Any = empty.set("inner", empty.set("leaf", 0)).set("other", 1)
  expect_map("set keeps a Map", json.set(tree, ["other"], 2)?)?
  expect_map("set adds to a Map", json.set(tree, ["fresh"], 3)?)?
  expect_map("set keeps a nested Map", json.set(tree, ["inner", "leaf"], 5)?.get("inner", null))?
  test.eq(json.get(json.set(tree, ["inner", "leaf"], 5)?, ["inner", "leaf"])?, 5)?
  test.eq(json.get(json.set(tree, ["inner", "leaf"], 5)?, ["other"])?, 1)?

  # The value and the replacement must both be JSON-encodable, checked before
  # anything is updated.
  let path_value: Any = p"src"
  test.error_kind(json.set(value, ["added"], path_value), "json-compatible")?
  test.error_kind(json.set(path_value, ["added"], 1), "json-compatible")?
  let holder: Any = {where: p"src"}
  test.error_kind(json.set(holder, ["where"], 1), "json-compatible")?
  test.error_kind(json.set(value, [-1], path_value), "json-compatible")?
}

proc test_json_remove_drops_the_named_position() [error] {
  let value = json.decode("{\"name\":\"pkg\",\"items\":[1,2],\"meta\":{\"ok\":true},\"nil\":null}")?

  # An empty path removes nothing and answers `null`.
  test.eq(json.remove(value, [])?, null)?

  # A leaf key is dropped, at any depth.
  test.eq(json.get(json.remove(value, ["name"])?, ["name"], "gone"), "gone")?
  test.eq(json.get(json.remove(value, ["nil"])?, ["nil"], "gone"), "gone")?
  test.eq(json.get(json.remove(value, ["meta", "ok"])?, ["meta", "ok"], "gone"), "gone")?
  test.eq(json.get(json.remove(value, ["meta", "ok"])?, ["name"])?, "pkg")?

  # A list element is dropped and the list shifts left.
  test.eq(json.get(json.remove(value, ["items", 0])?, ["items", 0])?, 2)?
  test.eq(rejection_message(json.remove(value, ["items", 2]))?, "list index 2 out of bounds")?

  # A missing leaf key and a missing intermediate key are distinct, and removal
  # never creates one.
  test.eq(rejection_message(json.remove(value, ["absent"]))?, "missing object key `absent`")?
  test.eq(rejection_message(json.remove(value, ["absent", "deep"]))?, "missing intermediate object key `absent`")?
  test.eq(rejection_message(json.remove(value, ["meta", "absent"]))?, "missing object key `absent`")?
  test.eq(rejection_message(json.remove(value, ["absent", 0]))?, "missing intermediate object key `absent`")?

  # The step decides which shape is required of the value it lands on.
  test.eq(rejection_message(json.remove(value, ["name", 0]))?, "expected list at index 0, found Str")?
  test.eq(rejection_message(json.remove(value, ["items", "x"]))?, "expected object at key `x`, found List")?

  # The result is the same kind of container.
  expect_record("remove keeps a Record", json.remove(value, ["name"])?)?
  let empty: Map[Any] = {}
  let tree: Any = empty.set("inner", empty.set("leaf", 0)).set("other", 1)
  expect_map("remove keeps a Map", json.remove(tree, ["other"])?)?

  # Bound as `Any`: the inferred type of a `json.remove` result cannot take a
  # dynamic method call, so the binding names the type the walk already uses.
  let pruned: Any = json.remove(tree, ["inner", "leaf"])?
  expect_map("remove keeps a nested Map", pruned.get("inner", null))?
  test.eq(pruned.get("inner", null).require(Map[Any])?.has("leaf"), false)?
  test.eq(json.get(pruned, ["inner", "leaf"], "gone"), "gone")?

  # Members the JSON codec would reject are ordinary values: they survive a
  # removal that does not name them.
  let holder: Any = {where: p"src", raw: b"raw", count: 1}
  let kept = json.remove(holder, ["count"])?
  test.eq(json.get(kept, ["where"])?, p"src")?
  test.eq(json.get(kept, ["raw"])?, b"raw")?
}

proc test_json_get_overloads_and_encoded_lines() [error] {
  let value = json.decode("{\"name\":\"pkg\",\"items\":[1,2],\"nil\":null}")?

  # The two-argument overload answers with the read or with the rejection.
  test.eq(json.get(value, ["name"])?, "pkg")?
  test.eq(rejection_message(json.get(value, ["absent"]))?, "missing object key `absent`")?
  test.eq(rejection_message(json.get(value, ["items", 2]))?, "list index 2 out of bounds")?

  # The three-argument overload answers with the fallback whenever the path is
  # rejected, whatever the step was.
  test.eq(json.get(value, ["absent"], "fallback"), "fallback")?
  test.eq(json.get(value, ["absent", "deep"], "fallback"), "fallback")?
  test.eq(json.get(value, ["items", 2], "fallback"), "fallback")?
  test.eq(json.get(value, ["items", "x"], "fallback"), "fallback")?
  test.eq(json.get(value, [-1], "fallback"), "fallback")?
  test.eq(json.get(value, [1.5], "fallback"), "fallback")?
  test.eq(json.get(value, [], "fallback"), value)?
  test.eq(json.get(value, ["name"], "fallback"), "pkg")?
  test.eq(json.get(value, ["nil"], "fallback"), null)?

  # Every item is encoded compactly and each is followed by a newline.
  test.eq(json.encode_lines([])?, "")?
  test.eq(
    json.encode_lines([1])?,
    """1
""",
  )?
  test.eq(
    json.encode_lines([1, "two", null, true])?,
    """1
"two"
null
true
""",
  )?
  test.eq(
    json.encode_lines(["a\"b"])?,
    """"a\\"b"
""",
  )?
  test.eq(
    json.encode_lines(
  [
  """a
b""",
],
)?,
    """"a\\nb"
""",
  )?
  test.eq(
    json.encode_lines([value, value])?,
    """{"items":[1,2],"name":"pkg","nil":null}
{"items":[1,2],"name":"pkg","nil":null}
""",
  )?

  # An item that cannot be encoded fails the whole composition, whichever item
  # it is; the message and the empty output are checked in the trace test.
  let path_value: Any = p"src"
  test.error_kind(json.encode_lines([path_value]), "json-compatible")?
  test.error_kind(json.encode_lines([1, path_value]), "json-compatible")?
  test.error_kind(json.encode_lines([path_value, 1]), "json-compatible")?
}

proc test_json_path_rejects_a_non_list_path(ctx: TestContext) [error] {
  # `path` is declared `List[Any]`, so a path of any other type fails the call
  # itself and aborts before the path policy runs: `path expected List` is only
  # reachable from inside this module, where the parameter is `Any`.
  let read = test.run_xsht_trace(
    ctx,
    """
let value: Any = {a: 1}
let where: Any = "a"
let _read = json.get(value, where) ?
""",
    ["--trace", "--raw"],
  )?
  test.eq(read.status, 3)?
  test.contains(read.stderr, "type-error")?
  test.contains(read.stderr, "List[Any]")?

  # The fallback overload converts a rejected path, not a failed call: a
  # non-`json-path` failure is propagated instead of answering the fallback.
  let with_fallback = test.run_xsht_trace(
    ctx,
    """
let value: Any = {a: 1}
let where: Any = "a"
let _read = json.get(value, where, "fallback") ?
""",
    ["--trace", "--raw"],
  )?
  test.eq(with_fallback.status, 3)?
  test.contains(with_fallback.stderr, "type-error")?
  test.contains(with_fallback.stderr, "List[Any]")?

  # An item that cannot be encoded fails the whole composition, and the failure
  # is reported before any output exists.
  let lines = test.run_xsht_trace(
    ctx,
    """
let bad: Any = Path("src")
let items: Any = [1, bad]
let _lines = json.encode_lines(items) ?
""",
    ["--trace", "--raw"],
  )?
  test.eq(lines.status, 3)?
  test.eq(lines.stdout, "")?
  test.contains(lines.stderr, "json-compatible")?
  test.contains(lines.stderr, "Path is not JSON-compatible")?
}
