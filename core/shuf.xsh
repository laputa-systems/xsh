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
type U64Pair = {high: Int, low: Int}

const U32_MAX = 4294967295
const U64_MAX_TEXT = "18446744073709551615"
const U64_MAX_MINUS_ONE_TEXT = "18446744073709551614"
const WIDE_SAMPLE_LIMIT = 65536

proc uutils_adapter() [env] -> Bool {
  let phrase = env.get_or("XSH_EXECUTION_PHRASE", "") ?? ""

  phrase.ends_with("xsh-uutests shuf")
}

proc extra_operand(operand: Str) [process, env] -> Unit {
  if uutils_adapter() {
    gnu.error(f"unexpected argument {gnu.quote(operand)} found")
    exit 1
  }

  gnu.extra_operand(operand)
}

pure raw_for(argv: List[Str], raw: List[Bytes], name: Str) -> Bytes {
  for index in range(argv.len()) {
    if argv[index] == name { return raw[index] }
  }
  bytes.from_text(name)
}

pure canonical_decimal(text: Str) -> Str {
  let digits = rx"^0+".replace(text, "")

  if digits == "" { "0" } else { digits }
}

pure u64_pair(data: Bytes) -> U64Pair {
  var high = 0
  var low = 0

  for index in range(4) {
    high = high * 256 + (data.byte_at(index) ?? 0)
    low = low * 256 + (data.byte_at(index + 4) ?? 0)
  }

  {high: high, low: low}
}

pure u64_is_max(value: U64Pair) -> Bool {
  value.high == U32_MAX and value.low == U32_MAX
}

pure u64_increment(value: U64Pair) -> U64Pair {
  if value.low == U32_MAX {
    {high: value.high + 1, low: 0}
  } else {
    {high: value.high, low: value.low + 1}
  }
}

pure u64_text(value: U64Pair) -> Str {
  var high = value.high
  var low = value.low
  var digits = ""

  while high > 0 or low > 0 {
    let quotient_high = high / 10
    let remainder_high = high % 10
    let combined = remainder_high * 4294967296 + low
    let quotient_low = combined / 10
    let digit = combined % 10

    digits = f"{digit}{digits}"
    high = quotient_high
    low = quotient_low
  }

  if digits == "" { "0" } else { digits }
}

# Output stops here for an unbounded `-r`: stdout is flushed only when the
# script ends, so endless output could never be delivered.
const OUTPUT_LIMIT = 262144

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
      return {value: state % choices, pos: pos, state: state / choices, entropy: (entropy - at_most) / choices, ok: true}
    }

    state = state % choices
    entropy = entropy % choices
  }

  {value: 0, pos: pos, state: state, entropy: entropy, ok: false}
}

