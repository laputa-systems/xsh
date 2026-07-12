#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

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

pure common_int(raw: Str, label: Str) -> Result[Int] {
  match raw {
    "1k" | "1K" => 1024
    _ => raw.parse_int().context("usage", f"unsupported ${label} '${raw}'")?
  }
}

proc main(...argv: List[Str]) [fs, error, io] {
  var count = 10
  var quiet = false
  var verbose = false
  var paths: List[Str] = []
  var index = 0

  while index < argv.len() {
    let arg = argv[index]

    if arg == "-n" or arg == "--lines" {
      if index + 1 >= argv.len() {
        return Err(AppletError.Usage("head: option requires an argument -- n"))
      }

      count = common_int(argv[index + 1], "line count")?
      index += 2
      continue
    }

    if arg == "-q" or arg == "--quiet" or arg == "--silent" {
      quiet = true
    } else if arg == "-v" or arg == "--verbose" {
      verbose = true
    } else if arg.starts_with("-n") and arg.count_chars() > 2 {
      count = common_int(arg.replace("-n", ""), "line count")?
    } else if arg.starts_with("-") and arg.count_chars() > 1 {
      count = common_int(arg.replace("-", ""), "line count")?
    } else {
      paths = paths.push(arg)
    }

    index += 1
  }

  if paths.len() == 0 {
    for line in io.stdin_text()?.lines() |> take(count) {
      print $line
    }

    return
  }

  var first = true
  let show_headers = verbose or paths.len() > 1 and ! quiet

  for item in paths {
    if show_headers {
      if ! first {
        print ""
      }

      let label = if item == "-" { "standard input" } else { item }
      print f"==> ${label} <=="
    }

    let input = read_text_inputs([item])?

    for line in input.lines() |> take(count) {
      print $line
    }

    first = false
  }
}
