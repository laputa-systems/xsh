# What `xsht check` reported for one program.
type Checked = {status: Status, stderr: Str}

# Runs only the checker over `source`: these programs write files, or would
# fail at run time.
proc check(ctx: TestContext, source: Str) [fs, process, error] -> Result[Checked] {
  let file = test.temp_file(ctx, name: "checked.xsh", contents: bytes.from_text(source))?
  let checked = run.capture --text "xsht" check $file
  {status: checked.status, stderr: checked.stderr}
}

# Requires the checker to reject `source` with every diagnostic in `codes`.
proc expect_rejected(ctx: TestContext, source: Str, codes: List[Str]) [fs, process, error] {
  let checked = check(ctx, source)?
  assert checked.status.exited_with(2), f"{source}: {checked.stderr}"
  for code in codes {
    assert f"[{code}]" in checked.stderr, f"expected {code} for {source}: {checked.stderr}"
  }
}

# Requires the checker to report none of `codes` for `source`, which other
# diagnostics may still reject.
proc expect_free_of(ctx: TestContext, source: Str, codes: List[Str]) [fs, process, error] {
  let checked = check(ctx, source)?
  for code in codes {
    assert f"[{code}]" not in checked.stderr, f"unexpected {code} for {source}: {checked.stderr}"
  }
}

test test_checker_handles_collection_modules { |ctx|
  expect_free_of(
    ctx,
    r"""
let numbers = [1].push(2)
let more = numbers.extend([3])
let flat = numbers.extend(more)
let contains = 3 in flat
let fallback: Int = (flat.get(9) ?? 4)
let get_or_fallback: Int = (flat.get(10) ?? 5)
let first: Int = (flat.get(0) ?? 0)
var argv: List[Any] = ["cc"]
argv = argv.extend([p"main.c", "-o", p"main.o"])
let argv_command = process.command_argv(p"cc", argv)
let m0: Map[Int] = {}
let m1 = m0.set("one", 1)
let value = m1.get("one")?
let map_fallback = (m1.get("missing") ?? 2)
let map_get_or_fallback = (m1.get("missing") ?? 3)
let keys = m1.keys()
let values = m1.values()
let by_name = {row.name: row.version for row in [{name: "pkg", version: "1"}]}
let version: Str = by_name.get("pkg")?
let groups0: Map[List[Str]] = {}
let groups1 = groups0.push("pkg", "one")
let groups2 = groups1.push("pkg", "two")
let grouped: List[Str] = groups2.get("pkg")?
let row = {name: "pkg", version: "1"}
let has_name = "name" in row
let field: Str = row.get("name")?
let fields = row.keys()
type RecordName = {name: Str}
let checked = row.require(RecordName)?
if "version" in checked { let _ = checked.get("version")?.require(Str)? }
""",
    ["check.type-mismatch", "check.unknown-module-api"],
  )

  for source in [
    "let row = {name: \"pkg\"}\nlet _ = record.has(row, \"name\")\n",
    "let row = {name: \"pkg\"}\nlet _ = record.get(row, \"name\")?\n",
    "let row = {name: \"pkg\"}\nlet _ = record.keys(row)\n",
  ] {
    expect_rejected(ctx, source, ["check.unresolved-name"])
  }

  for source in [
    "let xs = [1].push(\"two\")\n",
    "\nlet m0: Map[Int] = map.empty()\nlet m1 = m0.set(\"one\", \"bad\")\n",
    "\nlet m0: Map[Int] = map.empty()\nlet m1 = m0.push(\"one\", 1)\n",
  ] {
    expect_rejected(ctx, source, ["check.type-mismatch"])
  }
}

test test_checker_handles_while_match_aliases_schemas_and_rest_params { |ctx|
  expect_free_of(
    ctx,
    r"""
type PackageName = Str
type Package = { name: PackageName, root: Path, files: List[Path] }

proc describe(pkg: Package, prefix: Str = "pkg", ...labels: List[Str]) -> Result[Unit] {
  var tries = 0
  while tries < 3 {
    tries = tries + 1
    if tries == 2 {
      continue
    }
    break
  }

  match Ok(pkg.name) {
    Ok(name) if name == "demo" => print ${prefix} ${name},
    Err(e) => return Err(e),
    _ => print "other"
  }

  for label in labels {
    print ${label}
  }
}

let pkg: Package = { name: "demo", root: Path("src"), files: [Path("src/lib.rs")] }
describe (pkg) ?
describe (pkg) named extra ?
""",
    ["check.type-mismatch", "check.arity", "check.loop-control", "check.schema-field"],
  )
}

