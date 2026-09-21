proc main() [io, error] {
  let text = cli.usage(
    {
      count: {
        kind: "Int",
        required: true,
        help: "how many",
      },
      define: {
        kind: "Str",
        repeated: true,
        short: ["D"],
        help: "define a value",
      },
      verbose: "Bool",
    },
    "demo",
  )
  print f"${text.byte_len()}"
}
