#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

type LinkOptions = {operands: List[Str]}

proc main(...argv: List[Str]) [fs, error] {
  let opts: LinkOptions = cli.applet(argv, {operands: {form: "...PATH"}})?
  return Err(AppletError.Usage("usage: link SOURCE DEST")) when opts.operands.len() != 2

  fp"{opts.operands[0]}".hardlink(fp"{opts.operands[1]}")
}
