#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  return f"usage: xsh applets/${applet_name}.xsh -- ${summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  return AppletError.Usage(usage(applet_name, summary))
}

pure reject_unsupported(applet_name: Str, flag: Str) -> Error {
  return AppletError.Usage(f"${applet_name}: unsupported option '${flag}'")
}

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

proc main(...argv: List[Str]) [fs, error, io] {
  var serial = false
  var delim = "\t"
  var paths: List[Str] = []
  var index = 0

  while index < argv.len() {
    let arg = argv[index]

    if arg == "-s" {
      serial = true
    } else if arg == "-d" {
      if index + 1 >= argv.len() {
        return Err(usage_error("paste", "[-s] [-d DELIMS] [FILE...]"))
      }

      delim = delimiter(argv[index + 1])
      index += 1
    } else if arg.starts_with("-d") and arg.count_chars() > 2 {
      delim = delimiter(arg.replace("-d", ""))
    } else if arg.starts_with("-") and arg != "-" {
      return Err(reject_unsupported("paste", arg))
    } else {
      paths = paths.push(arg)
    }

    index += 1
  }

  if paths.len() == 0 {
    paths = paths.push("-")
  }

  if serial {
    paste_serial(paths, delim)?
  } else {
    paste_parallel(paths, delim)?
  }
}
