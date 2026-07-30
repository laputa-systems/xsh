#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

type TailOptions = {count: Str, quiet: Bool, verbose: Bool, paths: List[Str]}

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
  let opts: TailOptions = cli.applet(
    argv,
    {
      count: {
        form: "-n --lines N",
        default: "10",
      },
      quiet: {
        form: "-q --quiet --silent",
        default: false,
      },
      verbose: {
        form: "-v --verbose",
        default: false,
      },
      paths: {
        form: "...FILE",
      },
    },
  )?
  let count = common_int(opts.count, "line count")?
  let paths = opts.paths

  if paths.len() == 0 {
    let lines = io.stdin_text()?.lines().collect()
    let start = if lines.len() > count { lines.len() - count } else { 0 }

    for line in lines |> drop(start) {
      print $line
    }

    return
  }

  var first = true
  let show_headers = opts.verbose or paths.len() > 1 and ! opts.quiet

  for item in paths {
    if show_headers {
      if ! first {
        print ""
      }

      let label = if item == "-" { "standard input" } else { item }
      print f"==> ${label} <=="
    }

    let lines = read_text_inputs([item])?.lines().collect()
    let start = if lines.len() > count { lines.len() - count } else { 0 }

    for line in lines |> drop(start) {
      print $line
    }

    first = false
  }
}
