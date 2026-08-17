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

type UniqOptions = {show_counts: Bool, only_duplicates: Bool, paths: List[Str]}

proc main(...argv: List[Str]) [fs, error, io] {
  let opts: UniqOptions = cli.applet(
    argv,
    {
      show_counts: {
        form: "-c",
        default: false,
      },
      only_duplicates: {
        form: "-d",
        default: false,
      },
      paths: {
        form: "...FILE",
      },
    },
  )?
  let show_counts = opts.show_counts
  let only_duplicates = opts.only_duplicates
  let paths = opts.paths

  var previous = ""
  var count = 0

  for line in read_text_inputs(paths)?.lines() {
    if count == 0 {
      previous = line
      count = 1
    } else if line == previous {
      count += 1
    } else {
      if ! only_duplicates or count > 1 {
        if show_counts {
          print f"${tui.left_pad(f"${count}", 7)} ${previous}"
        } else {
          print $previous
        }
      }

      previous = line
      count = 1
    }
  }

  if count > 0 and (! only_duplicates or count > 1) {
    if show_counts {
      print f"${tui.left_pad(f"${count}", 7)} ${previous}"
    } else {
      print $previous
    }
  }
}
