#!/bin/xsh
use lib.text_input as text_input

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

  for line in text_input.read_text(paths)?.lines() {
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
