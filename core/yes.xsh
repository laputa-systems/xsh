#!/bin/xsh
use lib.gnu

const USAGE = """Usage: yes [STRING]...
  or:  yes OPTION
Repeatedly output a line with all specified STRING(s), or 'y'.

      --help     display this help and exit
      --version  output version information and exit
"""

const CHUNK_BYTES = 65536

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
  let line_bytes = line.byte_len()
  let count = if line_bytes > CHUNK_BYTES { 1 } else { CHUNK_BYTES / line_bytes }
  let chunk = bytes.from_text(repeated(line, count))

  # Keep writes bounded so a closed pipe can stop the producer before it
  # accumulates output in memory.
  while true {
    gnu.write_bytes(chunk)
  }
}
