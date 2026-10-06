#!/bin/xsh
use lib.gnu
error AppletError = Usage : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  f"usage: xsh applets/{applet_name}.xsh -- {summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  AppletError.Usage(usage(applet_name, summary))
}

pure unescape(raw: Str) -> Str {
  let newline = raw.replace("\\n", with: "\n")
  let tab = newline.replace("\\t", with: "\t")
  let slash = tab.replace("\\\\", with: "\\")
  slash.replace("%%", with: "%")
}

pure render_string_lines(values: List[Str]) -> Str {
  return "" when values.is_empty()

  f"""{values.join("\n")}
"""
}

pure render_pairs_between(values: List[Str], index: Int, lines: List[Str]) -> Str {
  while index < values.len() {
    let next = lines.push(f"{values.get(index) ?? ""} {values.get(index + 1) ?? ""}")
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

# Once FORMAT begins, every later argument is data, including option-looking
# strings. The explicit terminator is recognized only before FORMAT.
proc main(...argv: List[Str]) [error, io, process, env] {
  var arguments = argv
  if ! arguments.is_empty() {
    if arguments[0] == "--" {
      arguments = arguments[1..]
    } else if arguments[0] == "--help" {
      gnu.help("Usage: printf FORMAT [ARGUMENT]...\nPrint arguments according to FORMAT.\n")
      return
    } else if arguments[0] == "--version" {
      gnu.version("printf")
      return
    }
  }
  return Err(usage_error("printf", "FORMAT [ARG...]")) when arguments.is_empty()
  io.write_stdout(render(arguments[0], arguments[1..]))
}
