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

type CutOptions = {delimiter: Str, fields: Str, characters: Str, separated_only: Bool, paths: List[Str]}

proc main(...argv: List[Str]) [fs, error, io] {
  let opts: CutOptions = cli.applet(
    argv,
    {
      delimiter: {
        form: "-d DELIMITER",
        default: "\t",
      },
      fields: {
        form: "-f LIST",
        default: "",
      },
      characters: {
        form: "-c LIST",
        default: "",
      },
      separated_only: {
        form: "-s",
        default: false,
      },
      paths: {
        form: "...FILE",
      },
    },
  )?
  let delimiter = opts.delimiter
  var field_spec = opts.fields
  let char_spec = opts.characters
  let separated_only = opts.separated_only
  let paths = opts.paths

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
