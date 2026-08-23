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

proc test_static_module_namespace_satisfies_the_same_contract(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "static-module-contract")?
  fp"${root}/runner.xsh".write("""
export proc run(root: Path) [fs, error] -> Result[Unit] {
  fp"\${root}/out.txt".write("static")?
}
""")?

  let result = test.run_script(
    ctx,
    f"""
type Runner = module {
  export proc run(root: Path) [fs, error] -> Result[Unit]
}

use runner

let checked: Runner = runner
checked.run(p"${root}")?
""",
    [],
    {XSH_MODULE_PATH: root.display()},
  )?
  test.ok(result.success, result.stderr)?
  test.eq(fp"${root}/out.txt".read_text()?, "static")?
}
