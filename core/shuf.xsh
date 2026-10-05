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

# The random source: the bytes read so far (a file, or blocks of /dev/urandom
# when no file is given), the next unread byte, and the leftover entropy GNU's
# `randint` recycles between draws.
type Rng = {data: Bytes, pos: Int, state: Int, entropy: Int, device: Bool}

type Draw = {value: Int, rng: Rng, ok: Bool}

# Output stops here for an unbounded `-r`: stdout is flushed only when the
# script ends, so endless output could never be delivered.
const OUTPUT_LIMIT = 33554432

# The largest range one draw supports: its entropy has to fit in an Int.
const DRAW_LIMIT = 36028797018963968

# Records of `data` separated by `sep`; one trailing separator is ignored.
pure split_records(data: Bytes, sep: Int) -> List[Bytes] {
  if let Ok(text) = data.utf8() {
    var parts = text.split(if sep == 0 { "\0" } else { "\n" })

    if parts.len() > 0 and parts[parts.len() - 1] == "" {
      parts = parts[..parts.len() - 1]
    }

    return [bytes.from_text(part) for part in parts]
  }

  var records: List[Bytes] = []
  var start = 0

  for index in range(data.len()) {
    if data.byte_at(index) == sep {
      records += [data[start..index]]
      start = index + 1
    }
  }

  if start < data.len() {
    records += [data[start..]]
  }

  records
}

# Up to AT_MOST drawn uniformly, with GNU's byte-at-a-time rejection sampling
# so a `--random-source` file gives the same numbers.
proc draw(rng: Rng, at_most: Int) [fs, error] -> Draw {
  var data = rng.data
  var pos = rng.pos
  var state = rng.state
  var entropy = rng.entropy
  var settled = false
  var value = 0

  while ! settled {
    while entropy < at_most {
      if pos >= data.len() {
        if rng.device {
          data = bytes.read_at(p"/dev/urandom", 0, 4096) ?? b""
          pos = 0
        }

        if pos >= data.len() {
          return {value: 0, rng: {...rng, data: data, pos: pos, state: state, entropy: entropy}, ok: false}
        }
      }

      state = state * 256 + (data.byte_at(pos) ?? 0)
      entropy = entropy * 256 + 255
      pos += 1
    }

    let choices = at_most + 1
    let safe = entropy - (entropy + 1) % choices

    if state <= safe {
      value = state % choices
      state = state / choices
      entropy = (entropy - at_most) / choices
      settled = true
    } else {
      state = state % choices
      entropy = entropy % choices
    }
  }

  {value: value, rng: {data: data, pos: pos, state: state, entropy: entropy, device: rng.device}, ok: true}
}

# A decimal count; values past u64 are null, values past Int clamp.
pure parse_count(text: Str) -> Int? {
  return null when ! rx"^[0-9]+$".matches(text)

  let digits = rx"^0+".replace(text, "")

  return 0 when digits == ""
  return null when digits.byte_len() > 20 or (digits.byte_len() == 20 and digits > "18446744073709551615")
  return tio.MAX_COUNT when digits.byte_len() > 18

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
    gnu.error("multiple -i options specified")
    exit 1
  }

  if opts.output.len() > 1 {
    gnu.error("multiple output files specified")
    exit 1
  }

  if opts.echo and opts.range.len() > 0 {
    gnu.error("cannot combine -e and -i options")
    exit 1
  }

  if opts.range.len() > 0 and opts.operands.len() > 0 {
    gnu.extra_operand(opts.operands[0])
  }

  if ! opts.echo and opts.range.len() == 0 and opts.operands.len() > 1 {
    gnu.extra_operand(opts.operands[1])
  }

  var head = tio.MAX_COUNT
  var counted = false

  for text in opts.count {
    let parsed = parse_count(text)

    if parsed == null {
      gnu.error(f"invalid line count: {gnu.quote(text)}")
      exit 1
    }

    if ! counted or (parsed ?? 0) < head {
      head = parsed ?? 0
    }

    counted = true
  }

  var lo = 0
  var hi = -1

  if opts.range.len() > 0 {
    let text = opts.range[0]
    let cut = text.find("-") ?? -1
    let from = if cut > 0 { parse_count(text.byte_slice(0, length: cut)) } else { null }
    let to = if cut >= 0 { parse_count(text.byte_slice(cut + 1)) } else { null }

    if from == null or to == null or (from ?? 0) > (to ?? 0) + 1 {
      gnu.error(f"invalid input range: {gnu.quote(text)}")
      exit 1
    }

    if (from ?? 0) >= tio.MAX_COUNT or (to ?? 0) >= tio.MAX_COUNT {
      gnu.error("input ranges beyond 2^63 - 2 are not supported")
      exit 1
    }

    lo = from ?? 0
    hi = to ?? 0
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

  var rng: Rng = {data: b"", pos: 0, state: 0, entropy: 0, device: true}

  if opts.source != null {
    let name = opts.source ?? ""

    guard let data = gnu.read_operand(name) else { |failure|
      gnu.name_error(name, failure)
      exit 1
    }

    rng = {data: data, pos: 0, state: 0, entropy: 0, device: false}
  }

  var items: List[Bytes] = []
  var total = 0

  if opts.range.len() > 0 {
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
  }

  var out: List[Bytes] = []
  var size = 0
  var failed = false
  var limited = false

  if total > DRAW_LIMIT {
    gnu.error("input ranges this large are not supported")
    exit 1
  }

  if opts.repeat {
    if total == 0 {
      gnu.error("no lines to repeat")
      exit 1
    }

    var made = 0

    while made < head and ! failed and ! limited {
      let got = draw(rng, total - 1)

      rng = got.rng

      if ! got.ok {
        failed = true
      } else {
        let piece = if opts.range.len() > 0 { bytes.from_text(f"{lo + got.value}") } else { items[got.value] }

        out += [piece, mark]
        size += piece.len() + 1
        made += 1

        if size > OUTPUT_LIMIT and head == tio.MAX_COUNT {
          limited = true
        }
      }
    }
  } else {
    let amount = if head < total { head } else { total }

    if opts.range.len() > 0 and amount * 16 < total {
      var moved: Map[Int, Int] = map.empty()
      var index = 0

      while index < amount and ! failed {
        let here = lo + index
        let value = moved.get(here) ?? here
        let got = draw(rng, total - index - 1)

        rng = got.rng

        if ! got.ok {
          failed = true
        } else {
          let there = here + got.value
          var shown = value

          if there != here {
            shown = moved.get(there) ?? there
            moved[there] = value
          }

          out += [bytes.from_text(f"{shown}"), mark]
          index += 1
        }
      }
    } else {
      var picks: List[Int] = []

      if opts.range.len() > 0 {
        picks = [lo + step for step in range(total)]
      }

      var index = 0

      while index < amount and ! failed {
        let got = draw(rng, total - index - 1)

        rng = got.rng

        if ! got.ok {
          failed = true
        } else {
          let other = index + got.value

          if opts.range.len() > 0 {
            let held = picks[index]

            picks[index] = picks[other]
            picks[other] = held
            out += [bytes.from_text(f"{picks[index]}"), mark]
          } else {
            let held = items[index]

            items[index] = items[other]
            items[other] = held
            out += [items[index], mark]
          }

          index += 1
        }
      }
    }
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
    gnu.error("end of random source")
    exit 1
  }

  if limited {
    gnu.error("unbounded output stopped at 32 MiB: stdout is only flushed when the script ends")
    exit 1
  }
}
