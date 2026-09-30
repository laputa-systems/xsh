const commands = {
  build: {aliases: ["compile"], positionals: ["root"], types: {root: "Path"}, rest: "raw", options: {jobs: {kind: "Int", default: 4}}},
}
proc command_values() [error] -> Result[Str] {
  let first = cli.commands(["build", "workspace", "--jobs", "6"], commands)?
  let second = cli.commands(["compile", "workspace"], commands)?
  Ok(first.root.display() + "/" + second.command)
}
