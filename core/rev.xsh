#!/bin/xsh
use lib.text_input as text_input
error AppletError = Usage(message: Str) : Usage

pure reject_unsupported(applet_name: Str, flag: Str) -> Error {
  return AppletError.Usage(f"${applet_name}: unsupported option '${flag}'")
}

proc main(...paths: List[Str]) [fs, error, io] {
  for item in paths {
    if item.starts_with("-") and item != "-" {
      return Err(reject_unsupported("rev", item))
    }
  }

  for line in text_input.read_text(paths)?.lines() {
    print line.reverse()
  }
}
