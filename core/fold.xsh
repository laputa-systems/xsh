#!/bin/xsh
use lib.gnu
use lib.text_a2 as text

type Options = {width: Str, bytes: Bool, characters: Bool, spaces: Bool, help: Bool, version: Bool, paths: List[Str]}

pure advance(column: Int, byte: Int, by_bytes: Bool, width: Int) -> Int {
  if by_bytes { column + 1 } else if byte == 8 { if column > 0 { column - 1 } else { 0 } } else if byte == 13 { 0 } else if byte == 9 { column + 8 - column % 8 } else { column + width }
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
  let input = text.read(opts.paths)
  var start = 0
  var at = 0
  var column = 0
  var blank = -1
  while at < input.data.len() {
    let byte = input.data.byte_at(at) ?? 0
    if byte == 10 {
      gnu.write_bytes(input.data[start..at + 1]); start = at + 1; column = 0; blank = -1; at += 1; continue
    }
    let unit = if opts.bytes { {size: 1, width: 1} } else { text.character(input.data, at) }
    let next = advance(column, byte, opts.bytes, if opts.characters { 1 } else { unit.width })
    if next > width and at > start {
      let end = if opts.spaces and blank >= start { blank + 1 } else { at }
      gnu.write_bytes(input.data[start..end]); gnu.write_text("\n")
      start = end; at = end; column = 0; blank = -1; continue
    }
    column = next
    if byte == 32 or byte == 9 { blank = at }
    at += unit.size
  }
  gnu.write_bytes(input.data[start..])
  exit text.finish(input.failed)
}
