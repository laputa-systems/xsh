#!/bin/xsh
use lib.gnu

const USAGE = """Usage: printenv [OPTION]... [VARIABLE]...
Print the values of the specified environment VARIABLE(s).
If no VARIABLE is specified, print name and value pairs for them all.

  -0, --null     end each output line with NUL, not newline
      --help     display this help and exit
      --version  output version information and exit

NOTE: your shell may have its own version of printenv, which usually supersedes
the version described here.  Please refer to your shell's documentation
for details about the options it supports.
"""

type PrintenvOptions = {null: Bool, help: Bool, version: Bool, names: List[Str]}

# Exit statuses: 0 when every named variable is set, 1 when any is not, 2 for
# a usage error.
proc main(...argv: List[Str]) [process, env, error, io] {
  let opts: PrintenvOptions = cli.applet(
    argv,
    {
      gnu: {status: 2},
      null: {form: "-0 --null", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      names: {form: "...VARIABLE"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("printenv")
    return
  }

  let ending = if opts.null { "\0" } else { "\n" }

  if opts.names.len() == 0 {
    for item in env.list()? {
      gnu.write_text(f"{item.name}={item.value}{ending}")
    }

    return
  }

  var missing = false

  for name in opts.names {
    if "=" in name {
      missing = true
    } else if let Ok(value) = env.get(name) {
      gnu.write_text(f"{value}{ending}")
    } else {
      missing = true
    }
  }

  if missing {
    exit 1
  }
}
