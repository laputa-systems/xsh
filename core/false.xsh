#!/bin/xsh
use lib.gnu

const USAGE = """Usage: false [ignored command line arguments]
  or:  false OPTION
Exit with a status code indicating failure.

      --help        display this help and exit
      --version     output version information and exit

NOTE: your shell may have its own version of false, which usually supersedes
the version described here.  Please refer to your shell's documentation
for details about the options it supports.
"""

# GNU false ignores every argument and always fails; --help and --version count
# only as the sole argument, spelled out in full, and still exit with status 1.
proc main(...argv: List[Str]) [process, env, io] {
  if argv.len() == 1 {
    if argv[0] == "--help" {
      gnu.help(USAGE)
    } else if argv[0] == "--version" {
      gnu.version("false")
    }
  }

  abort(1)
}
