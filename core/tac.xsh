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
type RegexText = {text: Str, source_offsets: List[Int]}
type RawArgument = {marker: Str, value: Bytes}
type PreparedArguments = {text: List[Str], raw: List[RawArgument]}

# Keep the option prefix visible when a separator is attached to its option.
# Short options before `s` must be flags because `s` consumes the rest.
pure separator_offset(argument: Bytes) -> Int {
  if argument.len() < 2 or argument.byte_at(0) != 45 { return 0 }

  if argument.byte_at(1) == 45 {
    for at in range(2, argument.len()) {
      if argument.byte_at(at) == 61 {
        if let Ok(_) = argument[0..at].utf8() { return at + 1 }
        return 0
      }
    }
    return 0
  }

  for at in range(1, argument.len()) {
    let option = argument.byte_at(at) ?? -1
    if option == 115 { return at + 1 }
    return 0 when option not in [98, 114]
  }

  0
}

# Keep raw operands and separator bytes while the option parser sees safe text markers.
pure prepare_arguments(argv: List[Bytes]) -> PreparedArguments {
  var text: List[Str] = []
  var raw: List[RawArgument] = []
  var options = true
  var separator_next = false

  for index in range(argv.len()) {
    let argument = argv[index]
    let offset = if options and ! separator_next { separator_offset(argument) } else { 0 }
    match argument.utf8() {
      Ok(value) => text += [value]
      Err(_) => {
        let marker = f"\0tac-raw-argument-{index}\0"
        if offset > 0 {
          text += [f"{argument[0..offset].utf8() ?? ""}{marker}"]
          raw += [{marker: marker, value: argument[offset..]}]
        } else {
          text += [marker]
          raw += [{marker: marker, value: argument}]
        }
      }
    }

    if separator_next {
      separator_next = false
    } else if options {
      let value = argument.utf8() ?? ""
      if value == "--" {
        options = false
      } else {
        separator_next = (offset > 0 and offset == argument.len() and argument.byte_at(1) != 45) or
          (value.starts_with("--") and value.byte_len() > 2 and "--separator".starts_with(value))
      }
    }
  }

  {text: text, raw: raw}
}

pure argument_bytes(value: Str, raw: List[RawArgument]) -> Bytes {
  for argument in raw {
    if argument.marker == value { return argument.value }
  }

  bytes.from_text(value)
}

proc source_for(name: Bytes) [fs, error] -> Result[tio.Source, Error] {
  if let Ok(text) = name.utf8() {
    return tio.open_source(text)
  }

  let input_path = Path.parse_bytes(name)?
  let target = input_path.resolve()?
  let entry = target.metadata()?
  let kind = entry.mode / 4096 % 16
  let mode = if kind == 8 and entry.size > 0 {
    "file"
  } else if kind == 2 or kind == 6 {
    "device"
  } else {
    "whole"
  }

  Ok({name: target.display(), path: target, mode: mode, kind: kind, size: entry.size})
}

# Separators for the default newline separator. `lines()` drops the CR of a
# CRLF, so a line ending is recognized from the bytes after it.
pure newline_cuts(data: Bytes) -> Cuts {
  var position = 0

  let starts: List[Int] = collect {
    for item in data.lines() {
      let end = position + item.len()
      let after = data.byte_at(end) ?? -1
      let crlf = after == 13 and (data.byte_at(end + 1) ?? -1) == 10

      yield if crlf { end + 1 } else { end } when after == 10 or crlf

      position = end + (if crlf { 2 } else { 1 })
    }
  }

  {starts: starts, ends: [at + 1 for at in starts]}
}

# GNU `tac` finds separators scanning backward from the end, so overlapping
# candidates such as `xxx` for `xx` match the last two bytes.
pure literal_cuts(data: Bytes, separator: Bytes) -> Cuts {
  let width = separator.len()
  let first = separator.byte_at(0) ?? 0
  var at = data.len() - width

  let starts: List[Int] = collect {
    while at >= 0 {
      if (data.byte_at(at) ?? -1) == first and (width == 1 or data[at..at + width] == separator) {
        yield at
        at -= width
      } else {
        at -= 1
      }
    }
  }

  let count = starts.len()
  let ascending = [starts[count - 1 - step] for step in range(count)]

  {starts: ascending, ends: [start + width for start in ascending]}
}

pure normalize_regex(pattern: Str) -> Str {
  let source = bytes.from_text(pattern)
  var at = 0
  var in_class = false

  let pieces: List[Bytes] = collect {
    while at < source.len() {
      let byte = source.byte_at(at) ?? 0
      let next = source.byte_at(at + 1) ?? -1

      if byte == 92 {
        if ! in_class and next == 124 {
          yield b"|"
          at += 2
        } else {
          yield source[at..at + (if next < 0 { 1 } else { 2 })]
          at += if next < 0 { 1 } else { 2 }
        }
      } else if byte == 91 and ! in_class {
        in_class = true
        yield source[at..at + 1]
        at += 1
      } else if byte == 93 and in_class {
        in_class = false
        yield source[at..at + 1]
        at += 1
      } else if byte == 94 and ! in_class and at > 0 and (source.byte_at(at - 1) ?? -1) not in [40, 124] {
        yield b"\\^"
        at += 1
      } else if byte == 36 and ! in_class and at + 1 < source.len() and next not in [41, 124] {
        yield b"\\$"
        at += 1
      } else {
        yield source[at..at + 1]
        at += 1
      }
    }
  }

  bytes.concat(pieces).utf8() ?? pattern
}

