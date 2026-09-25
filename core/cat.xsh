#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

type CatOptions = {paths: List[Str]}

proc main(...argv: List[Str]) [fs, error, io] {
  let opts: CatOptions = cli.applet(argv, {paths: {form: "...FILE"}})?
  let paths = opts.paths

  if paths.len() == 0 {
    io.write_stdout_bytes(io.stdin_bytes()?)?
    return
  }

  for arg in paths {
    if arg == "-" {
      io.write_stdout_bytes(io.stdin_bytes()?)?
    } else {
      io.write_stdout_bytes(fp"${arg}".read_bytes()?)?
    }
  }
}
