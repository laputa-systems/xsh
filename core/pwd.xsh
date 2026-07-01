#!/usr/bin/env -S xsh --
error AppletError = Usage(message: Str) : Usage

proc main(...argv: List[Str]) [fs, error] {
  if argv.len() > 0 {
    return Err(AppletError.Usage("pwd: too many arguments"))
  }

  print fs.cwd()?.display()
}
