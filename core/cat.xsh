#!/bin/xsh
use lib.gnu
use lib.textio_a1 as tio

const USAGE = """Usage: cat [OPTION]... [FILE]...
Concatenate FILE(s) to standard output.

With no FILE, or when FILE is -, read standard input.

  -A, --show-all           equivalent to -vET
  -b, --number-nonblank    number nonempty output lines, overrides -n
  -e                       equivalent to -vE
  -E, --show-ends          display $ at end of each line
  -n, --number             number all output lines
  -s, --squeeze-blank      suppress repeated empty output lines
  -t                       equivalent to -vT
  -T, --show-tabs          display TAB characters as ^I
  -u                       (ignored)
  -v, --show-nonprinting   use ^ and M- notation, except for LFD and TAB
      --help        display this help and exit
      --version     output version information and exit

Examples:
  cat f - g  Output f's contents, then standard input, then g's contents.
  cat        Copy standard input to standard output.
"""

type CatOptions = {
  show_all: Bool,
  number_nonblank: Bool,
  show_e: Bool,
  show_ends: Bool,
  number: Bool,
  squeeze: Bool,
  show_t: Bool,
  show_tabs: Bool,
  unbuffered: Bool,
  show_nonprinting: Bool,
  help: Bool,
  version: Bool,
  files: List[Str],
}

# `special[byte]` marks the bytes that `-v` and `-T` rewrite; `table[byte]` is
# the replacement.
type Style = {
  number: Bool,
  nonblank: Bool,
  squeeze: Bool,
  ends: Bool,
  convert: Bool,
  cr: Bytes,
  special: List[Bool],
  table: List[Bytes],
}

# Output state carried across operands: the unfinished line, the next line
# number, and whether the previous output line was empty (for `-s`).
type State = {pending: Bytes, line: Int, blank: Bool}

type Rendered = {out: Bytes, state: State}

const CARET = "@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_"
const PRINTABLE = " !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~"

# The `-v` spelling of a byte below 128.
pure low_byte(value: Int) -> Str {
  return f"^{CARET.byte_slice(value, length: 1)}" when value < 32
  return "^?" when value == 127

  PRINTABLE.byte_slice(value - 32, length: 1)
}

# How `-v` and `-T` show one byte, or "" when it is copied unchanged; TAB and
# LFD stay as they are under `-v`.
pure display_byte(value: Int, tabs: Bool, nonprinting: Bool) -> Str {
  return "^I" when value == 9 and tabs
  return "" when ! nonprinting or value == 9 or value == 10 or (value >= 32 and value < 127)
  return f"M-{low_byte(value - 128)}" when value >= 128

  low_byte(value)
}

pure make_style(opts: CatOptions) -> Style {
  let ends = opts.show_ends or opts.show_all or opts.show_e
  let tabs = opts.show_tabs or opts.show_all or opts.show_t
  let nonprinting = opts.show_nonprinting or opts.show_all or opts.show_e or opts.show_t
  let table = [display_byte(value, tabs, nonprinting) for value in range(256)]

  {
    number: opts.number,
    nonblank: opts.number_nonblank,
    squeeze: opts.squeeze,
    ends: ends,
    convert: tabs or nonprinting,
    cr: if nonprinting or ends { b"^M" } else { b"\r" },
    special: [text != "" for text in table],
    table: [bytes.from_text(text) for text in table],
  }
}

pure convert(content: Bytes, style: Style) -> Bytes {
  return content when ! style.convert

  var pieces: List[Bytes] = []
  var start = 0

  for index in range(content.len()) {
    let value = content.byte_at(index) ?? 0

    if style.special[value] {
      pieces += [content[start..index], style.table[value]]
      start = index + 1
    }
  }

  return content when start == 0

  bytes.concat([@pieces, content[start..]])
}

# Render the complete lines of `data` (the unfinished last line is kept in
# `pending` unless `final`). `lines()` drops a trailing CR, so the terminator
# is recovered from the bytes after each line.
pure render(data: Bytes, final: Bool, style: Style, state: State) -> Rendered {
  var pieces: List[Bytes] = []
  var position = 0
  var line = state.line
  var blank = state.blank
  var pending = b""

  for item in data.lines() {
    let end = position + item.len()
    let after = data.byte_at(end) ?? -1
    let crlf = after == 13 and (data.byte_at(end + 1) ?? -1) == 10
    let newline = after == 10 or crlf
    let lone_cr = if after == 13 and ! crlf { 1 } else { 0 }

    if ! newline and ! final {
      if ! style.number and ! style.nonblank and ! style.squeeze {
        let end = if after == 13 { end } else { data.len() }
        pieces += [convert(data[position..end], style)]
        pending = if after == 13 { b"\r" } else { b"" }
      } else {
        pending = data[position..]
      }

      break
    }

    let empty = after == 10 and item.len() == 0
    let numbered = if style.nonblank { ! empty } else { style.number }

    if ! (style.squeeze and empty and blank) {
      if numbered {
        pieces += [bytes.from_text(f"{line:>6}\t")]
        line += 1
      }

      pieces += [convert(data[position..end + lone_cr], style)]

      if crlf {
        pieces += [style.cr]
      }

      if newline {
        pieces += [if style.ends { b"$\n" } else { b"\n" }]
      }
    }

    let width = if crlf { 2 } else if after == 10 or lone_cr == 1 { 1 } else { 0 }
    blank = empty
    position = end + width
  }

  {out: bytes.concat(pieces), state: {pending: pending, line: line, blank: blank}}
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: CatOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      show_all: {form: "-A --show-all", default: false},
      number_nonblank: {form: "-b --number-nonblank", default: false},
      show_e: {form: "-e", default: false},
      show_ends: {form: "-E --show-ends", default: false},
      number: {form: "-n --number", default: false},
      squeeze: {form: "-s --squeeze-blank", default: false},
      show_t: {form: "-t", default: false},
      show_tabs: {form: "-T --show-tabs", default: false},
      unbuffered: {form: "-u", default: false},
      show_nonprinting: {form: "-v --show-nonprinting", default: false},
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
    gnu.version("cat")
    return
  }

  let style = make_style(opts)
  let plain = ! (style.number or style.nonblank or style.squeeze or style.ends or style.convert)
  let operands = if opts.files.len() == 0 { ["-"] } else { opts.files }
  let out = tio.standard_file(1)
  var state = {pending: b"", line: 1, blank: false}
  var written = 0
  var failed = false

  for name in operands {
    guard let source = tio.open_source(name) else { |failure|
      gnu.name_error(name, failure)
      failed = true
      continue
    }

    if tio.is_unsafe_overwrite(source, out, written)? {
      gnu.error(f"{gnu.quote_maybe(name)}: input file is output file")
      failed = true
      continue
    }

    var offset = 0

    loop {
      let chunk_size = if plain { tio.CHUNK } else { 1024 }

      guard let chunk = tio.read_chunk(source, offset, chunk_size) else { |failure|
        gnu.name_error(name, failure)
        failed = true
        break
      }

      break when chunk.len() == 0

      offset += chunk.len()

      if plain {
        gnu.write_bytes(chunk)
        written += chunk.len()
      } else {
        let rendered = render(bytes.concat([state.pending, chunk]), false, style, state)
        state = rendered.state
        gnu.write_bytes(rendered.out)
        written += rendered.out.len()
      }
    }
  }

  if ! plain {
    gnu.write_bytes(render(state.pending, true, style, {...state, pending: b""}).out)
  }

  if failed {
    exit 1
  }
}
