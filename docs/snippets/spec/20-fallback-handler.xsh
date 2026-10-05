const config_path = p"build.json"

type Config = {jobs: Int}

proc load_config(file: Path) [fs, error] -> Result[Config] {
  json.read(file)?.require()
}

pure default_config() -> Config {
  Config(1)
}

# begin example
let config = load_config(config_path) ?? { |failure|
  eprint f"using defaults: {failure.message}"
  default_config()
}
# end example
