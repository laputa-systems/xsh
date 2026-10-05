# The first variable that is set wins; a missing one is an Err that `??`
# replaces.
let target = e"DEPLOY_TARGET" ?? e"DEFAULT_TARGET" ?? "staging"
print f"target: {target}"

env JOBS=8 VERBOSE=yes PREFIX=/opt/app {
  let jobs = env.int("JOBS", 1)?
  let verbose = env.bool("VERBOSE")?
  let prefix = env.Path.PREFIX?
  print f"jobs={jobs} verbose={verbose} bin={fp"{prefix}/bin"}"

  # A value that is not what it claims is an error, not a silent default.
  match env.int("VERBOSE", 1) {
    Ok(n) => print f"verbose level {n}"
    Err(error) => print f"VERBOSE: {error.message}"
  }
}

# Text reads never decode bytes lossily; path reads keep them.
let data = b"/srv/caf\xe9" as Path
env ({DATA_DIR: data}) {
  match e"DATA_DIR" {
    Ok(text) => print f"text: {text}"
    Err(error) => print f"as text: {error.message}"
  }

  print f"as a path, the bytes survive: {env.Path.DATA_DIR? == data}"
}
