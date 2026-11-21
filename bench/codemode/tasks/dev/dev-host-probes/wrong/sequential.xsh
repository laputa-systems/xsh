use svc

type Up = {host: Str, version: Str}

let hosts = json.read(p"/work/hosts.json")?.require(List[Str])?
var up: List[Up] = []
var down: List[Str] = []
for host in hosts {
  match svc.health_probe(host, timeout: 1s) {
    Ok(probe) => {
      if probe.status == "ok" {
        up += [Up(host: host, version: probe.version)]
      } else {
        down += [host]
      }
    }
    Err(_) => down += [host]
  }
}

print (json.encode({up: up, down: down})?)
