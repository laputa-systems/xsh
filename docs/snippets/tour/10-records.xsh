type Host = {name: Str, role: Str, cores: Int}

const fleet: List[Host] = [
  {name: "web-1", role: "web", cores: 4},
  {name: "web-2", role: "web", cores: 8},
  {name: "db-1", role: "db", cores: 16},
]

let web = [h.name for h in fleet if h.role == "web"]
let cores_by_name = {h.name: h.cores for h in fleet}
print f"web: {web.join(" ")}; db-1 has {cores_by_name.get("db-1") ?? 0} cores"

for {name, cores, ..} in fleet {
  print f"{name}={cores}"
}

let upgraded = {...fleet[0], cores: 32}
print f"{upgraded.name}: {fleet[0].cores} -> {upgraded.cores}"

for role in fleet |> group-by .role {
  print f"{role.key}: {[h.name for h in role.items].join(",")}"
}

match "deploy web-2 --force".fields() {
  ["deploy", target, ..flags] => print f"deploy {target} with {flags.len()} flag(s)"
  ["status"] => print "status"
  _ => print "usage: deploy TARGET | status"
}
