type Plugin = module {
  export let name: Str
  export optional let description: Str
  export proc execute(root: Path) [fs, error] -> Result[Unit]
}

proc test_module_load(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "module")?
  let plugin_path = fp"${root}/plugin.xsh"

  fs.write(
    plugin_path,
    """
##! Test plugin module contract.

## Exposes the plugin name.
export let name: Str = "demo"
## Exposes the optional plugin description.
export let description: Str = "loaded module"

## Writes the plugin name into the requested root.
export proc execute(root: Path) [fs, error] -> Result[Unit] {
  fs.write(fp"\${root}/out.txt", name)?
}
""",
  )?

  let plugin = module.load(plugin_path)?.require(Plugin)?
  test.eq(plugin.name, "demo")?
  test.ok(plugin.has("description"))?
  test.ok(plugin.keys().contains("name"))?
  plugin.execute(root)?
  test.eq(fp"${root}/out.txt".read_text()?, "demo")?
}

proc test_static_and_loaded_modules_reject_the_same_contract_mismatches(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "module-contract-mismatches")?
  let optional_path = fp"${root}/bad_optional.xsh"
  let effect_path = fp"${root}/bad_effect.xsh"
  optional_path.write("""
##! Module with an incompatible optional export.

## Deliberately not the contract's string type.
export let description: Int = 1
""")?
  effect_path.write("""
##! Module with an implementation effect outside its contract.

## Deliberately requires an extra process capability.
export proc execute() [fs, process, error] -> Result[Unit] {
  return Ok()
}
""")?

  let optional_contract = """
type Plugin = module {
  export optional let description: Str
}
"""
  let effect_contract = """
type Runner = module {
  export proc execute() [fs, error] -> Result[Unit]
}
"""

  for source in [
    f"""${optional_contract}
use bad_optional
let _: Plugin = bad_optional
""",
    f"""${effect_contract}
use bad_effect
let _: Runner = bad_effect
""",
    f"""${optional_contract}
let _ = module.load(p"${optional_path}")?.require(Plugin)?
""",
    f"""${effect_contract}
let _ = module.load(p"${effect_path}")?.require(Runner)?
""",
  ] {
    let result = test.run_script(ctx, source, [], {XSH_MODULE_PATH: root.display()})?
    test.ok(! result.success, source)?
  }
}

proc test_static_module_namespace_satisfies_the_same_contract(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "static-module-contract")?
  fp"${root}/runner.xsh".write("""
##! Static runner fixture.
## Writes the fixture marker.
export proc execute(root: Path) [fs, error] -> Result[Unit] {
  fp"\${root}/out.txt".write("static")?
}
""")?

  let result = test.run_script(
    ctx,
    f"""
type Runner = module {
  export proc execute(root: Path) [fs, error] -> Result[Unit]
}

use runner

proc main() [fs, error] -> Result[Unit] {
  let checked: Runner = runner
  checked.execute(p"${root}")?
}

main()?
""",
    [],
    {XSH_MODULE_PATH: root.display()},
  )?
  test.ok(result.success, result.stderr)?
  test.eq(fp"${root}/out.txt".read_text()?, "static")?
}

proc test_static_module_exports_bind_one_namespace(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "module-namespace")?
  fp"${root}/helper.xsh".write("""
##! Namespace-only static module fixture.

## A value export.
export let value: Str = "helper"
## An effectful callable export.
export proc execute() [error] -> Result[Unit] {
  return Ok()
}
## A pure callable export.
export pure render(value: Str) -> Str {
  return value.upper()
}
## A stream export.
export stream numbers() [] -> Stream[Int] {
  yield 1
}
## A tagged union export.
export type State = Ready | Stopped(Str)
## An error family export with an error facet.
export error HelperError = Failed(detail: Str) : Temporary
""")?

  let positive = test.run_script(
    ctx,
    """
use helper

proc check_stream() [] -> Unit {
  for value in helper.numbers() {
    let _: Int = value
  }
}

proc main() [error] -> Result[Unit] {
  let _: helper.State = helper.Ready
  let _: helper.State = helper.Stopped("stopped")
  let _: helper.HelperError = helper.HelperError.Failed(detail: "failed")
  helper.execute()?
  print \$helper.value
  print \${helper.render("ok")}
  let dynamic: Any = helper.HelperError.Failed(detail: "failed")
  match dynamic {
    _ is helper.Temporary => print "temporary"
    _ => print "missing"
  }
}


main()?
""",
    [],
    {XSH_MODULE_PATH: root.display()},
  )?
  test.ok(positive.success, positive.stderr)?

  for source in [
    """use helper
execute()
""",
    """use helper
let _: State = helper.Ready
""",
    """use helper
Stopped("stopped")
""",
    """use helper
HelperError.Failed(detail: "failed")
""",
    """use helper
match helper.HelperError.Failed(detail: "failed") {
  _ is Temporary => print "bare facet"
  _ => print "fallback"
}
""",
  ] {
    let result = test.run_script(ctx, source, [], {XSH_MODULE_PATH: root.display()})?
    test.ok(! result.success, source)?
  }

  let aliased = test.run_script(
    ctx,
    """
use helper as h

proc main() [error] -> Result[Unit] {
  let _: h.State = h.Ready
  let _: h.HelperError = h.HelperError.Failed(detail: "failed")
  h.execute()?
  print h.render("ok")
}

main()?
""",
    [],
    {XSH_MODULE_PATH: root.display()},
  )?
  test.ok(aliased.success, aliased.stderr)?

  for source in [
    """use helper as h
helper.execute()
""",
    """use helper as h
execute()
""",
    """use helper as h
let _: helper.State = h.Ready
""",
    """use helper as h
let _: State = h.Ready
""",
    """use helper as h
HelperError.Failed(detail: "failed")
""",
  ] {
    let result = test.run_script(ctx, source, [], {XSH_MODULE_PATH: root.display()})?
    test.ok(! result.success, source)?
  }
}

proc test_stream_exports_are_namespace_members_not_module_contract_members(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "module-stream-contract")?
  fp"${root}/stream_only.xsh".write("""
export stream numbers() [] -> Stream[Int] {
  yield 1
}
""")?

  let concrete_empty = test.run_script(
    ctx,
    """
type Runner = module {
  export proc run() [error] -> Result[Unit]
}

use stream_only
let _: Runner = stream_only
""",
    [],
    {XSH_MODULE_PATH: root.display()},
  )?
  test.ok(! concrete_empty.success, concrete_empty.stderr)?

  let stream_contract = test.run_script(
    ctx,
    """
type Invalid = module {
  export stream numbers() [] -> Stream[Int]
}
""",
    [],
    {XSH_MODULE_PATH: root.display()},
  )?
  test.ok(! stream_contract.success, stream_contract.stderr)?
}
