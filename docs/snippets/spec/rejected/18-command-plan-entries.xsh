proc unused() [process, error] {
  let duplicate = process.command {
    run true
    run true # error: check.builder-check
  }
  let pipeline = process.command {
    run true | run true # error: check.builder-entry
  }
  let redirected = process.command {
    run true > p"out" # error: check.builder-entry
  }
}