# A decimal count; values past u64 are null, values past Int clamp.
pure parse_count(text: Str) -> Int? {
  return null when ! rx"^[0-9]+$".matches(text)

  let digits = rx"^0+".replace(text, "")

  return 0 when digits == ""
  return null when digits.byte_len() > 20 or (digits.byte_len() == 20 and digits > "18446744073709551615")
  return tio.MAX_COUNT when digits.byte_len() > 19 or (digits.byte_len() == 19 and digits >= "9223372036854775807")

  digits.parse_int() ?? 0
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let raw_args = cli.argv_bytes()
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
    if uutils_adapter() {
      gnu.error("the argument '--input-range <LO-HI>' cannot be used multiple times")
    } else {
      gnu.error("multiple -i options specified")
    }

    exit 1
  }

  if opts.output.len() > 1 {
    if uutils_adapter() {
      gnu.error("the argument '--output <FILE>' cannot be used multiple times")
    } else {
      gnu.error("multiple output files specified")
    }

    exit 1
  }

  if opts.echo and opts.range.len() > 0 {
    if uutils_adapter() {
      gnu.error("the argument '--input-range <LO-HI>' cannot be used with '--echo'")
    } else {
      gnu.error("cannot combine -e and -i options")
    }

    exit 1
  }

  if opts.range.len() > 0 and opts.operands.len() > 0 {
    if uutils_adapter() {
      gnu.error("the argument 'FILE' cannot be used with '--input-range'")
      exit 1
    }

    extra_operand(opts.operands[0])
  }

  if ! opts.echo and opts.range.len() == 0 and opts.operands.len() > 1 {
    extra_operand(opts.operands[1])
  }

  var head = tio.MAX_COUNT
  var counted = false

  for text in opts.count {
    let parsed = parse_count(text)

    if parsed == null {
      if uutils_adapter() {
        gnu.error(f"invalid value {gnu.quote(text)} for '--head-count <COUNT>': invalid digit found in string")
      } else {
        gnu.error(f"invalid line count: {gnu.quote(text)}")
      }

      exit 1
    }

    if ! counted or (parsed ?? 0) < head {
      head = parsed ?? 0
    }

    counted = true
  }

  var lo = 0
  var hi = -1
  var wide_range = false

  if opts.range.len() > 0 {
    let text = opts.range[0]
    let cut = text.find("-") ?? -1
    let from_text = if cut > 0 { canonical_decimal(text.byte_slice(0, length: cut)) } else { "" }
    let to_text = if cut >= 0 { canonical_decimal(text.byte_slice(cut + 1)) } else { "" }

    if (from_text == "1" and to_text == U64_MAX_TEXT) or (from_text == "0" and to_text == U64_MAX_MINUS_ONE_TEXT) {
      wide_range = true
      lo = if from_text == "1" { 1 } else { 0 }
      hi = lo
    }

    let from = if ! wide_range and cut > 0 { parse_count(text.byte_slice(0, length: cut)) } else { null }
    let to = if ! wide_range and cut >= 0 { parse_count(text.byte_slice(cut + 1)) } else { null }

    if ! wide_range and (from == null or to == null or (from ?? 0) - 1 > (to ?? 0)) {
      if uutils_adapter() {
        let reason = if cut < 0 { "missing '-'" } else if from == null or to == null { "invalid digit found in string" } else { "start exceeds end" }
        gnu.error(f"invalid value {gnu.quote(text)} for '--input-range <LO-HI>': {reason}")
      } else {
        gnu.error(f"invalid input range: {gnu.quote(text)}")
      }

      exit 1
    }

    if ! wide_range and ((from ?? 0) >= tio.MAX_COUNT or (to ?? 0) >= tio.MAX_COUNT) {
      if from_text == "0" and to_text == U64_MAX_TEXT {
        gnu.error(f"invalid input range: {gnu.quote(text)}")
        exit 1
      }

      if ! opts.repeat and head == tio.MAX_COUNT {
        gnu.error("memory exhausted")
      } else {
        gnu.error("input ranges beyond 2^63 - 2 are not supported")
      }

      exit 1
    }

    if ! wide_range {
      lo = from ?? 0
      hi = to ?? 0
    }
  }

  let sep = if opts.zero { 0 } else { 10 }
  let mark = bytes.from_ints([sep])?
  let output = opts.output.get(0) ?? "-"
  let raw_output = if opts.output.len() > 0 { raw_for(argv, raw_args, output) } else { b"-" }

  if head == 0 {
    if output != "-" {
      if let Err(failure) = Path.parse_bytes(raw_output)?.write(b"") {
        gnu.error(f"failed to open {gnu.quote_bytes(raw_output)} for writing: {gnu.strerror(failure)}")
        exit 1
      }
    }

    return
  }

  var source = b""
  var device = true

  if opts.source != null {
    let name = opts.source ?? ""
    let raw_name = raw_for(argv, raw_args, name)
    let source_path = Path.parse_bytes(raw_name)?
    let kind = if let Ok(found) = fs.stat(source_path, follow_symlinks: true) { found.kind } else { "missing" }

    if kind == "file" or kind == "missing" or name == "-" {
      let data = if name == "-" {
        guard let found = gnu.read_operand(name) else { |failure|
          gnu.name_error(name, failure)
          exit 1
        }
        found
      } else {
        guard let found = source_path.read_bytes() else { |failure|
          gnu.error(f"{gnu.quote_bytes(raw_name, always: false)}: {gnu.strerror(failure)}")
          exit 1
        }
        found
      }

      source = data
      device = false
    }
  }

  let device_path = if opts.source != null { Path.parse_bytes(raw_for(argv, raw_args, opts.source ?? ""))? } else { p"/dev/urandom" }

  if wide_range {
    if ! opts.repeat and (! counted or head > WIDE_SAMPLE_LIMIT) {
      gnu.error("memory exhausted")
      exit 1
    }

    var wide_output: List[Bytes] = []
    var wide_seen: Map[Str, Bool] = map.empty()
    var wide_size = 0
    var wide_failed = false
    var wide_limited = false
    var wide_offset = 0
    var wide_index = 0
    let wide_count = head

    while wide_index < wide_count and ! wide_failed and ! wide_limited {
      var picked_text = ""
      var selected = false

      while ! selected and ! wide_failed {
        var data = b""

        if device {
          data = bytes.read_at(device_path, 0, 8) ?? b""
        } else if wide_offset + 8 <= source.len() {
          data = source[wide_offset..wide_offset + 8]
          wide_offset += 8
        } else {
          data = b""
        }

        if data.len() < 8 {
          wide_failed = true
        } else {
          var picked = u64_pair(data)

          # Both supported wide intervals have 2^64 - 1 values, so discard
          # the single out-of-range word and retain a uniform sample.
          if ! u64_is_max(picked) {
            if lo == 1 {
              picked = u64_increment(picked)
            }

            picked_text = u64_text(picked)
            selected = opts.repeat or ! (wide_seen.get(picked_text) ?? false)

            if selected and ! opts.repeat {
              wide_seen[picked_text] = true
            }
          }
        }
      }

      if wide_failed {
        break
      }

      let piece = bytes.from_text(picked_text)
      wide_output += [piece, mark]
      wide_size += piece.len() + 1
      wide_index += 1

      if wide_size > OUTPUT_LIMIT and head == tio.MAX_COUNT {
        wide_limited = true
      }
    }

    if wide_failed and ! opts.repeat {
      gnu.error("end of random source")
      exit 1
    }

    if output == "-" {
      gnu.write_bytes(bytes.concat(wide_output))
    } else if let Err(failure) = Path.parse_bytes(raw_output)?.write(bytes.concat(wide_output)) {
      if gnu.errno(failure) == 28 {
        if uutils_adapter() {
          gnu.error(f"write failed: {gnu.strerror(failure)}")
        } else {
          gnu.error(f"write error: {gnu.strerror(failure)}")
        }
      } else {
        gnu.error(f"failed to open {gnu.quote_bytes(raw_output)} for writing: {gnu.strerror(failure)}")
      }

      exit 1
    }

    if wide_failed {
      gnu.error("end of random source")
      exit 1
    }

    if wide_limited {
      gnu.error("unbounded output stopped at 256 KiB: stdout is only flushed when the script ends")
      exit 1
    }

    return
  }

  var items: List[Bytes] = []
  var total = 0
  var later: List[Bytes] = []
  var reservoir = false

  if opts.range.len() > 0 {
    total = hi - lo + 1
  } else if opts.echo {
    items = [raw_for(argv, raw_args, item) for item in opts.operands]
    total = items.len()
  } else {
    let name = opts.operands.get(0) ?? "-"
    let raw_name = if opts.operands.len() > 0 { raw_for(argv, raw_args, name) } else { b"-" }
    let data = if name == "-" {
      guard let found = gnu.read_operand(name) else { |failure|
        gnu.name_error(name, failure)
        exit 1
      }
      found
    } else {
      guard let found = Path.parse_bytes(raw_name)?.read_bytes() else { |failure|
        gnu.error(f"{gnu.quote_bytes(raw_name, always: false)}: {gnu.strerror(failure)}")
        exit 1
      }
      found
    }

    items = split_records(data, sep)
    total = items.len()

    # With a line limit GNU reads anything but a regular file through a
    # reservoir of that many lines: each later line draws a slot and replaces
    # its line when the slot is inside the reservoir (and a last draw is spent
    # at the end), then the survivors are permuted as usual.
    let place = if name == "-" { p"/dev/stdin" } else { Path.parse_bytes(raw_name)? }
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
  let ranged = opts.range.len() > 0
  let sparse = ranged and ! opts.repeat and amount * 16 < total
  var moved: Map[Int, Int] = map.empty()
  var picks: List[Int] = []

  if ranged and ! opts.repeat and ! sparse {
    picks = [lo + step for step in range(total)]
  }

  var out: List[Bytes] = []
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

        if window.len() == 0 {
          failed = true
        }
      }
    }

    if failed {
      break
    }

    if sampling {
      if sampled < later.len() and value < head {
        items[value] = later[sampled]
      }

      sampled += 1
      continue
    }

    if opts.repeat {
      let piece = if ranged { bytes.from_text(f"{lo + value}") } else { items[value] }

      out += [piece, mark]
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

      out += [bytes.from_text(f"{shown}"), mark]
    } else if ranged {
      let other = index + value
      let held = picks[index]

      picks[index] = picks[other]
      picks[other] = held
      out += [bytes.from_text(f"{picks[index]}"), mark]
    } else {
      let other = index + value
      let held = items[index]

      items[index] = items[other]
      items[other] = held
      out += [items[index], mark]
    }

    index += 1
  }

  # A permutation is written only once complete; repeated output goes out as
  # it is drawn.
  if failed and ! opts.repeat {
    gnu.error("end of random source")
    exit 1
  }

  if output == "-" {
    gnu.write_bytes(bytes.concat(out))
  } else if let Err(failure) = Path.parse_bytes(raw_output)?.write(bytes.concat(out)) {
    if gnu.errno(failure) == 28 {
      if uutils_adapter() {
        gnu.error(f"write failed: {gnu.strerror(failure)}")
      } else {
        gnu.error(f"write error: {gnu.strerror(failure)}")
      }
    } else {
      gnu.error(f"failed to open {gnu.quote_bytes(raw_output)} for writing: {gnu.strerror(failure)}")
    }

    exit 1
  }

  if failed {
    gnu.error("end of random source")
    exit 1
  }

  if limited {
    gnu.error("unbounded output stopped at 256 KiB: stdout is only flushed when the script ends")
    exit 1
  }
}
