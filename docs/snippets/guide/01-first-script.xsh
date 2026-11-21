#!/usr/bin/env -S xsh --
type Host = {name: Str, port: Int}

let raw = json.decode("""[{"name": "db", "port": 5432}, {"name": "web", "port": 80}]""")?
let hosts = raw.require(List[Host])?

for host in hosts |> sort-by .port {
  print f"{host.name} listens on {host.port}"
}
