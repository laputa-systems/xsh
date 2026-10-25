enum ExactnessState: Str { Ready = "ready" }
type ExactnessLeaf = {name: Str, state: ExactnessState}
type ExactnessTree = {child: ExactnessLeaf, rows: List[ExactnessLeaf?], opaque: Any}
type ExactnessDefault = {name: Str, count: UInt = 4}
type ExactnessNullable = {name: Str, missing: Str?}
nominal type ExactnessNamed = {name: Str}
type ExactnessShape = {name: Str}
type ExactnessFirst = {state: ExactnessState, wanted: Int}
type ExactnessSecond = {state: Str, other: Int}

test record_unknown_fields_preserve_reject_strip {
  let raw = json.decode(r"""{"name":"ok","state":"ready","future":{"v":1}}""")?
  let preserved = raw.require(ExactnessLeaf)?
  assert json.encode(preserved)? == r"""{"future":{"v":1},"name":"ok","state":"ready"}"""
  test.error_kind(raw.require_exact(ExactnessLeaf), "schema")
  match raw.require_exact(ExactnessLeaf) {
    Err(error) => {
      assert "future: unexpected field future" in error.message, error.message
    }
    Ok(_) => assert false, "reject unknown fields"
  }
  let stripped: ExactnessLeaf = raw.require_stripped()?
  assert stripped.state == Ready
  assert json.encode(stripped)? == r"""{"name":"ok","state":"ready"}"""
  assert json.encode(raw)? == r"""{"future":{"v":1},"name":"ok","state":"ready"}"""
  let exact: ExactnessLeaf = stripped.require_exact()?
  assert exact.name == "ok"
}

test record_unknown_fields_recurse_and_keep_opaque_slots {
  let raw = json.decode(r"""{"child":{"name":"a","state":"ready","future":1},"rows":[{"name":"b","state":"ready","future":2},null],"opaque":{"anything":3},"root_extra":4}""")?
  let stripped = raw.require_stripped(ExactnessTree)?
  assert json.encode(stripped)? == r"""{"child":{"name":"a","state":"ready"},"opaque":{"anything":3},"rows":[{"name":"b","state":"ready"},null]}"""
  assert stripped.rows[0]?.state == Ready
  let nested: Any = {child: {name: "a", state: "ready", future: 1}, rows: [], opaque: null}
  match nested.require_exact(ExactnessTree) {
    Err(error) => assert "child.future: unexpected field future" in error.message, error.message
    Ok(_) => assert false, "reject nested unknown field"
  }
  let erased = raw.require_stripped(Record)?
  assert json.encode(erased)? == json.encode(raw)?
  let opaque = raw.require_exact(Any)?
  assert json.encode(opaque)? == json.encode(raw)?
}

test record_unknown_fields_never_fill_defaults {
  let missing: Any = {name: "ok"}
  assert missing.require(ExactnessDefault) is Err(_)
  assert missing.require_exact(ExactnessDefault) is Err(_)
  assert missing.require_stripped(ExactnessDefault) is Err(_)
  assert missing.require_stripped(ExactnessNullable) is Err(_)
  let bad: Any = {name: "ok", count: -1, future: 2}
  assert bad.require_stripped(ExactnessDefault) is Err(_)
  let supplied: Any = {name: "ok", count: 9, future: 2}
  let checked = supplied.require_stripped(ExactnessDefault)?
  assert checked.count == 9
  assert ExactnessDefault(name: "ok").count == 4
}

test record_unknown_fields_keep_nominal_and_width_contracts {
  let wide = {name: "ok", future: 1}
  let compatible: ExactnessNamed = wide.require_stripped()?
  assert compatible.name == "ok"
  assert json.encode(compatible)? == r"""{"name":"ok"}"""
  let structural: ExactnessShape = wide
  assert structural.name == "ok"
  let dynamic: Any = wide
  assert dynamic is ExactnessShape
  assert wide.require_exact(ExactnessNamed) is Err(_)
}

test record_unknown_fields_union_failure_is_atomic {
  let raw: Any = {state: "ready", other: 2, extra: 3}
  let selected = raw.require_stripped(Union[ExactnessFirst, ExactnessSecond])?
  assert json.encode(selected)? == r"""{"other":2,"state":"ready"}"""
  assert json.encode(raw)? == r"""{"extra":3,"other":2,"state":"ready"}"""
  assert raw.require_exact(Union[ExactnessFirst, ExactnessSecond]) is Err(_)
  let rows: Any = [{name: "ok", state: "ready", extra: 1}]
  let nonempty = rows.require_stripped(NonEmpty[ExactnessLeaf])?
  assert nonempty.first().state == Ready
  assert json.encode(nonempty)? == r"""[{"name":"ok","state":"ready"}]"""
  let empty: Any = []
  assert empty.require_stripped(NonEmpty[ExactnessLeaf]) is Err(_)
}

test record_unknown_fields_map_values_and_optional_targets {
  let typed: Map[Str, Any] = {["first"]: {name: "ok", extra: 1}}
  let typed_preserved = typed.require(Map[Str, ExactnessShape])?
  assert json.encode(typed_preserved)? == r"""{"first":{"extra":1,"name":"ok"}}"""
  let typed_stripped = typed.require_stripped(Map[Str, ExactnessShape])?
  assert json.encode(typed_stripped)? == r"""{"first":{"name":"ok"}}"""
  match typed.require_exact(Map[Str, ExactnessShape]) {
    Err(error) => assert "[\"first\"].extra" in error.message, error.message
    Ok(_) => assert false, "reject unknown fields in typed map values"
  }
  let raw = json.decode(r"""{"first":{"name":"ok","state":"ready","extra":1}}""")?
  let values = raw.require_stripped(Map[Str, ExactnessLeaf])?
  assert values.get("first")?.state == Ready
  assert json.encode(values)? == r"""{"first":{"name":"ok","state":"ready"}}"""
  match raw.require_exact(Map[Str, ExactnessLeaf]) {
    Err(error) => assert "[\"first\"].extra" in error.message, error.message
    Ok(_) => assert false, "reject unknown fields in map values"
  }
  let optional: Any = null
  let checked: ExactnessLeaf? = optional.require_exact()?
  assert checked == null
}

test record_unknown_fields_linter_retains_explicit_policies { |ctx|
  for method in ["require_exact", "require_stripped"] {
    let source = f"type Entry = {{name: Str}}\npure checked(row: Entry) -> Result[Entry] {{ row.{method}(Entry) }}\n"
    let file = test.temp_file(ctx, name: f"{method}.xsh", contents: bytes.from_text(source))?
    let checked = run.capture --text "xsht" check $file
    assert checked.status.exited_with(0), checked.stderr
    let linted = run.capture --text "xsht" lint --only lint.redundant-require $file
    assert linted.status.exited_with(0), linted.stderr
    assert "lint.redundant-require" not in linted.stderr, linted.stderr
  }
}
