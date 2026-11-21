type Host = {name: Str, port: Int}

pure label(host: Host) -> Str {
  f"{host.name}:{host.port}"
}

let raw = json.decode("""[{"name": "web", "port": 80}, {"name": "db", "port": "5432"}]""")?
match raw.require(List[Host]) {
  Ok(hosts) => print f"{hosts.len()} hosts"
  Err(error) => print f"rejected: {error.message}"
}

tempdir dir {
  let file = fp"{dir}/hosts.txt"
  file.write_atomic("web 80\nmail 25\n\ndb 5432\n")
  var hosts: List[Host] = []
  for line in file.lines()? {
    let fields = line.fields()
    if fields.len() == 2 {
      hosts += [Host(name: fields[0], port: fields[1] as Int)]
    }
  }

  let labels = [label(h) for h in hosts |> sort-by .port]
  let listed = run.text printf "%s\n" @labels
  print (listed.trim())
  print (json.encode(hosts |> where .port < 1000 |> map .name)?)
}
