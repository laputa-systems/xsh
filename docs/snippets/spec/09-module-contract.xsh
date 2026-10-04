const plugin_path = p"plugins/rust.xsh"
const root = p"."
# begin example
type BuildPlugin = module {
  export let name: Str
  export optional let description: Str
  export proc build(root: Path) [fs, process, error] -> Result[Unit]
  export pure label(name: Str) -> Str
}

let plugin = module.load(plugin_path)?.require(BuildPlugin)?
plugin.build(root)?
# end example
