proc main() [io, error] {
  let rejected = cli.parse(["--count", "not-a-number"], {
    count: {
      kind: "Int",
      required: true,
    },
  })
  match rejected {
    Ok(_) => {
      print "accepted"
    }
    Err(failure) => {
      print f"${failure.message}"
    }
  }
}
