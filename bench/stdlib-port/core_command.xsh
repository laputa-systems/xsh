proc main() [io, fs, error] {
  # A small core command driven through the migrated CLI policy, repeated so
  # the parse and the applet dispatch are both measured.
  var sink = 0
  var round = 0
  while round < 40 {
    let parsed = cli.parse(
      ["--lines", "3", "core/ls.xsh"],
      {
        lines: {kind: "Int", default: 10},
        verbose: "Bool",
        script: {
          kind: "Path",
          positional: true,
        },
      },
    )?
    sink = sink + parsed.lines + parsed.script.name().byte_len()
    round = round + 1
  }
  print f"${sink}"
}
