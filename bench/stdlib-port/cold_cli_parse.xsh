proc main() [io, error] {
  let parsed = cli.parse(
    ["--count", "3", "-D", "one", "--verbose", "src/main.xsh"],
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
  print f"${parsed.count}"
}
