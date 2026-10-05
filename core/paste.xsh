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
  var columns: List[List[Bytes]] = []
  var stdin: List[Bytes] = []
  var read_stdin = false
  var failed = false
  for name in paths {
    if name == "-" {
      if ! read_stdin { stdin = text.records(io.stdin_bytes()?, mark); read_stdin = true }
      if opts.serial {
        columns += [stdin]; stdin = []
      } else { columns += [[]] }
    } else {
      guard let data = gnu.read_operand(name) else { |failure|
        gnu.name_error(name, failure); failed = true
        if ! opts.serial { exit 1 }
        continue
      }
      columns += [text.records(data, mark)]
    }
  }
  if opts.serial {
    for column in columns {
      var chunks: List[Bytes] = []
      for item in column |> enumerate() {
        if item.index > 0 { chunks += [separators[(item.index - 1) % separators.len()]] }
        chunks += [item.value]
      }
      gnu.write_bytes(bytes.concat([@chunks, end]))
    }
  } else {
    var row = 0
    var cursor = 0
    loop {
      var chunks: List[Bytes] = []
      var found = false
      for item in columns |> enumerate() {
        if item.index > 0 { chunks += [separators[(item.index - 1) % separators.len()]] }
        let line = if paths[item.index] == "-" { stdin.get(cursor) } else { item.value.get(row) }
        if paths[item.index] == "-" { cursor += 1 }
        if let Ok(value) = line { found = true; chunks += [value] }
      }
      break when ! found
      gnu.write_bytes(bytes.concat([@chunks, end]))
      row += 1
    }
  }
  exit text.finish(failed)
}
