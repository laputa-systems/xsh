type Config = {name: Str, enabled: Bool = true}

let name = "demo"
let config = Config(name:)
print $config.name
