#!/bin/xsh
use lib.gnu
use lib.textio_a1 as tio

const USAGE = """Usage: tac [OPTION]... [FILE]...
Write each FILE to standard output, last line first.

With no FILE, or when FILE is -, read standard input.

  -b, --before             attach the separator before instead of after
  -r, --regex              interpret the separator as a regular expression
  -s, --separator=STRING   use STRING as the separator instead of newline
      --help        display this help and exit
      --version     output version information and exit
"""

error TacError = Input(message: Str)

type TacOptions = {
  before: Bool,
  regex: Bool,
  separator: Str,
  help: Bool,
  version: Bool,
  files: List[Str],
}

# Where each separator starts and ends, in ascending order.
type Cuts = {starts: List[Int], ends: List[Int]}

# Separators for the default newline separator. `lines()` drops the CR of a
# CRLF, so a line ending is recognized from the bytes after it.
pure newline_cuts(data: Bytes) -> Cuts {
  var starts: List[Int] = []
  var position = 0

  for item in data.lines() {
    let end = position + item.len()
    let after = data.byte_at(end) ?? -1
    let crlf = after == 13 and (data.byte_at(end + 1) ?? -1) == 10

    if after == 10 or crlf {
      starts += [if crlf { end + 1 } else { end }]
    }

    position = end + (if crlf { 2 } else { 1 })
  }

  {starts: starts, ends: [at + 1 for at in starts]}
}

# GNU `tac` finds separators scanning backward from the end, so overlapping
# candidates such as `xxx` for `xx` match the last two bytes.
pure literal_cuts(data: Bytes, separator: Bytes) -> Cuts {
  let width = separator.len()
  let first = separator.byte_at(0) ?? 0
  var starts: List[Int] = []
  var at = data.len() - width

  while at >= 0 {
    if (data.byte_at(at) ?? -1) == first and (width == 1 or data[at..at + width] == separator) {
      starts += [at]
      at -= width
    } else {
      at -= 1
    }
  }

  let count = starts.len()
  let ascending = [starts[count - 1 - step] for step in range(count)]

  {starts: ascending, ends: [start + width for start in ascending]}
}

proc regex_cuts(data: Bytes, pattern: Str) [error] -> Result[Cuts] {
  guard let text = data.utf8() else {
    return Err(TacError.Input("regular expression separators need valid UTF-8 input"))
  }

  let found = regex.compile(pattern)?.find(text)

  Ok({starts: [item.start for item in found], ends: [item.end for item in found]})
}

pure reverse_records(data: Bytes, cuts: Cuts, before: Bool) -> Bytes {
  var edges = [0]
  edges += if before { cuts.starts } else { cuts.ends }
  edges += [data.len()]

  var pieces: List[Bytes] = []

  let count = edges.len() - 1

  for step in range(count) {
    let index = count - 1 - step
    pieces += [data[edges[index]..edges[index + 1]]]
  }

  bytes.concat(pieces)
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: TacOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      before: {form: "-b --before", default: false},
      regex: {form: "-r --regex", default: false},
      separator: {form: "-s --separator STRING", default: "\n"},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      files: {form: "...FILE"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("tac")
    return
  }

  if opts.separator == "" {
    gnu.error("separator cannot be empty")
    exit 1
  }

  let separator = bytes.from_text(opts.separator)
  var failed = false

  for name in if opts.files.len() == 0 { ["-"] } else { opts.files } {
    guard let source = tio.open_source(name) else { |failure|
      gnu.error(f"failed to open {gnu.quote(name)} for reading: {gnu.strerror(failure)}")
      failed = true
      continue
    }

    var chunks: List[Bytes] = []
    var offset = 0

    loop {
      guard let chunk = tio.read_chunk(source, offset) else { |failure|
        gnu.error(f"{gnu.quote_maybe(name)}: read error: {gnu.strerror(failure)}")
        failed = true
        chunks = []
        break
      }

      break when chunk.len() == 0

      chunks += [chunk]
      offset += chunk.len()
    }

    let data = bytes.concat(chunks)

    continue when data.len() == 0

    let cuts = if opts.regex {
      guard let found = regex_cuts(data, opts.separator) else { |failure|
        gnu.error(failure.message)
        failed = true
        continue
      }

      found
    } else if opts.separator == "\n" {
      newline_cuts(data)
    } else {
      literal_cuts(data, separator)
    }

    gnu.write_bytes(reverse_records(data, cuts, opts.before))
  }

  if failed {
    exit 1
  }
}
