let initial = ["é"]
type Config = {name: Str = "demo", names: List[Str] = initial, count: Int = -3}
type Alias = Config
let config = Alias()
print $config.name
print $config.count
print ${config.names[0]}
