proc unused() [process, error] {
  # begin example
  let plan = process.command { # error: parse.builder-block
    cwd = p"build"
    run true
  }
  # end example
}
