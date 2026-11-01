test test_record_get_with_annotated_and_inferred_names { |ctx|
  for parameter in ["name: Str", "name"] {
    let output = test.expect(ctx, f"""proc read({parameter}, raw: Record) -> Result[Str] {{
  let arch: Str = if let Ok(value) = raw.get("arch") {{ value.require()? }} else {{ "all" }}
  f"{{name}}:{{arch}}"
}}
assert read("pkg", {{arch: "aarch64"}})? == "pkg:aarch64"
assert read("pkg", {{}})? == "pkg:all"
assert read("pkg", {{arch: 5}}) is Err(_)
""", status: 0)?
    assert output.stderr == "", output.stderr
  }
}

test test_record_get_preserves_visible_and_dynamic_fields { |ctx|
  let output = test.expect(ctx, r"""type Info = {arch: Str, count: Int}
pure visible(row: Info, marker) -> Result[Str] { let _ = marker; row.get(field: "arch")? }
pure spread(row: Info, marker) -> Result[Str] { let _ = marker; row.get(...{field: "arch"})? }
pure dynamic(row: Info, key: Str, marker) -> Result[Str] { let _ = marker; row.get(key)?.require(Str)? }
pure missing(row: Info, marker) -> Result[Str] { let _ = marker; row.get("missing")?.require(Str)? }
pure maybe(row: Info?, marker) { let _ = marker; row?.get("arch") }
pure from_result(row: Result[Info], marker) -> Result[Str] { let _ = marker; row?.get("arch")? }
pure erased(row: Record?, marker) -> Result[Any]? { let _ = marker; row?.get("arch") }
proc main() [error] {
  let row: Info = {arch: "arm", count: 4}
  assert visible(row, 1)? == "arm"
  assert spread(row, 1)? == "arm"
  assert maybe(row, 1) == Ok("arm")
  assert maybe(null, 1) == null
  assert from_result(Ok(row), 1)? == "arm"
  assert erased(row, 1) != null
  assert erased(null, 1) == null
  assert dynamic(row, "arch", 1)? == "arm"
  assert dynamic(row, "missing", 1) is Err(_)
  assert missing(row, 1) is Err(_)
  assert dynamic(row, "count", 1) is Err(_)
}

""", status: 0)?
  assert output.stderr == "", output.stderr
}

test test_record_get_rejects_unvalidated_or_wrong_projection { |ctx|
  for source in [
    r"""type Info = {arch: Str, count: Int}
pure bad(row: Info, marker) -> Result[Str] { let _ = marker; row.get("count")? }
""",
    r"""type Info = {arch: Str, count: Int}
pure bad(row: Info, key: Str, marker) -> Result[Str] { let _ = marker; row.get(key)? }
""",
    r"""pure bad(row: Record, marker) -> Result[Str] { let _ = marker; row.get("arch")? }
""",
  ] {
    let output = test.run_xsh(ctx, source)?
    assert ! output.success, source
    assert "err[check." in output.stderr, output.stderr
  }
}

test test_module_get_keeps_its_export_contract { |ctx|
  let root = test.temp_dir(ctx, name: "generic-module-get")?
  let module_path = fp"{root}/plugin.xsh"
  module_path.write_atomic("""##! Exports a visible value.
## Worker count.
export let workers: Int = 4
""")?
  let output = test.expect(ctx, r"""type Plugin = module { export let workers: Int }
pure lookup(plugin: Plugin, marker) -> Result[Int] { let _ = marker; plugin.get("workers")? }
proc main(...args: List[Str]) [fs, error] {
  let plugin = module.load(Path(args[0]))?.require(Plugin)?
  assert lookup(plugin, "marker")? == 4
}
""", status: 0, args: [module_path])?
  assert output.stderr == "", output.stderr
}