test test_checker_rejects_loop_pattern_schema_and_parameter_errors { |ctx|
  for case in [
    {
      source: "break\n",
      code: "check.loop-control",
    },
    {
      source: "[1] |> each { break }\n",
      code: "check.loop-control",
    },
    {
      source: r"""let value = 1
match value { Ok(x) => print ${x} }
""",
      code: "check.pattern-type",
    },
    {
      source: "type Package = { name: Str, root: Path }\nlet pkg: Package = { name: \"demo\" }\n",
      code: "check.schema-field",
    },
    {
      source: "type Package = { name: Str }\nlet pkg: Package = { name: \"demo\", extra: \"x\" }\n",
      code: "check.schema-field",
    },
    {
      source: "proc bad(...items: Str) -> Result[Unit] { return Ok() }\n",
      code: "check.rest-type",
    },
    {
      source: "proc bad(a: Str, b: Str = a) -> Result[Unit] { return Ok() }\n",
      code: "check.unresolved-name",
    },
  ] {
    expect_rejected(ctx, case.source, [case.code])
  }
}

test test_checker_reports_opaque_status_record_literals { |ctx|
  let checked = check(ctx, "var status: Status = {ok: false, code: 0}\n")?
  assert checked.status.exited_with(2), checked.stderr
  assert "`Status` is a runtime-only type" in checked.stderr, checked.stderr
}

test test_checker_handles_compact_sugar_forms { |ctx|
  expect_free_of(
    ctx,
    r"""
proc write_note(path: Path) {
  path.write("ok")?
}
var total = 1
total += 2
let files: List[Path] = g"src/*.rs"
let label: Str = if total > 1 { "many" } else { "one" }
let value: Int = match Ok(total) { Ok(count) => count, Err(_) => 0 }
let cargo_exists = p"Cargo.toml".exists()?
""",
    [
      "check.required-return",
      "check.type-mismatch",
      "check.operator-type",
      "check.unknown-method",
      "check.pure-effect",
    ],
  )
}

test test_checker_accepts_local_field_and_map_entry_assignment { |ctx|
  expect_free_of(
    ctx,
    r"""
type Stats = {code: Int, comments: Int}

pure bump() -> Stats {
  var stats: Stats = {code: 0, comments: 0}
  stats.code += 1
  return stats
}

var counts: Map[Int] = map.empty()
counts["code"] = 2
""",
    [
      "check.undefined-name",
      "check.assign-let",
      "check.pure-assignment",
      "check.type-mismatch",
      "check.operator-type",
      "check.assign-target",
    ],
  )
}

test test_checker_rejects_field_assignment_to_let_binding { |ctx|
  expect_rejected(ctx, "\nlet stats = {code: 0}\nstats.code += 1\n", ["check.assign-let"])
}

# The mismatch is reported at the scalar right-hand side, `2`.
test test_checker_list_compound_assignment_points_at_scalar_rhs { |ctx|
  let checked = check(ctx, "var items = [1]\nitems += 2\n")?
  assert "err[check.type-mismatch]" in checked.stderr, checked.stderr
  assert ":2:10\n" in checked.stderr, checked.stderr
  assert "\n           ^ " in checked.stderr, checked.stderr
}

test test_checker_handles_ergonomic_sugar_pass_forms { |ctx|
  expect_free_of(
    ctx,
    r"""
let pkg = {name: "demo", version: "1.0", path: Path("src")}
let {name, version, ..} = pkg
var {path: package_path, ..} = pkg
package_path = Path("dist")
for {name, ..} in [pkg] {
  print $name
}
let jobs = env.Str.JOBS ?? "1"
fs.mkdir build ?
fs.remove build --missing-ok ?
json.write manifest ({name, version}) ?
""",
    [
      "check.type-mismatch",
      "check.destructure-type",
      "check.destructure-field",
      "check.module-command-value",
      "check.module-command-flag",
      "check.arity",
      "check.try-result",
      "check.standard-module-shadow",
    ],
  )
}

