#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

type SortOptions = {
  reverse: Bool,
  unique: Bool,
  numeric: Bool,
  fold_case: Bool,
  blank: Bool,
  key: Str,
  delimiter: Str,
  output: Str,
  paths: List[Str],
}

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
  let opts: SortOptions = cli.applet(
    argv,
    {
      reverse: {
        form: "-r --reverse",
        default: false,
      },
      unique: {
        form: "-u --unique",
        default: false,
      },
      numeric: {
        form: "-n --numeric-sort",
        default: false,
      },
      fold_case: {
        form: "-f --ignore-case",
        default: false,
      },
      blank: {
        form: "-b",
        default: false,
      },
      key: {
        form: "-k KEY",
        default: "",
      },
      delimiter: {
        form: "-t DELIMITER",
        default: "",
      },
      output: {
        form: "-o FILE",
        default: "",
      },
      paths: {
        form: "...FILE",
      },
    },
  )?
  let has_key = opts.key != ""
  let key_field = if has_key { key_index(opts.key) } else { 0 }
  let has_output = opts.output != ""
  let output = if has_output { fp"${opts.output}" } else { p"" }
  let delimiter = opts.delimiter
  let paths = opts.paths

  let input = read_text_inputs(paths)?

  let sorted = if opts.numeric and has_key {
    if opts.reverse {
      input.lines() |> sort-by --desc numeric_field_key(., delimiter, key_field)
    } else {
      input.lines() |> sort-by numeric_field_key(., delimiter, key_field)
    }
  } else if has_key {
    if opts.reverse {
      input.lines() |> sort-by --desc field_key(., delimiter, key_field, opts.fold_case)
    } else {
      input.lines() |> sort-by field_key(., delimiter, key_field, opts.fold_case)
    }
  } else if opts.numeric {
    if opts.reverse { input.lines() |> sort-by --desc numeric_key(.) } else { input.lines() |> sort-by numeric_key(.) }
  } else if opts.fold_case {
    if opts.reverse { input.lines() |> sort-by --desc .lower() } else { input.lines() |> sort-by .lower() }
  } else if opts.reverse {
    input.lines() |> sort-by --desc .
  } else {
    input.lines() |> sort
  }

  let lines = if opts.unique { sorted |> unique-by . } else { sorted }

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
