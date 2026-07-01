#!/usr/bin/env -S xsh --
proc read_text_inputs(paths: List[Str]) [fs, error, io] -> Result[Str] {
  var out = ""

  if paths.len() == 0 {
    return io.stdin_text()?
  }

  for item in paths {
    if item == "-" {
      out = f"${out}${io.stdin_text()?}"
    } else {
      out = f"${out}${fp"${item}".read_text()?}"
    }
  }

  return out
}

proc main(...argv: List[Str]) [fs, error, io] {
  let parsed = cli.parse(argv, {width: {form: "-w --width N", default: 80}, paths: {form: "...FILE", repeated: true}})?

  for line in read_text_inputs(parsed.paths)?.lines() {
    for wrapped in line.wrap(parsed.width) {
      print $wrapped
    }
  }
}
