type Config = {name: Str, enabled: Bool = true}

let name = "demo"
let config: Config = {name: name, enabled: true}
let kept: Config = {
  # Keep this field explanation.
  name: "kept",
  enabled: true,
}

proc observed_default() -> Bool {
  print observed
  return true
}

let observed: Config = {name: "observed", enabled: observed_default()}
print $observed.name

type Lookup = {value: Map[Int]}

let empty: Map[Int] = {}
let lookup: Lookup = {value: empty}
print ${lookup.value.len()}
print $config.name
print $kept.name
