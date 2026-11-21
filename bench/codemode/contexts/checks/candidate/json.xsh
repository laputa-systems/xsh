type Config = {name: Str, ports: List[Int]}

let ok = json.decode("""{"name": "api", "ports": [80, 443], "extra": true}""")?.require(Config)?
print f"{ok.name} {ok.ports.len()}"
match json.decode("""{"name": "api", "ports": [80, "x"]}""")?.require(Config) {
  Ok(_) => print "accepted"
  Err(error) => print (error.message)
}
print (json.encode({zeta: 1, alpha: [true, false], mid: "m"})?)
