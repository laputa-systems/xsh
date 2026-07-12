#!/bin/xsh
proc main(...argv: List[Str]) [fs, error, io] {
  let parsed = cli.parse(
    argv,
    {
      append: {
        form: "-a --append",
        default: false,
      },
      ignore_interrupts: {
        form: "-i --ignore-interrupts",
        default: false,
      },
      input: {
        form: "--input PATH",
        default: "",
      },
      outputs: {
        form: "...FILE",
        repeated: true,
      },
    },
  )?

  let data = if parsed.input == "" { io.stdin_text()? } else { fp"${parsed.input}".read_text()? }
  io.write_stdout(data)?

  for out in parsed.outputs {
    let target = fp"${out}"
    let existing = if parsed.append and target.exists()? { target.read_text()? } else { "" }
    target.write(f"${existing}${data}")?
  }
}
