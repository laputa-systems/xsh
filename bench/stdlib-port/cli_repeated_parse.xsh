proc main() [io, error] {
  # A long operand list with repeated and positional fields, parsed in a loop.
  var operand: List[Str] = []
  var index = 0
  while index < 40 {
    operand = operand.extend([f"src/file${index}.xsh", "-I", f"include${index}"])
    index = index + 1
  }
  var sink = 0
  var round = 0
  while round < 40 {
    let parsed = cli.parse(
      operand,
      {
        include: {
          kind: "Str",
          repeated: true,
          short: ["I"],
        },
        files: {
          kind: "Str",
          repeated: true,
          positional: true,
        },
      },
    )?
    sink = sink + parsed.include.len() + parsed.files.len()
    round = round + 1
  }
  print f"${sink}"
}
