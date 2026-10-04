type Config = {jobs: Int}

# begin example
proc read_config(file: Path) [fs, error] -> Result[Config] {
  json.read(file)?.require()
}
# end example
