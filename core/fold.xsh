#!/bin/xsh
use lib.gnu

const USAGE = """
Usage: fold [OPTION]... [FILE]...
Wrap each input line to fit in specified width.
  -b, --bytes          count bytes rather than columns
  -c, --characters     count characters rather than columns
  -s, --spaces         break at spaces
  -w, --width=WIDTH    use WIDTH columns instead of 80
      --help           display this help and exit
      --version        output version information and exit
"""

type FoldOptions = {bytes: Bool, characters: Bool, spaces: Bool, width: Str, help: Bool, version: Bool, files: List[Str]}
type FoldState = {pending: List[Bytes], column: Int, last_blank: Int, carry: Bytes}
type FoldStep = {state: FoldState, out: Bytes}

const INPUT_CHUNK = 4096
const FOLD_STEP = 128

pure utf8_width(data: Bytes, at: Int) -> Int {
  let lead = data.byte_at(at) ?? 0
  let size = if lead >= 194 and lead <= 223 { 2 } else if lead >= 224 and lead <= 239 { 3 } else if lead >= 240 and lead <= 244 { 4 } else { 1 }
  if size > 1 and at + size <= data.len() and (data[at..at + size].utf8() ?? "") != "" { size } else { 1 }
}

pure display_width(unit: Bytes) -> Int {
  return 0 when unit.len() == 1 and (unit.byte_at(0) ?? 0) == 0
  let text = unit.utf8() ?? ""
  return 1 when text == ""
  return 0 when rx"[\x{300}-\x{36f}\x{483}-\x{489}\x{591}-\x{5bd}\x{610}-\x{61a}\x{64b}-\x{65f}\x{670}\x{200b}-\x{200f}\x{20d0}-\x{20f0}\x{fe00}-\x{fe0f}\x{fe20}-\x{fe2f}\x{1ab0}-\x{1aff}\x{1dc0}-\x{1dff}]".matches(text)
  return 2 when rx"[\x{1100}-\x{115f}\x{231a}-\x{231b}\x{2329}-\x{232a}\x{2e80}-\x{a4cf}\x{ac00}-\x{d7a3}\x{f900}-\x{faff}\x{fe10}-\x{fe6f}\x{ff01}-\x{ff60}\x{1f300}-\x{1faff}\x{20000}-\x{3fffd}]".matches(text)
  1
}

pure advance(unit: Bytes, column: Int, byte_mode: Bool, char_mode: Bool) -> Int {
  let byte = unit.byte_at(0) ?? 0
  if byte_mode { return column + 1 }
  if byte == 9 { return column - column % 8 + 8 }
  if byte == 8 { return if column > 0 { column - 1 } else { 0 } }
  if byte == 13 { return 0 }
  column + (if char_mode { 1 } else { display_width(unit) })
}

pure columns(units: List[Bytes], byte_mode: Bool, char_mode: Bool) -> Int {
  var column = 0
  for unit in units { column = advance(unit, column, byte_mode, char_mode) }
  column
}

pure is_break(unit: Bytes) -> Bool {
  let byte = unit.byte_at(0) ?? 0
  byte == 9 or byte == 32
}

pure fold_line(line: Bytes, width: Int, byte_mode: Bool, char_mode: Bool, spaces: Bool) -> Bytes {
  var units: List[Bytes] = []
  var at = 0
  while at < line.len() {
    let size = if byte_mode { 1 } else { utf8_width(line, at) }
    units += [line[at..at + size]]
    at += size
  }

  var out: List[Bytes] = []
  var pending: List[Bytes] = []
  var column = 0
  var last_blank = -1
  var index = 0
  while index < units.len() {
    let unit = units[index]
    let next_column = advance(unit, column, byte_mode, char_mode)
    if next_column > width and pending.len() > 0 {
      if spaces and last_blank >= 0 {
        let split = last_blank + 1
        out += [bytes.concat(pending[..split]), b"\n"]
        pending = pending[split..]
        column = columns(pending, byte_mode, char_mode)
        last_blank = -1
        for pos in range(pending.len()) { if is_break(pending[pos]) { last_blank = pos } }
      } else {
        out += [bytes.concat(pending), b"\n"]
        pending = []
        column = 0
        last_blank = -1
      }
      continue
    }
    pending += [unit]
    column = next_column
    if is_break(unit) { last_blank = pending.len() - 1 }
    index += 1
  }
  out += [bytes.concat(pending)]
  bytes.concat(out)
}

