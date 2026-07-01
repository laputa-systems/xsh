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
  let shuffled = read_text_inputs(parsed.paths)?.lines() |> shuffle
  let lines = if limit > 0 { shuffled |> take(limit) } else { shuffled }

  for line in lines {
    print $line
  }
}
