#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

type PwdOptions = {operands: List[Str]}

proc main(...argv: List[Str]) [fs, error] {
  let opts: PwdOptions = cli.applet(argv, {operands: {form: "...ARG"}})?
  return Err(AppletError.Usage("pwd: too many arguments")) when ! opts.operands.is_empty()

  print fs.cwd()?.display()
}
