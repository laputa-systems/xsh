#!/bin/xsh
use lib.gnu

const USAGE = """Usage: arch [OPTION]...
Print machine architecture.

      --help     display this help and exit
      --version  output version information and exit
"""

type ArchOptions = {help: Bool, version: Bool}

proc main(...argv: List[Str]) [process, env, io, error] {
  let opts: ArchOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("arch")
    return
  }

  gnu.write_text(f"{system.uname()?.machine}\n")
}
