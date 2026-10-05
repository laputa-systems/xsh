const hosts = ["10.0.0.11", "10.0.0.12", "10.0.0.13"]

let probes = [spawn run --timeout=5s ping -c 1 $host > /dev/null? for host in hosts]
let statuses = wait probes?

for i in range(hosts.len()) {
  let state = if statuses[i].ok { "up" } else { "down" }
  print f"{hosts[i]} {state}"
}