pure utf8_prefix_is_incomplete(data: Bytes, at: Int, size: Int) -> Bool {
  return false when at + size <= data.len()
  let lead = data.byte_at(at) ?? 0
  var index = 1
  while at + index < data.len() {
    let byte = data.byte_at(at + index) ?? 0
    let valid = if index == 1 {
      (lead == 224 and byte >= 160 and byte <= 191)
        or (lead == 237 and byte >= 128 and byte <= 159)
        or (lead == 240 and byte >= 144 and byte <= 191)
        or (lead == 244 and byte >= 128 and byte <= 143)
        or (lead != 224 and lead != 237 and lead != 240 and lead != 244 and byte >= 128 and byte <= 191)
    } else { byte >= 128 and byte <= 191 }
    return false when ! valid
    index += 1
  }
  true
}

pure incomplete_utf8_suffix(data: Bytes) -> Int {
  let start = if data.len() > 3 { data.len() - 3 } else { 0 }
  for at in range(start, data.len()) {
    let lead = data.byte_at(at) ?? 0
    let size = if lead >= 194 and lead <= 223 { 2 } else if lead >= 224 and lead <= 239 { 3 } else if lead >= 240 and lead <= 244 { 4 } else { 1 }
    if size > 1 and utf8_prefix_is_incomplete(data, at, size) {
      return data.len() - at
    }
  }
  0
}

pure fold_chunk(data: Bytes, state: FoldState, width: Int, byte_mode: Bool, char_mode: Bool, spaces: Bool, finish: Bool) -> FoldStep {
  let combined = bytes.concat([state.carry, data])
  let carry_size = if finish { 0 } else { incomplete_utf8_suffix(combined) }
  let end = combined.len() - carry_size
  var pending = state.pending
  var column = state.column
  var last_blank = state.last_blank
  var out: List[Bytes] = []
  var at = 0
  while at < end {
    let byte = combined.byte_at(at) ?? -1
    if byte == 10 {
      out += [bytes.concat(pending), b"\n"]
      pending = []
      column = 0
      last_blank = -1
      at += 1
      continue
    }

    let size = if byte_mode { 1 } else { utf8_width(combined, at) }
    let unit = combined[at..at + size]
    let next_column = advance(unit, column, byte_mode, char_mode)
    if next_column > width and pending.len() > 0 {
      if spaces and last_blank >= 0 {
        let split = last_blank + 1
        out += [bytes.concat(pending[..split]), b"\n"]
        pending = pending[split..]
        column = columns(pending, byte_mode, char_mode)
        last_blank = -1
        for pos in range(pending.len()) { if is_break(pending[pos]) { last_blank = pos } }
      } else {
        out += [bytes.concat(pending), b"\n"]
        pending = []
        column = 0
        last_blank = -1
      }
      continue
    }

    pending += [unit]
    column = next_column
    if is_break(unit) { last_blank = pending.len() - 1 }
    at += size
  }

  let carry = if finish { b"" } else { combined[end..] }
  if finish {
    if pending.len() > 0 { out += [bytes.concat(pending)] }
    pending = []
    column = 0
    last_blank = -1
  }
  {state: {pending: pending, column: column, last_blank: last_blank, carry: carry}, out: bytes.concat(out)}
}

pure fold_data(data: Bytes, width: Int, byte_mode: Bool, char_mode: Bool, spaces: Bool) -> Bytes {
  let state: FoldState = {pending: [], column: 0, last_blank: -1, carry: b""}
  fold_chunk(data, state, width, byte_mode, char_mode, spaces, true).out
}

pure normalized_args(argv: List[Str]) -> List[Str] {
  var out: List[Str] = []
  var stopped = false
  var wants_width = false
  for arg in argv {
    if ! stopped and arg == "--" { stopped = true; out += [arg] } else if ! stopped and wants_width { out += [arg]; wants_width = false } else if ! stopped and arg in ["-w", "--width"] { out += [arg]; wants_width = true } else if ! stopped and rx"^-[bcs]*w$".matches(arg) { out += [arg]; wants_width = true } else if ! stopped and rx"^-[0-9]+$".matches(arg) { out += [f"-w{arg[1..]}"] } else { out += [arg] }
  }
  out
}

