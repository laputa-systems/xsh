#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

type CatOptions = {paths: List[Str]}

proc main(...argv: List[Str]) [fs, error, io] {
  let opts: CatOptions = cli.applet(argv, {paths: {form: "...FILE"}})?
  let paths = opts.paths

  if paths.len() == 0 {
    io.write_stdout(io.stdin_text()?)?
    return
  }

  for arg in paths {
    if arg == "-" {
      io.write_stdout(io.stdin_text()?)?
    } else {
      io.write_stdout(fp"${arg}".read_text()?)?
    }
  }
}
