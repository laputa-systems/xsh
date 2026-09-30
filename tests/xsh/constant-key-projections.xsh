type ProjectionPlugin = module {
  export let workers: Int
  export optional let description: Str?
  export optional let maybe: Str?
  export pure increment(value: Int = 1) -> Int
}

test test_constant_key_projection_module_index_and_callable [fs, error] { |ctx|
  let root = test.temp_dir(ctx, name: "constant-key-projection")?
  let module_path = fp"${root}/plugin.xsh"
  module_path.write("""
##! Provides known exports for projection checking.
## Parallel worker limit.
export let workers: Int = 4
let private_workers = 7
## Returns the next integer.
export pure increment(value: Int = 1) -> Int { value + 1 }
## A present nullable export.
export let maybe: Str? = null
""")?
  let plugin = module.load(module_path)?.require(ProjectionPlugin)?
  const field = "workers"
  let workers: Int = plugin[field]
  let increment = plugin.get("increment")?
  test.eq(workers, 4)?
  test.eq(increment.call(4), 5)?
  test.eq(increment.call(), 2)?
  test.eq(plugin.get("maybe")?, null)?
  test.error_kind(plugin.get("description"), "missing-field")?
  test.error_kind(plugin.get("private_workers"), "missing-field")?
}

type ProjectionConfig = {workers: Int, value: Str?, if: Bool}

test test_constant_key_projection_nullable_and_keyword_labels [error] {
  let config: ProjectionConfig = {workers: 4, value: null, "if": true}
  const field = "workers"
  let workers: Int = config.get(field)?
  let value: Str? = config.get("value")?
  let enabled: Bool = config["if"]
  test.eq(workers, 4)?
  test.eq(config.get(...{field: "workers"})?, 4)?
  test.eq(value, null)?
  test.ok(enabled)?
  test.error_kind(config.get("absent"), "missing-field")?
}

test test_constant_key_projection_rejects_incompatible_known_field_type [error] { |ctx|
  for source in [
    "const field = \"workers\"\ntype Config = {workers: Int}\nlet config: Config = {workers: 4}\nlet value: Str = config.get(field)?\n",
    "type Config = {workers: Int}\nlet config: Config = {workers: 4}\nlet value: Str = config[\"workers\"]\n",
  ] {
    let output = test.run_script(ctx, source)?
    test.ok(!output.success, source)?
    test.ok(output.stderr.contains("check.type-mismatch"), output.stderr)?
  }
}

test test_constant_key_projection_evaluates_receiver_once [error] { |ctx|
  let output = test.run_script(ctx, """
type Config = {workers: Int}
proc config() -> Config { print "receiver"; {workers: 4} }
const field = "workers"
print (config().get(field)?)
print (config()[field])
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "receiver\n4\nreceiver\n4\n")?
}

test test_constant_key_projection_imported_const_key [fs, error] { |ctx|
  let root = test.temp_dir(ctx, name: "imported-projection-key")?
  fp"${root}/keys.xsh".write_atomic(r"""##! Selection keys.
## Visible worker field.
export const workers = "workers"
""")?
  let output = test.run_script(ctx, r"""use keys as keys
type Config = {workers: Int}
let config: Config = {workers: 4}
print (config.get(keys.workers)?)
print (config[keys.workers])
""", [], {XSH_MODULE_PATH: root.display()})?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "4\n4\n")?
}

type ProjectionWide = {workers: Int, hidden: Bool}
type ProjectionVisible = {workers: Int}

test test_constant_key_projection_dynamic_and_hidden_fields_keep_validation [error] {
  let wide: ProjectionWide = {workers: 4, hidden: true}
  let visible: ProjectionVisible = wide
  var field = "workers"
  field = "hidden"
  test.ok(visible.get(field)?.require(Bool)?)?
  test.ok(visible.get("hidden")?.require(Bool)?)?
}

type ProjectionNamedGet = module { export pure get(value: Str) -> Str }

test test_constant_key_projection_keeps_exported_get_function [fs, error] { |ctx|
  let root = test.temp_dir(ctx, name: "named-get-export")?
  let module_path = fp"${root}/getter.xsh"
  module_path.write_atomic("""
##! User callable with a builtin method spelling.
## Formats the supplied value.
export pure get(value: Str) -> Str { f"user:$value" }
""")?
  let loaded = module.load(module_path)?.require(ProjectionNamedGet)?
  test.eq(loaded.get("workers"), "user:workers")?
  let getter = loaded["get"]
  test.eq(getter.call("workers"), "user:workers")?
}
