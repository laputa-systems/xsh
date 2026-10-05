# begin example
type Argv = NonEmpty[Str]

pure program(argv: Argv) -> Str {
  argv.first()
}

proc launch(extra: List[Str]) [process, error] {
  # A literal with an element written out is checked where it is written.
  let argv: Argv = ["git", "status", @extra]
  print program(argv)
  run @argv

  # Appending keeps the guarantee; a slice gives the list back.
  let verbose = argv.push("--verbose")
  let flags = verbose[1..]
  print verbose.last() flags.len()

  # Any other list is validated once, at an explicit boundary.
  let command = extra.require(Argv)?
  run @command
}

# end example

launch(["--short"])
