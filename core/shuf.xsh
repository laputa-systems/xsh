#!/bin/xsh
use lib.text_input as text_input

pure common_int(raw: Str, label: Str) -> Result[Int] {
  match raw {
    "1k" | "1K" => 1024
    _ => raw.parse_int().context("usage", f"unsupported ${label} '${raw}'")?
  }
}

proc main(...argv: List[Str]) [fs, error, io] {
  let parsed = cli.parse(
    argv,
    {head_count: {form: "-n --head-count N", default: "0"}, paths: {form: "...FILE", repeated: true}},
  )?

  let limit = common_int(parsed.head_count, "count")?
  let shuffled = text_input.read_text(parsed.paths)?.lines() |> shuffle
  let lines = if limit > 0 { shuffled |> take(limit) } else { shuffled }

  for line in lines {
    print $line
  }
}
