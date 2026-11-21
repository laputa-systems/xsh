const begin_line = "# BEGIN fleet"
const end_line = "# END fleet"

type Entry = {ip: Str, names: List[Str]}

let hosts = p"/work/etc/hosts"

let fleet = match try { json.read(p"/work/fleet.json")?.require(List[Entry])? } {
  Ok(entries) => entries
  Err(failure) => {
    eprint f"/work/fleet.json: {failure.message}"
    exit 4
  }
}

let block = [f"{e.ip}\t{e.names.join(" ")}\n" for e in fleet].join("")
let text = hosts.read_text()?
let parts = text.split("\n")
let begins = [i for i in range(parts.len()) if parts[i] == begin_line]
let ends = [i for i in range(parts.len()) if parts[i] == end_line]

let updated = if begins.is_empty() and ends.is_empty() {
  let separator = if text != "" and ! text.ends_with("\n") { "\n" } else { "" }
  text + separator + begin_line + "\n" + block + end_line + "\n"
} else if begins.len() == 1 and ends.len() == 1 and begins[0] < ends[0] {
  parts[..begins[0] + 1].join("\n") + "\n" + block + parts[ends[0]..].join("\n")
} else {
  eprint f"{hosts}: the fleet markers are malformed; nothing was changed"
  exit 4
}

if updated == text {
  print "unchanged"
} else {
  hosts.write_atomic(updated)?
  print "updated"
}
