#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  f"usage: xsh applets/${applet_name}.xsh -- ${summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  AppletError.Usage(usage(applet_name, summary))
}

pure unescape(raw: Str) -> Str {
  let newline = raw.replace("\\n", "\n")
  let tab = newline.replace("\\t", "\t")
  let slash = tab.replace("\\\\", "\\")
  slash.replace("%%", "%")
}

pure render_string_lines(values: List[Str]) -> Str {
  return "" when values.len() == 0

  f"""${values.join("\n")}
"""
}

pure render_pairs_between(values: List[Str], index: Int, lines: List[Str]) -> Str {
  while index < values.len() {
    let next = lines.push(f"${(values.get(index) ?? "")} ${(values.get(index + 1) ?? "")}")
    return render_pairs_between(values, index + 2, next)
  }

  render_string_lines(lines)
}

pure render_pairs(values: List[Str]) -> Str {
  let lines = []
  render_pairs_between(values, 0, lines)
}

pure render(fmt: Str, values: List[Str]) -> Str {
  return values.join("") when fmt == "%s"

  if fmt == "%s\\n" or fmt == "%d\\n" or fmt == "%i\\n" or fmt == """%s
""" or fmt == """%d
""" or fmt == """%i
""" {
    return render_string_lines(values)
  }

  if fmt == "%s %s\\n" or fmt == """%s %s
""" {
    return render_pairs(values)
  }

  unescape(fmt)
}

proc main(fmt = "", ...values: List[Str]) [error, io] {
  return Err(usage_error("printf", "FORMAT [ARG...]")) when fmt == ""

  io.write_stdout(render(fmt, values))?
}
