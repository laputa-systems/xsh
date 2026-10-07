#!/bin/xsh
use lib.gnu
use lib.textio_a1 as tio

const USAGE = """Usage: shuf [OPTION]... [FILE]
  or:  shuf -e [OPTION]... [ARG]...
  or:  shuf -i LO-HI [OPTION]...
Write a random permutation of the input lines to standard output.

With no FILE, or when FILE is -, read standard input.

Mandatory arguments to long options are mandatory for short options too.
  -e, --echo                treat each ARG as an input line
  -i, --input-range=LO-HI   treat each number LO through HI as an input line
  -n, --head-count=COUNT    output at most COUNT lines
  -o, --output=FILE         write result to FILE instead of standard output
      --random-source=FILE  get random bytes from FILE
  -r, --repeat              output lines can be repeated
  -z, --zero-terminated     line delimiter is NUL, not newline
      --help        display this help and exit
      --version     output version information and exit
"""

type ShufOptions = {
  echo: Bool,
  range: List[Str],
  count: List[Str],
  output: List[Str],
  source: Str?,
  repeat: Bool,
  zero: Bool,
  help: Bool,
  version: Bool,
  operands: List[Str],
}

# The result of one draw: the value (when `ok`), and the position in the byte
# window and the leftover entropy GNU's `randint` recycles between draws. A
# draw that runs out of window bytes is not `ok` and is resumed after a refill.
type Draw = {value: Int, pos: Int, state: Int, entropy: Int, ok: Bool}

# Output stops here for an unbounded `-r`: stdout is flushed only when the
# script ends, so endless output could never be delivered.
const OUTPUT_LIMIT = 262144

# The largest range one draw supports: its entropy has to fit in an Int.
const DRAW_LIMIT = 36028797018963968

# Records of `data` separated by `sep`; one trailing separator is ignored.
pure split_records(data: Bytes, sep: Int) -> List[Bytes] {
  if let Ok(text) = data.utf8() {
    var parts = text.split(if sep == 0 { "\0" } else { "\n" })

    if ! parts.is_empty() and parts[-1] == "" {
      parts = parts[..parts.len() - 1]
    }

    return [bytes.from_text(part) for part in parts]
  }

  var start = 0

  let records: List[Bytes] = collect {
    for index in range(data.len()) {
      if data.byte_at(index) == sep {
        yield data[start..index]
        start = index + 1
      }
    }

    yield data[start..] when start < data.len()
  }

  records
}

# Up to AT_MOST drawn uniformly from the bytes of WINDOW, with GNU's
# byte-at-a-time rejection sampling so a `--random-source` file gives the same
# numbers.
pure draw(window: Bytes, at: Int, carried: Int, kept: Int, at_most: Int) -> Draw {
  var pos = at
  var state = carried
  var entropy = kept

  while true {
    while entropy < at_most {
      if pos >= window.len() {
        return {value: 0, pos: pos, state: state, entropy: entropy, ok: false}
      }

      state = state * 256 + (window.byte_at(pos) ?? 0)
      entropy = entropy * 256 + 255
      pos += 1
    }

    let choices = at_most + 1
    let safe = entropy - (entropy + 1) % choices

    if state <= safe {
      return {
        value: state % choices,
        pos: pos,
        state: state / choices,
        entropy: (entropy - at_most) / choices,
        ok: true,
      }
    }

    state = state % choices
    entropy = entropy % choices
  }

  {value: 0, pos: pos, state: state, entropy: entropy, ok: false}
}

