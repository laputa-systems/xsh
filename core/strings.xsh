#!/usr/bin/env -S xsh --
error AppletError = Usage(message: Str) : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  return f"usage: xsh applets/${applet_name}.xsh -- ${summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  return AppletError.Usage(usage(applet_name, summary))
}

pure common_int(raw: Str, label: Str) -> Result[Int] {
  match raw {
    "1k" | "1K" => 1024
    _ => raw.parse_int().context("usage", f"unsupported ${label} '${raw}'")?
  }
}

proc read_bytes_input(applet_name: Str, paths: List[Str]) [fs, error, io] -> Result[Bytes] {
  if paths.len() == 0 {
    return io.stdin_bytes()?
  }

  if paths.len() != 1 {
    return Err(usage_error(applet_name, "FILE"))
  }

  if paths[0] == "-" {
    return io.stdin_bytes()?
  }

  return fp"${paths[0]}".read_bytes()?
}

proc main(...argv: List[Str]) [fs, error, io] {
  let parsed = cli.parse(
    argv,
    {min_len: {form: "-n --min-len N", default: "4"}, paths: {form: "...FILE", repeated: true}},
  )?

  let min_len = common_int(parsed.min_len, "length")?

  for value in read_bytes_input("strings", parsed.paths)?.strings(min_len) {
    print $value
  }
}
