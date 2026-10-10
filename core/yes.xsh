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
pure repeated(line: Bytes, count: Int) -> Bytes {
  var out = line
  var copies = 1

  while copies * 2 <= count {
    out = bytes.concat([out, out])
    copies *= 2
  }

  return out when copies == count

  bytes.concat([out, out.slice(0, length: (count - copies) * line.len())])
}

proc main(...argv: List[Bytes]) [process, env, error, io] {
  # The xsh host leaves SIGPIPE ignored, so a closed reader would surface as
  # EPIPE and end with status 141 rather than the signal death GNU yes has.
  # Restore the default action so the process is terminated by SIGPIPE.
  process.set_signal_action("PIPE", "default")?

  let prepared = gnu.prepare_arguments(argv)
  let opts: YesOptions = cli.applet(
    prepared.text,
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

  # Operands may be undecodable bytes, so the line is assembled as bytes.
  var pieces: List[Bytes] = []
  for index in range(opts.words.len()) {
    if index > 0 { pieces += [b" "] }
    pieces += [gnu.argument_bytes(opts.words[index], prepared.raw)]
  }
  if opts.words.is_empty() { pieces = [b"y"] }
  pieces += [b"\n"]

  let line = bytes.concat(pieces)
  let line_len = line.len()
  let count = if line_len > CHUNK_BYTES { 1 } else { CHUNK_BYTES / line_len }
  let chunk = repeated(line, count)

  # Keep writes bounded so a closed pipe can stop the producer before it
  # accumulates output in memory.
  while true {
    gnu.write_bytes(chunk)
  }
}
