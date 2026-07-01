#!/usr/bin/env -S xsh --
error AppletError = Usage(message: Str) : Usage

proc main(...argv: List[Str]) [fs, error] {
  if argv.len() != 2 {
    return Err(AppletError.Usage("usage: link SOURCE DEST"))
  }

  fp"${argv[0]}".hardlink(fp"${argv[1]}")?
}
