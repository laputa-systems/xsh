test test_constants_prepare_data_and_keep_aliases [error] {
  const count = 2 + 3
  const values: List[Int] = [count, 9]
  var copy = values
  copy += [10]
  test.eq(count, 5)?
  test.eq(values, [5, 9])?
  test.eq(copy, [5, 9, 10])?
}

test test_constants_reject_runtime_dependencies [error] { |ctx|
  let ordinary = test.run_script(ctx, "let source = 1\nconst value = source\nprint value\n")?
  test.eq(ordinary.success, false)?
  let overflow = test.run_script(ctx, "const value = 9223372036854775807 + 1\nprint value\n")?
  test.eq(overflow.success, false)?
}

const global_constant = later_constant + 1
const later_constant = 4
const protocol_path = p"relative/config"
const protocol_pattern = rx"^static$"
const protocol_bytes = b"static"
const empty_numbers: List[Int] = []
const empty_alias = empty_numbers
type ConstantConfig = {name: Str = "default", values: List[Int] = empty_numbers}
const protocol_config = ConstantConfig(name: "static")
enum ConstantEvent { Ready, Count(Int) }
const protocol_event = Count(global_constant)

test test_constants_prepare_constructors_paths_regex_and_forward_references [error] {
  test.eq(global_constant, 5)?
  test.eq(protocol_path.display(), "relative/config")?
  test.eq(protocol_pattern.matches("static"), true)?
  test.eq(protocol_bytes.len(), 6)?
  test.eq(protocol_config.name, "static")?
  test.eq(protocol_config.values, [])?
  test.eq(empty_alias, [])?
  test.eq(protocol_event, Count(5))?
}

test test_constants_reject_cycles_contextless_empty_values_and_local_captures [error] { |ctx|
  let sources = [
    "const a = b\nconst b = a\n",
    "const values = []\n",
    "const values: List[Any] = []\n",
    "const value = 1 / 0\n",
    "const value = args\n",
    "pure source() -> Int { 1 }\nconst value = source()\n",
    "const value = \"x\".upper()\n",
    "const value = p\"config\".read_text()?\n",
    "proc helper(input: Int) { const value = input }\n",
    "type Config = {count: Int}\nconst value = Config(count: \"bad\")\n",
  ]
  for source in sources {
    let executed = test.run_script(ctx, source)?
    test.eq(executed.success, false)?
  }
}

test test_constants_exports_are_ordinary_readonly_module_data [fs, error] { |ctx|
  let root = test.temp_dir(ctx, name: "constant-module")?
  fp"${root}/config.xsh".write_atomic(r"""##! Immutable configuration.
## A prepared scalar.
export const size = 3
## A prepared collection.
export const values: List[Int] = [size, 4]
## A schema with a prepared default.
export type Config = {values: List[Int] = values}
const private_value = 9
""")?
  let executed = test.run_script(ctx, r"""use config as c
const count = c.size + 1
const config = c.Config()
var values = c.values
values += [5]
print $count
print ${config.values.len()}
print ${c.values.len()}
print ${values.len()}
""", [], {XSH_MODULE_PATH: root.display()})?
  test.ok(executed.success, executed.stderr)?
  test.eq(executed.stdout, "4\n2\n2\n3\n")?
}

test test_constants_contextual_maps_share_without_mutation [error] {
  const table: Map[Int] = {["last"]: 1, first: 2, ["last"]: 3}
  const combined: Map[Int] = {...table, first: 4}
  var changed = combined
  changed["first"] = 9
  test.eq(table.get("last")?, 3)?
  test.eq(table.get("first")?, 2)?
  test.eq(combined.get("first")?, 4)?
  test.eq(changed.get("first")?, 9)?
}

test test_constants_reject_shadowed_runtime_values [error] { |ctx|
  let sources = [
    "const input = 1\npure helper(input: Int) -> Int { const captured = input; captured }\n",
    "const item = 1\nfor item in [2] { const captured = item }\n",
    "const item = 1\nlet {item} = {item: 2}\nconst captured = item\n",
    "const item = 1\nmatch 2 { item => { const captured = item } }\n",
    "const value = 1 == \"different\"\n",
    "type Config = {count: Int}\nconst value: Config = {count: 1, extra: 2}\n",
  ]
  for source in sources {
    let executed = test.run_script(ctx, source)?
    test.eq(executed.success, false)?
  }
}

test test_constants_functions_read_prepared_globals_before_runtime_registration [error] { |ctx|
  let executed = test.run_script(ctx, r"""pure prepared() -> Int { value }
print ${prepared()}
const value = 8
proc display() { print $value }
display()
""")?
  test.ok(executed.success, executed.stderr)?
  test.eq(executed.stdout, "8\n8\n")?
  let asserted = test.run_script(ctx, "const condition = false\nproc check() [error] { condition }\ncheck()\n")?
  test.eq(asserted.success, false)?
}

test test_constants_checked_operators_keep_typed_optional_and_duration_data [error] {
  const pause = 250ms * 2 + 1s
  const intervals = 1s / 250ms
  const maybe: Str? = "ready"
  const present = maybe != null
  const equal_zero = [-0.0] == [0.0]
  test.eq(pause, 1500ms)?
  test.eq(intervals, 4)?
  test.eq(present, true)?
  test.eq(equal_zero, true)?
}

test test_constants_fail_before_any_runtime_statement [error] { |ctx|
  let prepared = test.run_script(ctx, "print starting\nconst invalid = 1 / 0\n")?
  test.eq(prepared.success, false)?
  test.eq(prepared.stdout, "")?
  let runtime = test.run_script(ctx, "print starting\nlet invalid = 1 / 0\n")?
  test.eq(runtime.success, false)?
  test.eq(runtime.stdout, "starting\n")?
}
