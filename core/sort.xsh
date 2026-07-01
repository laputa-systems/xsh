#!/usr/bin/env -S xsh --
error AppletError = Usage(message: Str) : Usage

pure reject_unsupported(applet_name: Str, flag: Str) -> Error {
  return AppletError.Usage(f"${applet_name}: unsupported option '${flag}'")
}

pure numeric_key(line: Str) -> Int {
  return line.trim().words().get(0, "0").parse_int() ?? 0
}

pure key_index(spec: Str) -> Int {
  return (spec.split(",").get(0, "1").split(".").get(0, "1").parse_int() ?? 1) - 1
}

pure field_key(line: Str, delimiter: Str, field: Int, fold_case: Bool) -> Str {
  let parts = if delimiter == "" { line.trim().words() } else { line.split(delimiter) }
  let key = parts.get(field, "")
  return if fold_case { key.lower() } else { key }
}

pure numeric_field_key(line: Str, delimiter: Str, field: Int) -> Int {
  return field_key(line, delimiter, field, false).parse_int() ?? 0
}

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
  var reverse = false
  var unique = false
  var numeric = false
  var fold_case = false
  var key_field = 0
  var has_key = false
  var delimiter = ""
  var output = p""
  var has_output = false
  var paths: List[Str] = []
  var index = 0

  while index < argv.len() {
    let arg = argv[index]

    match arg {
      "-r" | "--reverse" => reverse = true
      "-u" | "--unique" => unique = true
      "-n" | "--numeric-sort" => numeric = true
      "-f" | "--ignore-case" => fold_case = true
      "-b" => {}
      "-k" => {
        key_field = key_index(argv[index + 1])
        has_key = true
        index += 1
      }
      "-t" => {
        delimiter = argv[index + 1]
        index += 1
      }
      "-nr" | "-rn" => {
        numeric = true
        reverse = true
      }
      "-o" => {
        output = fp"${argv[index + 1]}"
        has_output = true
        index += 1
      }
      _ => {
        if arg.starts_with("-") {
          if arg.starts_with("-k") and arg.count_chars() > 2 {
            key_field = key_index(arg.replace("-k", ""))
            has_key = true
          } else if arg.starts_with("-t") and arg.count_chars() > 2 {
            delimiter = arg.replace("-t", "")
          } else {
            return Err(reject_unsupported("sort", arg))
          }
        } else {
          paths = paths.push(arg)
        }
      }
    }

    index += 1
  }

  let input = read_text_inputs(paths)?

  let sorted = if numeric and has_key {
    if reverse {
      input.lines() |> sort-by --desc numeric_field_key(., delimiter, key_field)
    } else {
      input.lines() |> sort-by numeric_field_key(., delimiter, key_field)
    }
  } else if has_key {
    if reverse {
      input.lines() |> sort-by --desc field_key(., delimiter, key_field, fold_case)
    } else {
      input.lines() |> sort-by field_key(., delimiter, key_field, fold_case)
    }
  } else if numeric {
    if reverse { input.lines() |> sort-by --desc numeric_key(.) } else { input.lines() |> sort-by numeric_key(.) }
  } else if fold_case {
    if reverse { input.lines() |> sort-by --desc .lower() } else { input.lines() |> sort-by .lower() }
  } else if reverse {
    input.lines() |> sort-by --desc .
  } else {
    input.lines() |> sort
  }

  let lines = if unique { sorted |> unique-by . } else { sorted }

  let text = if lines.len() == 0 {
    ""
  } else {
    f"""${lines.join("\n")}
"""
  }

  if has_output {
    output.write(text)?
  } else {
    for line in lines {
      print $line
    }
  }
}
