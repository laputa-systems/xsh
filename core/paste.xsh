#!/bin/xsh
use lib.gnu
use lib.text_a2 as text

type Options = {serial: Bool, delimiters: Str, zero: Bool, help: Bool, version: Bool, paths: List[Str]}

proc delimiters(spec: Str) -> List[Bytes] {
  let data = bytes.from_text(spec)
  var at = 0
  collect {
    while at < data.len() {
      let value = data.byte_at(at) ?? 0
      if value == 92 {
        if at + 1 >= data.len() { gnu.error(f"delimiter list ends with an unescaped backslash: {spec}"); exit 1 }
        at += 1
        let escaped = data.byte_at(at) ?? 0
        if escaped == 48 { yield b"" } else if escaped in [97, 98, 102, 110, 114, 116, 118] {
          let byte = if escaped == 97 { 7 } else if escaped == 98 { 8 } else if escaped == 102 { 12 } else if escaped == 110 { 10 } else if escaped == 114 { 13 } else if escaped == 116 { 9 } else { 11 }
          yield bytes.from_ints([byte])?
        } else {
          let unit = text.character(data, at)
          yield data[at..at + unit.size]
          at += unit.size - 1
        }
      } else {
        let unit = text.character(data, at)
        yield data[at..at + unit.size]
        at += unit.size - 1
      }
      at += 1
    }
    yield b"" when data.is_empty()
  }
}

type Reader = {source: text.TextSource, buffer: Bytes, eof: Bool}
type Step = {reader: Reader, found: Bool}

# Emit bytes as they arrive, so a record without a terminator cannot delay
# stdout errors or require memory proportional to an unbounded device.
proc next_record(reader: Reader, mark: Int, prefix: Bytes) -> Result[Step] {
  var current = reader
  var held_prefix = prefix
  var found = false
  while ! current.eof or ! current.buffer.is_empty() {
    var end = -1
    for at in range(current.buffer.len()) {
      if current.buffer.byte_at(at) == mark { end = at; break }
    }
    if end >= 0 {
      gnu.write_bytes(bytes.concat([held_prefix, current.buffer[..end]]))
      return Ok({reader: {source: current.source, buffer: current.buffer[end + 1..], eof: current.eof}, found: true})
    }
    if ! current.buffer.is_empty() {
      gnu.write_bytes(bytes.concat([held_prefix, current.buffer]))
      held_prefix = b""; found = true
      current = {source: current.source, buffer: b"", eof: current.eof}
    }
    if current.eof { break }
    let chunk = text.read_source(current.source, 8192)?
    current = {source: current.source, buffer: chunk, eof: chunk.is_empty()}
  }
  Ok({reader: current, found: found})
}

proc main(...argv: List[Str]) {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1},
    serial: {form: "-s --serial", default: false},
    delimiters: {form: "-d --delimiters LIST", default: "\t"},
    zero: {form: "-z --zero-terminated", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    paths: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: paste [OPTION]... [FILE]...\nMerge lines of files.\n  -s, --serial\n  -d, --delimiters=LIST\n  -z, --zero-terminated"); return }
  if opts.version { gnu.version("paste"); return }
  let paths = if opts.paths.is_empty() { ["-"] } else { opts.paths }
  let separators = delimiters(opts.delimiters)
  let end = if opts.zero { b"\0" } else { b"\n" }
  let mark = if opts.zero { 0 } else { 10 }
  var failed = false
  if opts.serial {
    for name in paths {
      guard let source = text.open_source(name) else { |failure| gnu.name_error(name, failure); failed = true; continue }
      defer text.close_source(source)?
      var reader: Reader = {source: source, buffer: b"", eof: false}
      var index = 0
      loop {
        let prefix = if index == 0 { b"" } else { separators[(index - 1) % separators.len()] }
        guard let step = next_record(reader, mark, prefix) else { |failure| gnu.name_error(name, failure); failed = true; break }
        reader = step.reader
        break when ! step.found
        index += 1
      }
      gnu.write_bytes(end)
    }
  } else {
    var readers: List[Reader] = []
    for name in paths {
      guard let source = text.open_source(name) else { |failure|
        gnu.name_error(name, failure)
        for reader in readers { text.close_source(reader.source)? }
        exit 1
      }
      readers += [{source: source, buffer: b"", eof: false}]
    }
    defer { for reader in readers { text.close_source(reader.source)? } }
    var stdin: Reader = {source: text.Stdin, buffer: b"", eof: false}
    loop {
      var found = false
      var prefix = b""
      for column in range(readers.len()) {
        if column > 0 { prefix = bytes.concat([prefix, separators[(column - 1) % separators.len()]]) }
        let reader = if paths[column] == "-" { stdin } else { readers[column] }
        guard let step = next_record(reader, mark, prefix) else { |failure| gnu.name_error(paths[column], failure); exit 1 }
        if paths[column] == "-" { stdin = step.reader } else { readers = [@readers[..column], step.reader, @readers[column + 1..]] }
        if step.found { found = true; prefix = b"" }
      }
      break when ! found
      gnu.write_bytes(bytes.concat([prefix, end]))
    }
  }
  exit text.finish(failed)
}
