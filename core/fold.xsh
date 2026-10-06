#!/bin/xsh
use lib.gnu
use lib.text_a2 as text

type Options = {width: Str, bytes: Bool, characters: Bool, spaces: Bool, help: Bool, version: Bool, paths: List[Str]}

pure advance(column: Int, byte: Int, by_bytes: Bool, width: Int) -> Int {
  if by_bytes { column + 1 } else if byte == 8 { if column > 0 { column - 1 } else { 0 } } else if byte == 13 { 0 } else if byte == 9 { column + 8 - column % 8 } else { column + width }
}

proc fold_chunk(data: Bytes, opts: Options, width: Int, eof: Bool) -> Bytes {
  var start = 0
  var at = 0
  var column = 0
  var blank = -1
  while at < data.len() {
    let byte = data.byte_at(at) ?? 0
    if byte == 10 {
      gnu.write_bytes(data[start..at + 1]); start = at + 1; column = 0; blank = -1; at += 1; continue
    }
    let lead = data.byte_at(at) ?? 0
    let need = if lead >= 194 and lead <= 223 { 2 } else if lead >= 224 and lead <= 239 { 3 } else if lead >= 240 and lead <= 244 { 4 } else { 1 }
    break when ! eof and ! opts.bytes and at + need > data.len()
    let unit = if opts.bytes { {size: 1, width: 1} } else { text.character(data, at) }
    let next = advance(column, byte, opts.bytes, if opts.characters { 1 } else { unit.width })
    if next > width and at > start {
      let end = if opts.spaces and blank >= start { blank + 1 } else { at }
      gnu.write_bytes(data[start..end]); gnu.write_text("\n")
      start = end; at = end; column = 0; blank = -1; continue
    }
    column = next
    if byte == 32 or byte == 9 { blank = at }
    at += unit.size
  }
  if eof { gnu.write_bytes(data[start..]); return b"" }
  data[start..]
}

proc main(...argv: List[Str]) {
  let opts: Options = cli.applet(text.numeric_options(argv, "-w"), {
    gnu: {status: 1},
    width: {form: "-w --width WIDTH", default: "80"},
    bytes: {form: "-b --bytes", default: false, conflicts: ["characters"]},
    characters: {form: "-c --characters", default: false, conflicts: ["bytes"]},
    spaces: {form: "-s --spaces", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    paths: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: fold [OPTION]... [FILE]...\nWrap each input line to fit in specified width.\n  -w, --width=WIDTH\n  -b, --bytes\n  -s, --spaces"); return }
  if opts.version { gnu.version("fold"); return }
  let width = opts.width.parse_int() ?? 0
  if width <= 0 { gnu.error(f"invalid number of columns: {gnu.quote_value(opts.width)}{if rx"^[0-9]+$".matches(opts.width) { ": Numerical result out of range" } else { "" }}"); exit 1 }
  var failed = false
  for name in if opts.paths.is_empty() { ["-"] } else { opts.paths } {
    guard let source = text.open_source(name) else { |failure| gnu.name_error(name, failure); failed = true; continue }
    defer text.close_source(source)?
    var held = b""
    loop {
      guard let chunk = text.read_source(source) else { |failure|
        let _ = fold_chunk(held, opts, width, true)
        gnu.name_error(name, failure); failed = true; break
      }
      let eof = chunk.is_empty()
      held = fold_chunk(bytes.concat([held, chunk]), opts, width, eof)
      break when eof
    }
  }
  exit text.finish(failed)
}
