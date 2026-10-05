#!/bin/xsh
use lib.text_input

error AppletError = Usage : Usage

pure reject_unsupported(applet_name: Str, flag: Str) -> Error {
  AppletError.Usage(f"{applet_name}: unsupported option '{flag}'")
}

proc main(...paths: List[Str]) [fs, error, io] {
  for item in paths {
    return Err(reject_unsupported("rev", item)) when item.starts_with("-") and item != "-"
  }

  for line in text_input.read_text(paths)?.lines() {
    print line.reverse()
  }
}
