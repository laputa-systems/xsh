#!/bin/xsh
use lib.gnu

const USAGE = """Usage: yes [STRING]...
  or:  yes OPTION
Repeatedly output a line with all specified STRING(s), or 'y'.

      --help     display this help and exit
      --version  output version information and exit
"""

# The runtime buffers stdout until the script ends and reports no write
# failures, so an endless loop would only grow memory. The output is bounded to
# this many bytes of whole lines (at least 16 lines for very long operands);
# real `yes` runs until its reader closes the pipe.
const OUTPUT_LIMIT = 33554432

type YesOptions = {help: Bool, version: Bool, words: List[Str]}

# LINE repeated COUNT times, built by doubling.
pure repeated(line: Str, count: Int) -> Str {
  var out = line
  var copies = 1

  while copies * 2 <= count {
    out += out
    copies *= 2
  }

  return out when copies == count

  out + out.byte_slice(0, length: (count - copies) * line.byte_len())
}

proc main(...argv: List[Str]) [process, env, error, io] {
  let opts: YesOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      words: {form: "...STRING"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("yes")
    return
  }

  let line = (if opts.words.is_empty() { "y" } else { opts.words.join(" ") }) + "\n"
  let fitting = OUTPUT_LIMIT / line.byte_len()
  let count = if fitting < 16 { 16 } else { fitting }

  gnu.write_text(repeated(line, count))
}
