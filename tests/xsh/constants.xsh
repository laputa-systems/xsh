const global_constant = later_constant + 1
const later_constant = 4
const protocol_path = p"relative/config"
const protocol_pattern = rx"^static$"
const protocol_bytes = b"static"
const empty_numbers: List[Int] = []

test test_constants_prepare_data_and_keep_aliases {
  const count = 2 + 3
  const values = [count, 9]
  var copy = values
  copy += [10]
  assert count == 5
  assert values == [5, 9]
  assert copy == [5, 9, 10]
}

test test_constants_reject_runtime_dependencies { |ctx|
  let ordinary = test.run_script(
    ctx,
    """let source = 1
const value = source
print value
""",
  )?
  assert ! ordinary.success
  let overflow = test.run_script(
    ctx,
    """const value = 9223372036854775807 + 1
print value
""",
  )?
  assert ! overflow.success
}

const empty_alias = empty_numbers

type ConstantConfig = {name: Str = "default", values: List[Int] = empty_numbers}

const protocol_config = ConstantConfig(name: "static")

enum ConstantEvent { Ready, Count(Int) }

const protocol_event: ConstantEvent = Count(global_constant)

test test_constants_prepare_constructors_paths_regex_and_forward_references {
  assert global_constant == 5
  assert protocol_path == "relative/config"
  assert protocol_pattern.matches("static")
  assert protocol_bytes.len() == 6
  assert protocol_config.name == "static"
  assert protocol_config.values == []
  assert empty_alias == []
  assert protocol_event == Count(5)
}

test test_constants_reject_cycles_contextless_empty_values_and_local_captures { |ctx|
  let sources = [
    """const a = b
const b = a
""",
    """const values = []
""",
    """const values: List[Any] = []
""",
    """const value = 1 / 0
""",
    """const value = args
""",
    """pure source() -> Int { 1 }
const value = source()
""",
    """const value = "x".upper()
""",
    """const value = p"config".read_text()?
""",
    """proc helper(input: Int) { const value = input }
""",
    """type Config = {count: Int}
const value = Config(count: "bad")
""",
  ]
  for source in sources {
    let executed = test.run_script(ctx, source)?
    assert ! executed.success
  }
}

test test_constants_exports_are_ordinary_readonly_module_data { |ctx|
  let root = test.temp_dir(ctx, name: "constant-module")?
  fp"{root}/config.xsh".write_atomic(r"""##! Immutable configuration.
## A prepared scalar.
export const size = 3
## A prepared collection.
export const values: List[Int] = [size, 4]
## A schema with a prepared default.
export type Config = {values: List[Int] = values}
const private_value = 9
""")
  let executed = test.run_script(
    ctx,
    r"""use config as c
const count = c.size + 1
const config = c.Config()
var values = c.values
values += [5]
print $count
print ${config.values.len()}
print ${c.values.len()}
print ${values.len()}
""",
    [],
    {XSH_MODULE_PATH: root},
  )?
  let {success: succeeded, stderr: failure_details, ..} = executed
  assert succeeded, failure_details
  assert executed.stdout == """4
2
2
3
"""
}

test test_constants_contextual_maps_share_without_mutation {
  const table: Map[Int] = {["last"]: 1, first: 2, ["last"]: 3}
  const combined: Map[Int] = {...table, first: 4}
  var changed = combined
  changed["first"] = 9
  assert table.get("last")? == 3
  assert table.get("first")? == 2
  assert combined.get("first")? == 4
  assert changed.get("first")? == 9
}

test test_constants_reject_shadowed_runtime_values { |ctx|
  let sources = [
    """const input = 1
pure helper(input: Int) -> Int { const captured = input; captured }
""",
    """const item = 1
for item in [2] { const captured = item }
""",
    """const item = 1
let {item} = {item: 2}
const captured = item
""",
    """const item = 1
match 2 { item => { const captured = item } }
""",
    """const value = 1 == "different"
""",
    """type Config = {count: Int}
const value: Config = {count: 1, extra: 2}
""",
  ]
  for source in sources {
    let executed = test.run_script(ctx, source)?
    assert ! executed.success
  }
}

test test_constants_functions_read_prepared_globals_before_runtime_registration { |ctx|
  let executed = test.run_script(
    ctx,
    r"""pure prepared() -> Int { value }
print ${prepared()}
const value = 8
proc display() { print $value }
display()
""",
  )?
  let {success: succeeded, stderr: failure_details, ..} = executed
  assert succeeded, failure_details
  assert executed.stdout == """8
8
"""
  let asserted = test.run_script(
    ctx,
    """const condition = false
proc check() [error] { condition }
check()
""",
  )?
  assert ! asserted.success
}

test test_constants_checked_operators_keep_typed_optional_and_duration_data {
  const pause = 250ms * 2 + 1s
  const intervals = 1s / 250ms
  const maybe: Str? = "ready"
  const present = maybe != null
  const equal_zero = [-0.0] == [0.0]
  assert pause == 1500ms
  assert intervals == 4
  assert present == true
  assert equal_zero
}

test test_constants_fail_before_any_runtime_statement { |ctx|
  let prepared = test.run_script(
    ctx,
    """print starting
const invalid = 1 / 0
""",
  )?
  assert ! prepared.success
  assert prepared.stdout == ""
  let runtime = test.run_script(
    ctx,
    """print starting
let invalid = 1 / 0
""",
  )?
  assert ! runtime.success
  assert runtime.stdout == """starting
"""
}

test test_constants_constructor_spreads_use_prepared_visible_fields {
  const supplied = {name: "spread"}
  const configured = ConstantConfig(...supplied)
  assert configured.name == "spread"
  assert configured.values == []
}

test test_constants_constructor_spreads_reject_runtime_and_erased_sources { |ctx|
  for source in [
    """type Config = {value: Int}
let source = {value: 1}
const config = Config(...source)
""",
    """type Config = {value: Int}
const source: Map[Int] = {value: 1}
const config = Config(...source)
""",
    """type Config = {value: Int}
const source = {value: 1}
const config = Config(value: 2, ...source)
""",
    """type Config = {value: Int}
const source = {other: 1}
const config = Config(...source)
""",
    """type Config = {value: Int}
const source: Int? = 1
const config = Config(value: source)
""",
    """type Config = {value: Int}
const source: Config? = {value: 1}
const config = Config(...source)
""",
  ] {
    let executed = test.run_script(ctx, source)?
    assert ! executed.success
  }
}

test test_constants_closed_record_projections_preserve_declared_field_types { |ctx|
  const source = {nested: {value: "ready"}}
  const selected = source.nested.value
  assert selected == "ready"
  let rejected = test.run_script(
    ctx,
    """type Item = {value: Str?}
const source: Item = {value: "ready"}
const selected: Str = source.value
""",
  )?
  assert ! rejected.success
}
