#!/bin/xsh
pure delimiter(raw: Str) -> Str {
  if raw == "\\t" {
    return "\t"
  }

  if raw == "\\n" {
    return "\n"
  }

  if raw == "" {
    return "\t"
  }

  return raw
}

proc read_input(input_path: Str) [fs, error, io] -> Result[List[Str]] {
  if input_path == "-" {
    return io.stdin_text()?.lines().collect()
  }

  return fp"${input_path}".lines()?.collect()
}

proc paste_serial(paths: List[Str], delim: Str) [fs, error, io] {
  for item in paths {
    print read_input(item)?.join(delim)
  }
}

proc paste_parallel(paths: List[Str], delim: Str) [fs, error, io] {
  var columns: List[List[Str]] = []
  var count = 0

  for item in paths {
    let lines = read_input(item)?
    columns = columns.push(lines)

    if lines.len() > count {
      count = lines.len()
    }
  }

  for index in range(count) {
    let row = [column.get(index, "") for column in columns]
    print row.join(delim)
  }
}

type PasteOptions = {serial: Bool, delimiter: Str, paths: List[Str]}

proc main(...argv: List[Str]) [fs, error, io] {
  let opts: PasteOptions = cli.applet(
    argv,
    {
      serial: {
        form: "-s",
        default: false,
      },
      delimiter: {
        form: "-d DELIMS",
        default: "\t",
      },
      paths: {
        form: "...FILE",
      },
    },
  )?
  let serial = opts.serial
  let delim = delimiter(opts.delimiter)
  var paths = opts.paths

  if paths.len() == 0 {
    paths = paths.push("-")
  }

  if serial {
    paste_serial(paths, delim)?
  } else {
    paste_parallel(paths, delim)?
  }
}
