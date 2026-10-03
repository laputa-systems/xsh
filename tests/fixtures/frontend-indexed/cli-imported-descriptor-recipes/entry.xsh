use config as c
const commands = {...c.descriptor.commands, clean: {positionals: ["root"], types: {root: "Path"}, rest: "raw"}}
const invocation = {argv: ["clean", "workspace", "extra"], commands: commands}
let parsed = cli.commands(...invocation)?
let root: Path = parsed.root
let rest: List[Str] = parsed.raw
print root.display() ${rest[0]}
