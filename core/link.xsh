#!/bin/xsh
use lib.gnu

const USAGE = """Usage: link FILE1 FILE2
Call the link function to create a link named FILE2 to an existing FILE1.

      --help     display this help and exit
      --version  output version information and exit
"""

type LinkOptions = {help: Bool, version: Bool, operands: List[Str]}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: LinkOptions = cli.applet(
    argv,
    {
      gnu: {status: 1, permute: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      operands: {form: "...FILE"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("link")
    return
  }

  if opts.operands.len() == 0 {
    gnu.missing_operand()
  }

  if opts.operands.len() == 1 {
    gnu.missing_operand_after(opts.operands[0])
  }

  if opts.operands.len() > 2 {
    gnu.extra_operand(opts.operands[2])
  }

  let source = fp"{opts.operands[0]}"
  let dest = fp"{opts.operands[1]}"

  match fs.link(source, dest) {
    Ok(_) => return
    Err(failure) => {
      gnu.error(f"cannot create link {gnu.quote(opts.operands[1])} to {gnu.quote(opts.operands[0])}: {gnu.strerror(failure)}")
      exit 1
    }
  }
}
