type Config = {path: Path}

proc read_config() [fs, error] -> Result[Config] {
  json.read(p"server.json")?.require()
}

proc connect(config: Config) [fs, error] -> Result[Path] {
  guard config.path.exists() else {
    error.fail(f"no database at {config.path}")
    return config.path
  }

  config.path
}

proc serve(db: Path) [io] {
  print f"serving {db}"
}

# begin example
let setup = try {
  let config = read_config()?
  connect(config)?
}
match setup {
  Ok(db) => { serve(db) }
  Err(failure) => { eprint f"setup failed: {failure.message}" }
}
# end example
