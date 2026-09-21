proc main() [io, error] {
  var sink = 0
  var round = 0
  while round < 200 {
    let parsed = cli.parse(
      ["--count", "3", "-D", "one", "-Dtwo", "--verbose", "src/main.xsh"],
      {
        count: {
          kind: "Int",
          required: true,
        },
        define: {
          kind: "Str",
          repeated: true,
          short: ["D"],
        },
        file: {
          kind: "Path",
          positional: true,
        },
        verbose: "Bool",
      },
    )?
    sink = sink + parsed.count + parsed.define.len()
    round = round + 1
  }
  print f"${sink}"
}
