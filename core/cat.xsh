#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

proc main(...argv: List[Str]) [fs, error, io] {
  if argv.len() == 0 {
    io.write_stdout(io.stdin_text()?)?
    return
  }

  for arg in argv {
    if arg.starts_with("-") and arg != "-" {
      return Err(AppletError.Usage("cat: unsupported option"))
    }

    if arg == "-" {
      io.write_stdout(io.stdin_text()?)?
    } else {
      io.write_stdout(fp"${arg}".read_text()?)?
    }
  }
}
