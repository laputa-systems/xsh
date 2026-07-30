#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

type PwdOptions = {operands: List[Str]}

proc main(...argv: List[Str]) [fs, error] {
  let opts: PwdOptions = cli.applet(argv, {operands: {form: "...ARG"}})?
  if opts.operands.len() > 0 {
    return Err(AppletError.Usage("pwd: too many arguments"))
  }

  print fs.cwd()?.display()
}
