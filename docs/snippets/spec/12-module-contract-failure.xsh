type BuildPlugin = module {
  export let name: Str
  export proc build(root: Path) [fs, process, error] -> Result[Unit]
}

proc build_with(plugin_path: Path, root: Path) [fs, process, error, io] -> Result[Unit] {
  # begin example
  match module.load(plugin_path)?.require(BuildPlugin) {
    Ok(plugin) => plugin.build(root)?
    Err(is MissingExport) => print "the plugin lacks a required export"
    Err(is MismatchedExport) => print "a plugin export has another signature"
    Err(error) => return Err(error)
  }
  # end example
}