# A decimal count; values past u64 are null, values past Int clamp.
pure parse_count(text: Str) -> Int? {
  return null when ! rx"^[0-9]+$".matches(text)

  let digits = rx"^0+".replace(text, with: "")

  return 0 when digits == ""
  return null when digits.byte_len() > 20 or (digits.byte_len() == 20 and digits > "18446744073709551615")
  return tio.MAX_COUNT when digits.byte_len() > 19 or (digits.byte_len() == 19 and digits >= "9223372036854775807")

  digits.parse_int() ?? 0
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: ShufOptions = cli.applet(
    argv,
    {
      gnu: {status: 1, unsupported: {"--random-seed": "a uutils extension; use --random-source=FILE"}},
      echo: {form: "-e --echo", default: false},
      range: {form: "-i --input-range LO-HI", repeated: true},
      count: {form: "-n --head-count COUNT", repeated: true},
      output: {form: "-o --output FILE", repeated: true},
      source: {form: "--random-source FILE"},
      repeat: {form: "-r --repeat", default: false},
      zero: {form: "-z --zero-terminated", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      operands: {form: "...ARG"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("shuf")
    return
  }

  if opts.range.len() > 1 {
    gnu.error("the argument '--input-range <LO-HI>' cannot be used multiple times")
    exit 1
  }

  if opts.output.len() > 1 {
    gnu.error("the argument '--output <FILE>' cannot be used multiple times")
    exit 1
  }

  if opts.echo and ! opts.range.is_empty() {
    gnu.error("the argument '-e' cannot be used with '--input-range <LO-HI>'")
    exit 1
  }

  if ! opts.range.is_empty() and ! opts.operands.is_empty() {
    gnu.error(f"the argument {gnu.quote(opts.operands[0])} cannot be used with '--input-range <LO-HI>'")
    exit 1
  }

  if ! opts.echo and opts.range.is_empty() and opts.operands.len() > 1 {
    gnu.error(f"unexpected argument {gnu.quote(opts.operands[1])} found")
    exit 1
  }

  var head = tio.MAX_COUNT
  var counted = false

  for text in opts.count {
    let parsed = parse_count(text)

    if parsed == null {
      gnu.error(f"invalid value {gnu.quote(text)} for '--head-count <COUNT>': invalid digit found in string")
      exit 1
    }

    if ! counted or parsed < head {
      head = parsed
    }

    counted = true
  }

  var lo = 0
  var hi = -1

  if ! opts.range.is_empty() {
    let text = opts.range[0]
    let cut = text.find("-") ?? -1

    if cut < 0 {
      gnu.error(f"invalid value {gnu.quote(text)} for '--input-range <LO-HI>': missing '-'")
      exit 1
    }

    let from = if cut > 0 { parse_count(text.byte_slice(0, length: cut)) } else { null }
    let to = parse_count(text.byte_slice(cut + 1))

    if from == null or to == null {
      gnu.error(f"invalid value {gnu.quote(text)} for '--input-range <LO-HI>': invalid digit found in string")
      exit 1
    }

    if from - 1 > to {
      gnu.error(f"invalid value {gnu.quote(text)} for '--input-range <LO-HI>': start exceeds end")
      exit 1
    }

    if from >= tio.MAX_COUNT or to >= tio.MAX_COUNT {
      if ! opts.repeat and head == tio.MAX_COUNT {
        gnu.error("memory exhausted")
      } else {
        gnu.error("input ranges beyond 2^63 - 2 are not supported")
      }

      exit 1
    }

    lo = from
    hi = to
  }

  let sep = if opts.zero { 0 } else { 10 }
  let mark = bytes.from_ints([sep])?
  let output = opts.output.get(0) ?? "-"

  if head == 0 {
    if output != "-" {
      if let Err(failure) = fp"{output}".write(b"") {
        gnu.error(f"failed to open {gnu.quote(output)} for writing: {gnu.strerror(failure)}")
        exit 1
      }
    }

    return
  }

  var source = b""
  var device = true

  if opts.source != null {
    let name = opts.source
    let kind = if let Ok(found) = fs.stat(fp"{name}", follow_symlinks: true) { found.kind } else { "missing" }

    if kind == "file" or kind == "missing" or name == "-" {
      guard let data = gnu.read_operand(name) else { |failure|
        gnu.name_error(name, failure)
        exit 1
      }

      source = data
      device = false
    }
  }

  let device_path = if opts.source != null { fp"{opts.source}" } else { /dev/urandom }

  var items: List[Bytes] = []
  var total = 0
  var later: List[Bytes] = []
  var reservoir = false

  if ! opts.range.is_empty() {
    total = hi - lo + 1
  } else if opts.echo {
    items = [bytes.from_text(item) for item in opts.operands]
    total = items.len()
  } else {
    let name = opts.operands.get(0) ?? "-"

    guard let data = gnu.read_operand(name) else { |failure|
      gnu.name_error(name, failure)
      exit 1
    }

    items = split_records(data, sep)
    total = items.len()

    # With a line limit GNU reads anything but a regular file through a
    # reservoir of that many lines: each later line draws a slot and replaces
    # its line when the slot is inside the reservoir (and a last draw is spent
    # at the end), then the survivors are permuted as usual.
    let place = if name == "-" { /dev/stdin } else { fp"{name}" }
    let regular = if let Ok(found) = fs.stat(place, follow_symlinks: true) { found.kind == "file" } else { false }

    if ! opts.repeat and ! regular and head < tio.MAX_COUNT and total >= head {
      reservoir = true
      later = items[head..]
      items = items[..head]
      total = head
    }
  }

  if total > DRAW_LIMIT {
    if ! opts.repeat and head >= total {
      gnu.error("memory exhausted")
    } else {
      gnu.error("input ranges this large are not supported")
    }

    exit 1
  }

  if opts.repeat and total == 0 {
    gnu.error("no lines to repeat")
    exit 1
  }

  # Each step is one draw: repeat mode picks any line, sparse mode shuffles a
  # long input range through a map of the swapped positions, dense mode
  # shuffles the items (or the range) in place.
  let amount = if opts.repeat { head } else if head < total { head } else { total }
  let ranged = ! opts.range.is_empty()
  let sparse = ranged and ! opts.repeat and amount * 16 < total
  var moved: Map[Int, Int] = {}
  var picks: List[Int] = []

  if ranged and ! opts.repeat and ! sparse {
    picks = [lo + step for step in range(total)]
  }

  var size = 0
  var failed = false
  var limited = false
  var window = b""
  var spot = 0
  var state = 0
  var entropy = 0
  var offset = 0
  var index = 0

  var sampled = 0

  let out: List[Bytes] = collect {
    while (sampled < later.len() + (if reservoir { 1 } else { 0 }) or index < amount) and ! failed and ! limited {
      let sampling = reservoir and sampled < later.len() + 1
      let at_most = if sampling { head + sampled } else if opts.repeat { total - 1 } else { total - index - 1 }
      var value = 0
      var settled = false

      while ! settled and ! failed {
        let got = draw(window, spot, state, entropy, at_most)

        state = got.state
        entropy = got.entropy
        spot = got.pos

        if got.ok {
          value = got.value
          settled = true
        } else {
          if device {
            window = bytes.read_at(device_path, 0, 4096) ?? b""
          } else {
            let stop = if offset + 256 < source.len() { offset + 256 } else { source.len() }

            window = source[offset..stop]
            offset = stop
          }

          spot = 0

          if window.is_empty() {
            failed = true
          }
        }
      }

      break when failed

      if sampling {
        if sampled < later.len() and value < head {
          items[value] = later[sampled]
        }

        sampled += 1
        continue
      }

      if opts.repeat {
        let piece = if ranged { bytes.from_text(f"{lo + value}") } else { items[value] }

        yield @[piece, mark]
        size += piece.len() + 1

        if size > OUTPUT_LIMIT and head == tio.MAX_COUNT {
          limited = true
        }
      } else if sparse {
        let here = lo + index
        let held = moved.get(here) ?? here
        let there = here + value
        var shown = held

        if there != here {
          shown = moved.get(there) ?? there
          moved[there] = held
        }

        yield @[bytes.from_text(f"{shown}"), mark]
      } else if ranged {
        let other = index + value
        let held = picks[index]

        picks[index] = picks[other]
        picks[other] = held
        yield @[bytes.from_text(f"{picks[index]}"), mark]
      } else {
        let other = index + value
        let held = items[index]

        items[index] = items[other]
        items[other] = held
        yield @[items[index], mark]
      }

      index += 1
    }
  }

  # A permutation is written only once complete; repeated output goes out as
  # it is drawn.
  if failed and ! opts.repeat {
    gnu.error(f"{gnu.quote(opts.source ?? "")}: end of file")
    exit 1
  }

  if output == "-" {
    gnu.write_bytes(bytes.concat(out))
  } else if let Err(failure) = fp"{output}".write(bytes.concat(out)) {
    if gnu.errno(failure) == 28 {
      gnu.error(f"write error: {gnu.strerror(failure)}")
    } else {
      gnu.error(f"failed to open {gnu.quote(output)} for writing: {gnu.strerror(failure)}")
    }

    exit 1
  }

  if failed {
    gnu.error(f"{gnu.quote(opts.source ?? "")}: end of file")
    exit 1
  }

  if limited {
    gnu.error("unbounded output stopped at 256 KiB: stdout is only flushed when the script ends")
    exit 1
  }
}
