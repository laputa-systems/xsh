#!/bin/xsh
use lib.gnu
use lib.textio_a1 as tio
use unix

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
type StreamRendered = {out: Bytes, pending_cr: Bool}
type RawArgument = {marker: Str, value: Bytes}
type PreparedArguments = {text: List[Str], raw: List[RawArgument]}

# Keep raw operands for paths while giving the text option parser safe markers.
pure prepare_arguments(argv: List[Bytes]) -> PreparedArguments {
  var text: List[Str] = []
  var raw: List[RawArgument] = []

  for index in range(argv.len()) {
    let argument = argv[index]
    match argument.utf8() {
      Ok(value) => text += [value]
      Err(_) => {
        let marker = f"\0cat-raw-argument-{index}\0"
        text += [marker]
        raw += [{marker: marker, value: argument}]
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

proc report_name_error(name: Bytes, failure: Error) [process, env] {
  if let Ok(text) = name.utf8() {
    gnu.name_error(text, failure)
  } else {
    gnu.error(f"{gnu.quote_bytes(name, always: false)}: {gnu.strerror(failure)}")
  }
}

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

  var start = 0

  let pieces: List[Bytes] = collect {
    for index in range(content.len()) {
      let value = content.byte_at(index) ?? 0

      if style.special[value] {
        yield @[content[start..index], style.table[value]]
        start = index + 1
      }
    }
  }

  return content when start == 0

  bytes.concat([@pieces, content[start..]])
}

# Render the complete lines of `data` (the unfinished last line is kept in
# `pending` unless `final`). `lines()` drops a trailing CR, so the terminator
# is recovered from the bytes after each line.
pure render(data: Bytes, final: Bool, style: Style, state: State) -> Rendered {
  var position = 0
  var line = state.line
  var blank = state.blank
  var pending = b""

  let pieces: List[Bytes] = collect {
    for item in data.lines() {
      let end = position + item.len()
      let after = data.byte_at(end) ?? -1
      let crlf = after == 13 and (data.byte_at(end + 1) ?? -1) == 10
      let newline = after == 10 or crlf
      let lone_cr = if after == 13 and ! crlf { 1 } else { 0 }

      if ! newline and ! final {
        pending = data[position..]
        break
      }

      let empty = after == 10 and item.is_empty()
      let numbered = if style.nonblank { ! empty } else { style.number }

      if ! (style.squeeze and empty and blank) {
        if numbered {
          yield bytes.from_text(f"{line:>6}\t")
          line += 1
        }

        yield convert(data[position..end + lone_cr], style)

        yield style.cr when crlf

        yield if style.ends { b"$\n" } else { b"\n" } when newline
      }

      let width = if crlf { 2 } else if after == 10 or lone_cr == 1 { 1 } else { 0 }
      blank = empty
      position = end + width
    }
  }

  {out: bytes.concat(pieces), state: {pending: pending, line: line, blank: blank}}
}

# Render bytes without line numbering or blank-line squeezing as they arrive.
# Only a trailing CR needs to wait for the next chunk to distinguish CRLF.
pure render_stream(data: Bytes, style: Style, pending_cr: Bool) -> StreamRendered {
  var index = 0
  var waiting_cr = pending_cr
  let pieces: List[Bytes] = collect {
    while index < data.len() {
      let value = data.byte_at(index) ?? 0
      if waiting_cr {
        if value == 10 {
          yield style.cr
          yield if style.ends { b"$\n" } else { b"\n" }
          waiting_cr = false
          index += 1
          continue
        }
        yield convert(b"\r", style)
        waiting_cr = false
      }

      if value == 13 {
        waiting_cr = true
      } else if value == 10 {
        yield if style.ends { b"$" } else { b"" }
        yield b"\n"
      } else {
        yield convert(data[index..index + 1], style)
      }
      index += 1
    }
  }

  {out: bytes.concat(pieces), pending_cr: waiting_cr}
}

proc main(...argv: List[Bytes]) [fs, process, env, error, io] {
  let prepared = prepare_arguments(argv)
  let opts: CatOptions = cli.applet(
    tio.without_presume_pipe(prepared.text),
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
  let operands = if opts.files.is_empty() { ["-"] } else { opts.files }
  let out = tio.standard_file(1)
  var state = {pending: b"", line: 1, blank: false}
  let streaming = ! (style.number or style.nonblank or style.squeeze)
  var pending_cr = false
  var written = 0
  var failed = false

  for name in operands {
    let raw_name = argument_bytes(name, prepared.raw)

    guard let source = source_for(raw_name) else { |failure|
      report_name_error(raw_name, failure)
      failed = true
      continue
    }

    if tio.is_unsafe_overwrite(source, out, written) {
      gnu.error(f"{gnu.quote_bytes(raw_name, always: false)}: input file is output file")
      failed = true
      continue
    }

    if source.mode == "device" and streaming {
      guard let fd = unix.open_fd(source.path) else { |failure|
        report_name_error(raw_name, failure)
        failed = true
        continue
      }

      loop {
        guard let chunk = unix.read_fd(fd, tio.CHUNK) else { |failure|
          report_name_error(raw_name, failure)
          failed = true
          break
        }

        break when chunk.is_empty()
        let rendered = render_stream(chunk, style, pending_cr)
        pending_cr = rendered.pending_cr
        gnu.write_bytes(rendered.out)
        written += rendered.out.len()
      }

      unix.close_fd(fd)?
      continue
    }

    var offset = 0

    loop {
      guard let chunk = tio.read_chunk(source, offset) else { |failure|
        report_name_error(raw_name, failure)
        failed = true
        break
      }

      break when chunk.is_empty()

      offset += chunk.len()

      if plain {
        gnu.write_bytes(chunk)
        written += chunk.len()
      } else if streaming {
        let rendered = render_stream(chunk, style, pending_cr)
        pending_cr = rendered.pending_cr
        gnu.write_bytes(rendered.out)
        written += rendered.out.len()
      } else {
        let rendered = render(bytes.concat([state.pending, chunk]), false, style, state)
        state = rendered.state
        gnu.write_bytes(rendered.out)
        written += rendered.out.len()
      }
    }
  }

  if streaming and pending_cr {
    gnu.write_bytes(convert(b"\r", style))
  } else if ! plain and ! streaming {
    gnu.write_bytes(render(state.pending, true, style, {...state, pending: b""}).out)
  }

  if failed {
    exit 1
  }
}
