#!/bin/xsh
use lib.text_input as text_input

pure selected_index(index: Int, spec: Str) -> Bool {
  let position = index + 1

  for raw in spec.split(",") {
    if "-" in raw {
      let parts = raw.split("-")
      let start = if (parts.get(0) ?? "") == "" { 1 } else { parts[0].parse_int() ?? 1 }
      let end_text = parts.get(1) ?? ""

      if end_text == "" {
        return true when position >= start
      } else {
        let end = end_text.parse_int() ?? start

        return true when position >= start and position <= end
      }
    } else {
      return true when position == (raw.parse_int() ?? -1)
    }
  }

  false
}

pure cut_fields(line: Str, delimiter: Str, spec: Str, separated_only: Bool) -> Str {
  guard delimiter in line else {
    return if separated_only { "" } else { line }
  }

  let parts = line.split(delimiter)
  let selected = [item.value for item in parts |> enumerate() if selected_index(item.index, spec)]
  selected.join(delimiter)
}

pure cut_chars(line: Str, spec: Str) -> Str {
  let chars = line.split("")
  let selected = [item.value for item in chars |> enumerate() if selected_index(item.index, spec)]
  selected.join("")
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
  let {characters: char_spec, separated_only, paths, ..} = opts

  if field_spec == "" and char_spec == "" {
    field_spec = "1"
  }

  for line in text_input.read_text(paths)?.lines() {
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
