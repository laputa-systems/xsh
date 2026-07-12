#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

pure reject_unsupported(applet_name: Str, flag: Str) -> Error {
  return AppletError.Usage(f"${applet_name}: unsupported option '${flag}'")
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
  var show_counts = false
  var only_duplicates = false
  var paths: List[Str] = []

  for arg in argv {
    match arg {
      "-c" => show_counts = true
      "-d" => only_duplicates = true
      _ => {
        if arg.starts_with("-") {
          return Err(reject_unsupported("uniq", arg))
        }

        paths = paths.push(arg)
      }
    }
  }

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
