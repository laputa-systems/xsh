type Plugin = module {
  export let name: Str
  export optional let description: Str
  export proc execute(root: Path) [fs, error] -> Result[Unit]
}

let root_handle = fs.tempdir()?
defer fs.close_root(root_handle)?
let root = fs.root_path(root_handle)?
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
plugin.execute(root)?
print $plugin.name plugin.has("description") (fp"${root}/out.txt".read_text()?)
