#!/bin/xsh
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

pure selected_index(index: Int, spec: Str) -> Bool {
  let position = index + 1

  for raw in spec.split(",") {
    if "-" in raw {
      let parts = raw.split("-")
      let start = if parts.get(0, "") == "" { 1 } else { parts[0].parse_int() ?? 1 }
      let end_text = parts.get(1, "")

      if end_text == "" {
        if position >= start {
          return true
        }
      } else {
        let end = end_text.parse_int() ?? start

        if position >= start and position <= end {
          return true
        }
      }
    } else {
      if position == (raw.parse_int() ?? -1) {
        return true
      }
    }
  }

  return false
}

pure cut_fields(line: Str, delimiter: Str, spec: Str, separated_only: Bool) -> Str {
  if ! (delimiter in line) {
    return if separated_only { "" } else { line }
  }

  let parts = line.split(delimiter)
  let selected = [item.value for item in parts |> enumerate() if selected_index(item.index, spec)]
  return selected.join(delimiter)
}

pure cut_chars(line: Str, spec: Str) -> Str {
  let chars = line.split("")
  let selected = [item.value for item in chars |> enumerate() if selected_index(item.index, spec)]
  return selected.join("")
}

proc main(...argv: List[Str]) [fs, error, io] {
  var delimiter = "\t"
  var field_spec = ""
  var char_spec = ""
  var separated_only = false
  var paths: List[Str] = []
  var index = 0

  while index < argv.len() {
    let arg = argv[index]

    if arg == "-d" {
      delimiter = argv[index + 1]
      index += 1
    } else if arg.starts_with("-d") and arg.count_chars() > 2 {
      delimiter = arg.replace("-d", "")
    } else if arg == "-f" {
      field_spec = argv[index + 1]
      index += 1
    } else if arg.starts_with("-f") and arg.count_chars() > 2 {
      field_spec = arg.replace("-f", "")
    } else if arg == "-c" {
      char_spec = argv[index + 1]
      index += 1
    } else if arg.starts_with("-c") and arg.count_chars() > 2 {
      char_spec = arg.replace("-c", "")
    } else if arg == "-s" {
      separated_only = true
    } else {
      paths = paths.push(arg)
    }

    index += 1
  }

  if field_spec == "" and char_spec == "" {
    field_spec = "1"
  }

  for line in read_text_inputs(paths)?.lines() {
    if char_spec != "" {
      print cut_chars(line, char_spec)
    } else {
      let out = cut_fields(line, delimiter, field_spec, separated_only)

      if out != "" or ! separated_only {
        print $out
      }
    }
  }
}