pure previous_boundary(data: Bytes, at: Int) -> Int {
  var previous = at - 1

  while previous > 0 and (data.byte_at(previous) ?? 0).bit_and(192) == 128 {
    previous -= 1
  }

  previous
}

# Map each byte to one scalar so the text regex engine can search invalid
# UTF-8 data while preserving offsets into the source bytes.
proc regex_byte_text(data: Bytes) [error] -> Result[RegexText] {
  var chars: List[Bytes] = []
  var source_offsets = [0]

  for index in range(data.len()) {
    let byte = data.byte_at(index) ?? 0
    let encoded = if byte < 128 {
      bytes.from_ints([byte])?
    } else {
      bytes.from_ints([192 + byte / 64, 128 + byte % 64])?
    }
    chars += [encoded]
    if byte >= 128 { source_offsets += [index] }
    source_offsets += [index + 1]
  }

  Ok({text: bytes.concat(chars).utf8()?, source_offsets: source_offsets})
}

# GNU tac searches regex separators backward, using each prior match start as
# the end of the next search range.
proc regex_cuts_in_text(text: Str, pattern: Str, source_offsets: List[Int]) [error] -> Result[Cuts] {
  let compiled = regex.compile(f"(?m){normalize_regex(pattern)}")?
  let text_bytes = bytes.from_text(text)

  if pattern == "$" or pattern.starts_with("^") {
    let found = compiled.find(text)
    return Ok({
      starts: [source_offsets[item.start] for item in found],
      ends: [source_offsets[item.end] for item in found],
    })
  }

  var cursor = text_bytes.len()
  var reversed_starts: List[Int] = []
  var reversed_ends: List[Int] = []

  loop {
    var at = cursor
    var match_start = -1
    var match_end = -1

    loop {
      let found = compiled.find(text.byte_slice(at, length: cursor - at))

      if ! found.is_empty() and found[0].start == 0 {
        match_start = at
        match_end = at + found[0].end
        break
      }

      break when at == 0
      at = previous_boundary(text_bytes, at)
    }

    break when match_start < 0

    reversed_starts += [match_start]
    reversed_ends += [match_end]

    let next = if match_end == match_start { previous_boundary(text_bytes, match_start) } else { match_start }
    break when next < 0 or next >= cursor
    cursor = next
  }

  let count = reversed_starts.len()
  let starts = [source_offsets[reversed_starts[count - 1 - step]] for step in range(count)]
  let ends = [source_offsets[reversed_ends[count - 1 - step]] for step in range(count)]

  Ok({starts: starts, ends: ends})
}

proc regex_cuts(data: Bytes, pattern: Bytes) [error] -> Result[Cuts] {
  if let Ok(text) = data.utf8() {
    if let Ok(regex_pattern) = pattern.utf8() {
      return regex_cuts_in_text(text, regex_pattern, [index for index in range(data.len() + 1)])
    }
  }

  let input = regex_byte_text(data)?
  let regex_pattern = regex_byte_text(pattern)?.text
  regex_cuts_in_text(input.text, regex_pattern, input.source_offsets)
}

pure reverse_records(data: Bytes, cuts: Cuts, before: Bool) -> Bytes {
  var edges = [0]
  edges += if before { cuts.starts } else { cuts.ends }
  edges += [data.len()]

  let count = edges.len() - 1

  let pieces: List[Bytes] = collect {
    for step in range(count) {
      let index = count - 1 - step
      yield data[edges[index]..edges[index + 1]]
    }
  }

  bytes.concat(pieces)
}

proc main(...argv: List[Bytes]) [fs, process, env, error, io] {
  let prepared = prepare_arguments(argv)
  let opts: TacOptions = cli.applet(
    prepared.text,
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

  let separator_value = argument_bytes(opts.separator, prepared.raw)

  if separator_value.is_empty() and opts.regex {
    gnu.error("separator cannot be empty")
    exit 1
  }

  # An empty literal separator denotes a NUL byte.
  let separator = if separator_value.is_empty() { b"\0" } else { separator_value }
  var failed = false

  for name in if opts.files.is_empty() { ["-"] } else { opts.files } {
    let raw_name = argument_bytes(name, prepared.raw)

    guard let source = source_for(raw_name) else { |failure|
      gnu.error(f"failed to open {gnu.quote_bytes(raw_name)} for reading: {gnu.strerror(failure)}")
      failed = true
      continue
    }

    var chunks: List[Bytes] = []
    var offset = 0

    loop {
      guard let chunk = tio.read_chunk(source, offset) else { |failure|
        gnu.error(f"{gnu.quote_bytes(raw_name, always: false)}: read error: {gnu.strerror(failure)}")
        failed = true
        chunks = []
        break
      }

      break when chunk.is_empty()

      chunks += [chunk]
      offset += chunk.len()
    }

    let data = bytes.concat(chunks)

    continue when data.is_empty()

    let cuts = if opts.regex {
      guard let found = regex_cuts(data, separator_value) else { |failure|
        gnu.error(failure.message)
        failed = true
        continue
      }

      found
    } else if separator == b"\n" {
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