test test_checker_rejects_ergonomic_sugar_pass_errors { |ctx|
  for case in [
    {source: "fs.nope out ?\n", code: "check.unknown-module-api"},
    {source: "fs.read out ?\n", code: "check.unknown-module-api"},
    {source: "fs.remove out --unknown ?\n", code: "check.module-command-flag"},
    {source: "let {name, name} = {name: \"demo\"}\n", code: "check.destructure-field"},
    {source: "let {name} = 1\n", code: "check.destructure-type"},
    {source: "let pkg = {name: \"demo\"}\nlet {version} = pkg\n", code: "check.destructure-field"},
    {source: "export let {name} = {name: \"demo\"}\n", code: "check.export-destructure"},
    {source: "let jobs = env.Str.JOBS or \"1\"\n", code: "check.result-fallback"},
  ] {
    expect_rejected(ctx, case.source, [case.code])
  }
}

test test_checker_infers_args_parse_literal_schema_records { |ctx|
  expect_free_of(
    ctx,
    r"""
let parsed = cli.parse([], {
  count: {kind: "Int", required: true},
  name: "Str",
  roots: {kind: "Path", repeated: true},
  verbose: "Bool",
})?
let count: Int = parsed.count
let name: Str? = parsed.name
let roots: List[Path] = parsed.roots
let verbose: Bool = parsed.verbose
""",
    ["check.type-mismatch", "check.unknown-field"],
  )
  expect_rejected(
    ctx,
    "\nlet parsed = cli.parse([], {name: \"Str\"})?\nlet name: Int = parsed.name\n",
    ["check.type-mismatch"],
  )
}

# The descriptor error names the unsupported type and points at the entry of
# the constant that declares it, `{kind: "Nope"}`.
test test_checker_cli_constant_descriptor_errors_keep_the_declaration_origin { |ctx|
  let checked = check(ctx, "const schema = {count: {kind: \"Nope\"}}\nlet _ = cli.parse([], schema)\n")?
  assert "err[check.cli-descriptor]" in checked.stderr, checked.stderr
  assert "unsupported option type `Nope`" in checked.stderr, checked.stderr
  assert ":1:24\n" in checked.stderr, checked.stderr
  assert "\n                         ^^^^^^^^^^^^^^ " in checked.stderr, checked.stderr
}

test test_checker_treats_old_schema_helper_name_as_unresolved_call { |ctx|
  expect_rejected(
    ctx,
    "type Row = {name: Str}\nlet raw = {name: \"demo\"}\nlet row = validate(raw, Row)?\n",
    ["check.unresolved-call"],
  )
}

test test_checker_reports_missing_known_record_fields_and_removed_record_contracts { |ctx|
  expect_rejected(
    ctx,
    r"""
type Row = {name: Str}
let row: Row = {name: "demo"}
let version = row.version
let checked = record.require({name: "demo"}, {name: "Strng", bad: "Result[Str, ]"})
let loaded = record.require({}, {build: "Proc(Path -> Result[Unit]"})
""",
    ["check.unknown-field", "check.removed-record-require"],
  )
}

# An unsupported scalar key is reported at the key itself, `1.5`.
test test_checker_computed_map_literals_locate_key_errors_without_weakening_values { |ctx|
  let checked = check(ctx, "let values = {[1.5]: 2}\n")?
  assert "err[check.map-key-type]" in checked.stderr, checked.stderr
  assert ":1:16\n" in checked.stderr, checked.stderr
  assert "\n                 ^^^ " in checked.stderr, checked.stderr

  for source in [
    "let values = {[\"one\"]: 1, two: \"bad\"}\n",
    "let key: Any = \"one\"\nlet values = {[key]: 1}\n",
  ] {
    let rejected = check(ctx, source)?
    assert rejected.status.exited_with(2), f"{source}: {rejected.stderr}"
    assert "[check." in rejected.stderr, f"{source}: {rejected.stderr}"
  }
}
