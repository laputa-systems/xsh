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

proc main(...paths: List[Str]) [fs, error, io] {
  for item in paths {
    if item.starts_with("-") and item != "-" {
      return Err(reject_unsupported("rev", item))
    }
  }

  for line in read_text_inputs(paths)?.lines() {
    print line.reverse()
  }
}
