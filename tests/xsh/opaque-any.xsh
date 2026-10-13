type OpaqueAnyEnvelope = {value: Any}
type OpaqueAnyNamed = {name: Str}

pure keep_opaque_any(value: Any) -> Any { value }
pure first_opaque_any(...items: List[Any]) -> Any { items[0] }

test test_opaque_any_rejects_navigation_without_running { |ctx|
  for expression in [
    "raw.name",
    "raw?.name",
    "raw[\"name\"]",
    "raw?[\"name\"]",
    "raw[0..1]",
    "raw?[0..1]",
    "raw.name.len()",
    "raw.len()",
    "raw?.len()",
    "raw.trim(chars: \" \" )",
    "{...raw}",
    "[item for item in raw]",
    "{item: item for item in raw}",
    "raw |> collect",
    "[1] |> zip(raw)",
    "raw?",
    "try { raw? }",
    "retry [1ms] { raw? }",
    "wait raw",
  ] {
    let source = "let raw: Any = {name: \"demo\"}\nprint reached\nlet selected = " + expression + "\n"
    let output = test.expect(ctx, source, status: 2)?
    assert output.stdout == "", expression
    assert "check.dynamic-boundary" in output.stderr, expression + ": " + output.stderr
  }
  let iteration = test.expect(ctx, "let raw: Any = [1]\nfor item in raw { assert item == 1 }\n", status: 2)?
  assert "check.dynamic-boundary" in iteration.stderr, iteration.stderr
  for source in [
    "let raw: Any = {name: \"demo\"}\nlet selected: Map[Any] = {...raw}\n",
    "let raw: Any = {nested: {count: 1}}\nlet selected = {...raw, nested.count: 2}\n",
    "let raw: Result[Any] = Ok([1])\nlet selected = raw |> collect\n",
  ] {
    let output = test.expect(ctx, source, status: 2)?
    assert "check.dynamic-boundary" in output.stderr, source + ": " + output.stderr
  }
}

test test_opaque_any_rejects_navigation_after_guarded_unwrapping { |ctx|
  for declaration in [
    "let raw: Any? = {name: \"demo\"}",
    "let raw: Result[Any] = Ok({name: \"demo\"})",
  ] {
    for expression in ["raw?.name", "raw?[\"name\"]", "raw?[0..1]", "raw?.len()"] {
      let output = test.expect(ctx, declaration + "\nlet selected = " + expression + "\n", status: 2)?
      assert "check.dynamic-boundary" in output.stderr, expression + ": " + output.stderr
    }
  }
}

test test_opaque_any_rejects_concrete_api_boundaries { |ctx|
  for source in [
    "let raw: Any = p\"missing\"\nlet selected = json.read(raw)\n",
    "let raw: Any = p\"missing\"\nlet selected = p\"file\".copy(raw)\n",
    "let raw: Any = p\"missing\"\nlet selected = Path(raw)\n",
    "let raw: Any = \"printf\"\nlet selected = process.command_argv(raw, [\"printf\"])\n",
    "let raw: Any = [\"printf\"]\nlet selected = process.command_argv(\"printf\", raw)\n",
    "let raw: List[Any] = [\"printf\"]\nlet selected = process.command_argv(\"printf\", raw)\n",
    "let raw: Any = \"printf\"\nlet selected = process.command_argv(\"printf\", [raw])\n",
  ] {
    let output = test.expect(ctx, source, status: 2)?
    assert "check.dynamic-boundary" in output.stderr, source + ": " + output.stderr
  }
}

test test_opaque_any_rejects_user_argument_splices { |ctx|
  for source in [
    "pure keep(value: Any) -> Any { value }\nlet raw: Any = [1]\nlet selected = keep(@raw)\n",
    "pure first(...items: List[Any]) -> Any { items[0] }\nlet raw: Any = [1]\nlet selected = first(@raw)\n",
  ] {
    let output = test.expect(ctx, source, status: 2)?
    assert "check.dynamic-boundary" in output.stderr, source + ": " + output.stderr
  }
}

