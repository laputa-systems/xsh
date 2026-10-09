#!/bin/xsh
use lib.gnu

const USAGE = """Usage: yes [STRING]...
  or:  yes OPTION
Repeatedly output a line with all specified STRING(s), or 'y'.

      --help     display this help and exit
      --version  output version information and exit
"""

type YesOptions = {help: Bool, version: Bool, words: List[Str]}

pure raw_words(argv: List[Str], raw: List[Bytes]) -> List[Bytes] {
  var words: List[Bytes] = []
  var options = true
  for index in range(argv.len()) {
    if options and argv[index] == "--" {
      options = false
    } else if options and argv[index] in ["--help", "--version"] {
    } else {
      words += [raw[index]]
    }
  }
  words
}

pure join_words(words: List[Bytes]) -> Bytes {
  if words.len() == 0 { return b"y" }
  var parts: List[Bytes] = []
  for index in range(words.len()) {
    if index > 0 { parts += [b" "] }
    parts += [words[index]]
  }
  bytes.concat(parts)
}

# LINE repeated COUNT times, built by doubling.
pure repeated(line: Bytes, count: Int) -> Bytes {
  var out = line
  var copies = 1

  while copies * 2 <= count {
    out = bytes.concat([out, out])
    copies *= 2
  }

  return out when copies == count

  bytes.concat([out, line[0..(count - copies) * line.len()]])
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

  let line = bytes.concat([join_words(raw_words(argv, cli.argv_bytes())), b"\n"])
  let fitting = 2097152 / line.len()
  let count = if fitting < 2 { 2 } else { fitting }
  # Rust ignores SIGPIPE for its runtime; restore the utility default only
  # when the caller did not already ignore it.
  if process.inherited_signal_action("PIPE")? != "ignore" {
    process.set_signal_action("PIPE", "default")?
  }
  if process.signal_action("TERM")? != "ignore" {
    process.set_signal_action("TERM", "default")?
  }
  if process.signal_action("INT")? != "ignore" {
    process.set_signal_action("INT", "default")?
  }
  let chunk = repeated(line, count)
  while true {
    if let Err(failure) = io.write_stdout_bytes(chunk) {
      gnu.error(f"standard output: {gnu.strerror(failure)}")
      exit 1
    }
  }
}
