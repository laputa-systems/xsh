type Config = {tool: Str, args: List[Str]}

type Migration = {id: Str, name: Str}

const migration_name = rx"^([0-9]{4})-.*\.sql$"

proc append(file: Path, text: Str) -> Result[Unit] {
  let before = if file.exists()? { file.read_text()? } else { "" }
  file.write(before + text)?
}

let config = json.read(p"/work/migrate.json")?.require(Config)?
let state = p"/work/state"
let applied_file = p"/work/state/applied"
let log_file = p"/work/state/migrate.log"
let error_file = p"/work/state/last-error"

state.mkdir(parents: true)?
let original = if applied_file.exists()? { applied_file.read_text()? } else { "" }
var applied = [line for line in original.split("\n") if line != ""]
applied_file.write([f"{id}\n" for id in applied].join(""))?

let pending = fs.children(p"/work/migrations")?
  |> where { |entry| entry.kind == "file" }
  |> map { |entry|
    let groups = migration_name.captures(entry.name)
    if groups.len() == 2 { Migration(id: groups[1], name: entry.name) } else { Migration(id: "", name: "") }
  }
  |> where { |m| m.id != "" and m.id not in applied }
  |> sort-by .id

let tool = config.tool
let tool_args = [a for a in config.args if a != ""]

for migration in pending {
  let name = migration.name
  let result = cd (p"/work/migrations") { run.capture --text MIGRATION_ID=${migration.id} $tool @tool_args apply $name }?
  let status = result.status.exit_code() ?? 255

  append(log_file, result.stdout)?
  if status != 0 {
    error_file.write(result.stderr)?
    print f"failed {migration.id}: exit {status}"
    exit 4
  }

  applied += [migration.id]
  applied_file.write([f"{id}\n" for id in applied].join(""))?
  print f"applied {migration.id}"
}

error_file.remove()?
