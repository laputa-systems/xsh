#!/bin/xsh
use lib.gnu

const USAGE = """Usage: true [ignored command line arguments]
  or:  true OPTION
Exit with a status code indicating success.

      --help        display this help and exit
      --version     output version information and exit

NOTE: your shell may have its own version of true, which usually supersedes
the version described here.  Please refer to your shell's documentation
for details about the options it supports.
"""

# GNU true ignores every argument; --help and --version count only as the sole
# argument, spelled out in full.
proc main(...argv: List[Str]) [process, env, io] {
  return when argv.len() != 1

  if argv[0] == "--help" {
    gnu.help(USAGE)
  } else if argv[0] == "--version" {
    gnu.version("true")
  }
}
