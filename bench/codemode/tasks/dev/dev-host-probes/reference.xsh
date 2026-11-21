use svc

type Up = {host: Str, version: Str}

let hosts = json.read(p"/work/hosts.json")?.require(List[Str])?

let versions = hosts |> par-map(jobs: 3) { |host|
  match svc.health_probe(host, timeout: 1s) {
    Ok(probe) => if probe.status == "ok" { probe.version } else { null }
    Err(_) => null
  }
}

var up: List[Up] = []
var down: List[Str] = []
for i in range(hosts.len()) {
  let version = versions[i]
  if version == null {
    down += [hosts[i]]
  } else {
    up += [Up(host: hosts[i], version: version)]
  }
}

print (json.encode({up: up, down: down})?)
