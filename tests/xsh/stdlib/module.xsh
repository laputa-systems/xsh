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
export let name: Str = "demo"
export let description: Str = "loaded module"

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
