#!/bin/xsh
error AppletError = Usage : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  f"usage: xsh applets/{applet_name}.xsh -- {summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  AppletError.Usage(usage(applet_name, summary))
}

pure common_int(raw: Str, label: Str) -> Result[Int] {
  if raw.ends_with("k") or raw.ends_with("K") {
    return raw.replace("k", "").replace("K", "").parse_int().context("usage", f"unsupported {label} '{raw}'")? * 1024
  }

  if raw.ends_with("m") or raw.ends_with("M") {
    return raw.replace("m", "").replace("M", "").parse_int().context("usage", f"unsupported {label} '{raw}'")? * 1024 * 1024
  }

  raw.parse_int().context("usage", f"unsupported {label} '{raw}'")?
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

  f"{letters[index / 26 % 26]}{letters[index % 26]}"
}

proc read_text_input(source: Str) [fs, error, io] -> Result[Str] {
  return io.stdin_text()? when source == "-"

  fp"{source}".read_text()?
}

proc read_bytes_input(source: Str) [fs, error, io] -> Result[Bytes] {
  return io.stdin_bytes()? when source == "-"

  fp"{source}".read_bytes()?
}

type SplitOptions = {lines: Str, bytes: Str, suffix_length: Str, paths: List[Str]}

proc main(...argv: List[Str]) [fs, error, io] {
  let opts: SplitOptions = cli.applet(
    argv,
    {
      lines: {
        form: "-l LINES",
        default: "100",
      },
      bytes: {
        form: "-b BYTES",
        default: "0",
      },
      suffix_length: {
        form: "-a LENGTH",
        default: "2",
      },
      paths: {
        form: "...FILE",
      },
    },
  )?
  let lines_per_file = common_int(opts.lines, "line count")?
  let bytes_per_file = common_int(opts.bytes, "byte count")?
  let paths = opts.paths

  if opts.suffix_length != "2" {
    return Err(usage_error("split", "only two-letter suffixes are supported"))
  }

  if lines_per_file <= 0 or bytes_per_file < 0 {
    return Err(usage_error("split", "[-l N|-b N] [FILE [PREFIX]]"))
  }

  return Err(usage_error("split", "[-l N|-b N] [FILE [PREFIX]]")) when paths.len() > 2

  let input_path = paths.get(0) ?? "-"
  let prefix = paths.get(1) ?? "x"

  if bytes_per_file > 0 {
    let input = read_bytes_input(input_path)?
    var offset = 0
    var chunk = 0

    while offset < input.len() {
      let remaining = input.len() - offset
      let chunk_end = if bytes_per_file < remaining { offset + bytes_per_file } else { input.len() }
      fp"{prefix}{suffix(chunk)}".write(input[offset..chunk_end])
      offset += bytes_per_file
      chunk += 1
    }

    return
  }

  let input = read_text_input(input_path)?.lines().collect()
  var chunk = 0
  var current = []

  for item in input |> enumerate() {
    current += [item.value]

    if current.len() == lines_per_file or item.index + 1 == input.len() {
      fp"{prefix}{suffix(chunk)}".write(f"""{current.join("\n")}
""")

      current = []
      chunk += 1
    }
  }
}