pure raw_files(argv: List[Str], raw: List[Bytes]) -> List[Bytes] {
  var files: List[Bytes] = []
  var stopped = false
  var index = 0
  while index < argv.len() {
    let arg = argv[index]
    if ! stopped and arg == "--" { stopped = true; index += 1 } else if ! stopped and (arg == "-w" or arg == "--width") { index += 2 } else if ! stopped and (arg == "-b" or arg == "-c" or arg == "-s" or arg == "--bytes" or arg == "--characters" or arg == "--spaces" or arg.starts_with("-w") or arg.starts_with("--width=") or arg.starts_with("-b") or arg.starts_with("-c") or arg.starts_with("-s")) { index += 1 } else if ! stopped and rx"^-[0-9]+$".matches(arg) { index += 1 } else if ! stopped and arg.starts_with("-") and arg != "-" { index += 1 } else { files += [raw[index]]; index += 1 }
  }
  files
}

proc fold_path(input_path: Path, width: Int, byte_mode: Bool, char_mode: Bool, spaces: Bool) [fs, process, env, io, error] -> Result[Unit, Error] {
  let chunks = input_path.chunks(INPUT_CHUNK)?
  var state: FoldState = {pending: [], column: 0, last_blank: -1, carry: b""}
  for chunk in chunks {
    state = fold_feed(chunk, state, width, byte_mode, char_mode, spaces)
  }
  let final = fold_chunk(b"", state, width, byte_mode, char_mode, spaces, true)
  if final.out.len() > 0 { gnu.write_bytes(final.out) }
  Ok()
}

proc fold_feed(data: Bytes, initial: FoldState, width: Int, byte_mode: Bool, char_mode: Bool, spaces: Bool) [process, env, io] -> FoldState {
  var state = initial
  var at = 0
  while at < data.len() {
    let end = if data.len() - at > FOLD_STEP { at + FOLD_STEP } else { data.len() }
    let step = fold_chunk(data[at..end], state, width, byte_mode, char_mode, spaces, false)
    state = step.state
    if step.out.len() > 0 { gnu.write_bytes(step.out) }
    at = end
  }
  state
}

proc fold_stdin(width: Int, byte_mode: Bool, char_mode: Bool, spaces: Bool) [process, env, io, error] {
  var state: FoldState = {pending: [], column: 0, last_blank: -1, carry: b""}
  loop {
    guard let next = io.stdin_read(INPUT_CHUNK) else { |failure|
      gnu.error(f"read error: {gnu.strerror(failure)}")
      exit 1
    }
    break when next == null
    let chunk = next ?? b""
    state = fold_feed(chunk, state, width, byte_mode, char_mode, spaces)
  }
  let final = fold_chunk(b"", state, width, byte_mode, char_mode, spaces, true)
  if final.out.len() > 0 { gnu.write_bytes(final.out) }
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: FoldOptions = cli.applet(
    normalized_args(argv),
    {
      gnu: {status: 1},
      bytes: {form: "-b --bytes", default: false},
      characters: {form: "-c --characters", default: false},
      spaces: {form: "-s --spaces", default: false},
      width: {form: "-w --width WIDTH", default: "80"},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      files: {form: "...FILE"},
    },
  )?
  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("fold"); return }
  let parsed_width = match opts.width.parse_int() {
    Ok(value) => value
    Err(_) => {
      let suffix = if rx"^[0-9]+$".matches(opts.width) { ": Numerical result out of range" } else { "" }
      gnu.error(f"invalid number of columns: {gnu.quote_value(opts.width)}{suffix}")
      exit 1
    }
  }
  if parsed_width <= 0 {
    let suffix = if parsed_width == 0 { ": Numerical result out of range" } else { "" }
    gnu.error(f"invalid number of columns: {gnu.quote_value(opts.width)}{suffix}")
    exit 1
  }
  let names = raw_files(argv, cli.argv_bytes())
  var failed = false
  for name in if names.len() == 0 { [b"-"] } else { names } {
    if name == b"-" {
      fold_stdin(parsed_width, opts.bytes, opts.characters, opts.spaces)
    } else {
      guard let input_path = Path.parse_bytes(name) else { |failure|
        gnu.error(f"{gnu.quote_bytes(name, always: false)}: {gnu.strerror(failure)}")
        failed = true
        continue
      }
      if let Err(failure) = fold_path(input_path, parsed_width, opts.bytes, opts.characters, opts.spaces) {
        gnu.error(f"{gnu.quote_bytes(name, always: false)}: {gnu.strerror(failure)}")
        failed = true
      }
    }
  }
  if failed { exit 1 }
}
