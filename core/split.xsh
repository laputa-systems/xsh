#!/usr/bin/env -S xsh --
error AppletError = Usage(message: Str) : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  return f"usage: xsh applets/${applet_name}.xsh -- ${summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  return AppletError.Usage(usage(applet_name, summary))
}

pure common_int(raw: Str, label: Str) -> Result[Int] {
  if raw.ends_with("k") or raw.ends_with("K") {
    return raw.replace("k", "").replace("K", "").parse_int().context("usage", f"unsupported ${label} '${raw}'")? * 1024
  }

  if raw.ends_with("m") or raw.ends_with("M") {
    return raw.replace("m", "").replace("M", "").parse_int().context("usage", f"unsupported ${label} '${raw}'")? * 1024 * 1024
  }

  return raw.parse_int().context("usage", f"unsupported ${label} '${raw}'")?
}

pure suffix(index: Int) -> Str {
  let letters = [
    "a",
    "b",
    "c",
    "d",
    "e",
    "f",
    "g",
    "h",
    "i",
    "j",
    "k",
    "l",
    "m",
    "n",
    "o",
    "p",
    "q",
    "r",
    "s",
    "t",
    "u",
    "v",
    "w",
    "x",
    "y",
    "z",
  ]

  return f"${letters[index / 26 % 26]}${letters[index % 26]}"
}

proc read_text_input(source: Str) [fs, error, io] -> Result[Str] {
  if source == "-" {
    return io.stdin_text()?
  }

  return fp"${source}".read_text()?
}

proc read_bytes_input(source: Str) [fs, error, io] -> Result[Bytes] {
  if source == "-" {
    return io.stdin_bytes()?
  }

  return fp"${source}".read_bytes()?
}

proc main(...argv: List[Str]) [fs, error, io] {
  var lines_per_file = 100
  var bytes_per_file = 0
  var paths: List[Str] = []
  var index = 0

  while index < argv.len() {
    let arg = argv[index]

    if arg == "-l" {
      lines_per_file = common_int(argv[index + 1], "line count")?
      index += 1
    } else if arg.starts_with("-l") and arg.count_chars() > 2 {
      lines_per_file = common_int(arg.replace("-l", ""), "line count")?
    } else if arg == "-b" {
      bytes_per_file = common_int(argv[index + 1], "byte count")?
      index += 1
    } else if arg.starts_with("-b") and arg.count_chars() > 2 {
      bytes_per_file = common_int(arg.replace("-b", ""), "byte count")?
    } else if arg == "-a" {
      if argv[index + 1] != "2" {
        return Err(usage_error("split", "only two-letter suffixes are supported"))
      }

      index += 1
    } else {
      paths = paths.push(arg)
    }

    index += 1
  }

  if lines_per_file <= 0 or bytes_per_file < 0 {
    return Err(usage_error("split", "[-l N|-b N] [FILE [PREFIX]]"))
  }

  if paths.len() > 2 {
    return Err(usage_error("split", "[-l N|-b N] [FILE [PREFIX]]"))
  }

  let input_path = paths.get(0, "-")
  let prefix = paths.get(1, "x")

  if bytes_per_file > 0 {
    let input = read_bytes_input(input_path)?
    var offset = 0
    var chunk = 0

    while offset < input.len() {
      fp"${prefix}${suffix(chunk)}".write(input.slice(offset, bytes_per_file))?
      offset += bytes_per_file
      chunk += 1
    }

    return
  }

  let input = read_text_input(input_path)?.lines().collect()
  var chunk = 0
  var current: List[Str] = []

  for item in input |> enumerate() {
    current = current.push(item.value)

    if current.len() == lines_per_file or item.index + 1 == input.len() {
      fp"${prefix}${suffix(chunk)}".write(f"""${current.join("\n")}
""")?

      current = []
      chunk += 1
    }
  }
}