test test_opaque_any_rejects_ordering_chain_operands { |ctx|
  for expression in ["raw < 2 < 3", "0 < raw < 3", "0 < 1 < raw", "raw < raw < raw"] {
    let output = test.expect(ctx, "let raw: Any = 1\nlet selected = " + expression + "\n", status: 2)?
    assert "check.dynamic-boundary" in output.stderr, expression + ": " + output.stderr
  }
}

test test_opaque_any_keeps_storage_equality_and_keys {
  let raw: Any = {name: "demo"}
  let copied: Any = raw
  let field: OpaqueAnyEnvelope = OpaqueAnyEnvelope(raw)
  let items = [raw, copied]
  let entries: Map[Any] = {one: raw}
  assert keep_opaque_any(field.value) == raw
  assert raw == copied
  assert copied == raw
  assert raw != null
  assert raw in items
  assert raw not in [null]
  assert items[0] == raw
  assert entries["one"] == raw
  assert items.get(0)? == raw
  assert entries.get("one")? == raw
  let one = [raw]
  assert keep_opaque_any(@one) == raw
  assert first_opaque_any(@items) == raw
  for item in items { assert item == raw }
  for {key, value} in entries {
    assert key == "one"
    assert value == raw
  }
  assert (items |> group-by { |item| item } |> count()) == 1
  assert (items |> unique-by { |item| item } |> count()) == 1
  assert json.encode(raw)? == "{\"name\":\"demo\"}"
}

test test_opaque_any_keeps_validation_and_path_operations {
  let raw: Any = {name: "demo", cells: [1, 2]}
  assert raw.require(OpaqueAnyNamed)?.name == "demo"
  if raw is OpaqueAnyNamed {
    assert raw.name == "demo"
  } else {
    assert false
  }
  if let name is Str = json.get(raw, ["name"])? {
    assert name == "demo"
  } else {
    assert false
  }

  assert json.get(raw, ["name"])? == "demo"
  assert json.get(raw, ["absent"], "fallback") == "fallback"
  assert json.get(raw, ["cells", 1])? == 2
  let checked = json.get(raw, ["cells", 0])?.require(Int)?
  assert 0 < checked < 3
  let entries: Map[Any] = {name: "map"}
  let map_value: Any = entries
  assert json.get(map_value, ["name"])? == "map"
  assert json.get(map_value, ["absent"], "fallback") == "fallback"
  let list_value: Any = [1, 2]
  assert json.get(list_value, [1])? == 2
  assert json.get(list_value, [2], 3) == 3
  assert json.get(json.set(raw, ["name"], "new")?, ["name"])? == "new"
  assert json.get(json.remove(raw, ["name"])?, ["name"], null) == null
  let first: List[Any] = [1]
  assert list_value.require(List[Any])?[0..1] == first
  assert ([item for item in list_value.require(List[Any])?] |> count()) == 2
}

test test_opaque_any_keeps_erased_record_leaf_access {
  let erased: Record = {name: "demo", nested: {count: 1}}
  assert erased.name == "demo"
  assert erased["name"] == "demo"
  assert erased.get("name")? == "demo"
  assert json.get(erased.nested, ["count"])? == 1
  let names = ["printf"]
  let paths = [p"printf"]
  let _: Command = process.command_argv("printf", names)
  let _: Command = process.command_argv(p"printf", paths)
}

test test_opaque_any_keeps_opaque_callable_results { |ctx|
  test.expect(ctx, r"""pure answer() -> Int { 7 }
proc received() -> Result[Int] { 9 }
let pure_handle: Pure = answer
let proc_handle: Proc = received
let value: Any = pure_handle.call()
let result: Result[Any] = proc_handle.call()
assert value == 7
assert result? == 9
""", status: 0)?
}
