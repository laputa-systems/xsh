#!/bin/xsh
use lib.text_input as text_input

proc main(...argv: List[Str]) [fs, error, io] {
  let parsed = cli.parse(argv, {width: {form: "-w --width N", default: 80}, paths: {form: "...FILE", repeated: true}})?

  for line in text_input.read_text(parsed.paths)?.lines() {
    for wrapped in line.wrap(parsed.width) {
      print $wrapped
    }
  }
}
