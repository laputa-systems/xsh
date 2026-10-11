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
type WideRange = {low: List[Int], count: List[Int]}
type WideDraw = {value: List[Int], pos: Int, ok: Bool}
type WideSum = {value: List[Int], overflow: Bool}

# Output stops here for an unbounded `-r`: stdout is flushed only when the
# script ends, so endless output could never be delivered.
const OUTPUT_LIMIT = 262144

# The largest range one draw supports: its entropy has to fit in an Int.
const DRAW_LIMIT = 36028797018963968

pure parse_wide_integer(text: Str) -> List[Int]? {
  return null when ! rx"^[0-9]+$".matches(text)

  var value = [0, 0, 0, 0, 0, 0, 0, 0]

  for digit_at in range(text.byte_len()) {
    var carry = text.byte_slice(digit_at, length: 1).parse_int() ?? 0

    for offset in range(8) {
      let index = 7 - offset
      let expanded = value[index] * 10 + carry
      value[index] = expanded % 256
      carry = expanded / 256
    }

    return null when carry != 0
  }

  value
}

pure wide_less(left: List[Int], right: List[Int]) -> Bool {
  for index in range(8) {
    return left[index] < right[index] when left[index] != right[index]
  }

  false
}

pure wide_add(left: List[Int], right: List[Int]) -> WideSum {
  var value = [0, 0, 0, 0, 0, 0, 0, 0]
  var carry = 0

  for offset in range(8) {
    let index = 7 - offset
    let sum = left[index] + right[index] + carry
    value[index] = sum % 256
    carry = sum / 256
  }

  {value: value, overflow: carry != 0}
}

pure wide_subtract(left: List[Int], right: List[Int]) -> List[Int] {
  var value = [0, 0, 0, 0, 0, 0, 0, 0]
  var borrow = 0

  for offset in range(8) {
    let index = 7 - offset
    var difference = left[index] - right[index] - borrow

    if difference < 0 {
      difference += 256
      borrow = 1
    } else {
      borrow = 0
    }

    value[index] = difference
  }

  value
}

pure wide_bit_length(value: List[Int]) -> Int {
  for index in range(8) {
    let byte = value[index]

    if byte != 0 {
      var top = byte
      var byte_bits = 0

      while top > 0 {
        byte_bits += 1
        top = top / 2
      }

      return (7 - index) * 8 + byte_bits
    }
  }

  0
}

# Draws below COUNT with enough random bits to keep rejection sampling efficient
# for both narrow and full-width unsigned ranges.
pure wide_draw(window: Bytes, at: Int, count: List[Int]) -> WideDraw {
  let one = [0, 0, 0, 0, 0, 0, 0, 1]
  let bits = wide_bit_length(wide_subtract(count, one))
  let byte_count = (bits + 7) / 8
  let high_bits = bits % 8
  var pos = at

  while pos + byte_count <= window.len() {
    var candidate = [0, 0, 0, 0, 0, 0, 0, 0]
    let start = 8 - byte_count

    for index in range(byte_count) {
      candidate[start + index] = window.byte_at(pos + index) ?? 0
    }

    if high_bits != 0 and byte_count > 0 {
      var limit = 1

      for _ in range(high_bits) { limit *= 2 }
      candidate[start] = candidate[start] % limit
    }

    pos += byte_count

    if wide_less(candidate, count) {
      return {value: candidate, pos: pos, ok: true}
    }
  }

  {value: [0, 0, 0, 0, 0, 0, 0, 0], pos: pos, ok: false}
}

pure wide_to_decimal(value: List[Int]) -> Str {
  var remainder_value = value
  var digits = ""

  while wide_bit_length(remainder_value) > 0 {
    var quotient = [0, 0, 0, 0, 0, 0, 0, 0]
    var remainder = 0

    for index in range(8) {
      let expanded = remainder * 256 + remainder_value[index]
      quotient[index] = expanded / 10
      remainder = expanded % 10
    }

    digits = "0123456789".byte_slice(remainder, length: 1) + digits
    remainder_value = quotient
  }

  if digits == "" { "0" } else { digits }
}

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

proc main(...argv: List[Bytes]) [fs, process, env, error, io] {
  let prepared = gnu.prepare_arguments(argv)
  let opts: ShufOptions = cli.applet(
    prepared.text,
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
    gnu.usage_error("multiple -i options specified")
    exit 1
  }

  if opts.output.len() > 1 {
    gnu.usage_error("multiple output files specified")
    exit 1
  }

  if opts.echo and ! opts.range.is_empty() {
    gnu.usage_error("cannot combine -e and -i options")
    exit 1
  }

  if ! opts.range.is_empty() and ! opts.operands.is_empty() {
    gnu.usage_error(f"extra operand {gnu.quote(opts.operands[0])}")
    exit 1
  }

  if ! opts.echo and opts.range.is_empty() and opts.operands.len() > 1 {
    gnu.usage_error(f"extra operand {gnu.quote(opts.operands[1])}")
    exit 1
  }

  var head = tio.MAX_COUNT
  var counted = false

  for text in opts.count {
    let parsed = parse_count(text)

    if parsed == null {
      gnu.error(f"invalid line count: {gnu.quote(text)}")
      exit 1
    }

    if ! counted or parsed < head {
      head = parsed
    }

    counted = true
  }

  var lo = 0
  var hi = -1
  var wide_range: WideRange? = null

  if ! opts.range.is_empty() {
    let text = opts.range[0]
    let cut = text.find("-") ?? -1

    if cut < 0 {
      gnu.error(f"invalid input range: {gnu.quote(text)}")
      exit 1
    }

    let from = if cut > 0 { parse_count(text.byte_slice(0, length: cut)) } else { null }
    let to = parse_count(text.byte_slice(cut + 1))

    if from == null or to == null {
      gnu.error(f"invalid input range: {gnu.quote(text)}")
      exit 1
    }

    if from - 1 > to {
      gnu.error(f"invalid input range: {gnu.quote(text)}")
      exit 1
    }

    if from >= tio.MAX_COUNT or to >= tio.MAX_COUNT {
      let wide_low = parse_wide_integer(text.byte_slice(0, length: cut))
      let wide_high = parse_wide_integer(text.byte_slice(cut + 1))

      if wide_low != null and wide_high != null {
        if wide_less(wide_high, wide_low) {
          gnu.error(f"invalid input range: {gnu.quote(text)}")
          exit 1
        }

        let difference = wide_subtract(wide_high, wide_low)
        let size = wide_add(difference, [0, 0, 0, 0, 0, 0, 0, 1])

        if ! size.overflow {
          let small_range = if let Ok(size_as_int) = wide_to_decimal(size.value).parse_int() { size_as_int <= DRAW_LIMIT } else { false }
          let small_head = counted and head < DRAW_LIMIT
          let bounded_repeat = counted and opts.repeat and head == tio.MAX_COUNT

          if small_range or small_head or bounded_repeat {
            wide_range = {low: wide_low, count: size.value}
            lo = 0
            hi = 0
          } else if ! opts.repeat and head == tio.MAX_COUNT {
            gnu.error("memory exhausted")
            exit 1
          } else {
            gnu.error("input ranges beyond 2^63 - 2 are not supported")
            exit 1
          }
        } else if ! opts.repeat and head == tio.MAX_COUNT {
          gnu.error("memory exhausted")
          exit 1
        } else {
          gnu.error("input ranges beyond 2^63 - 2 are not supported")
          exit 1
        }
      } else if ! opts.repeat and head == tio.MAX_COUNT {
        gnu.error("memory exhausted")
        exit 1
      } else {
        gnu.error("input ranges beyond 2^63 - 2 are not supported")
        exit 1
      }
    }

    if wide_range == null {
      lo = from
      hi = to
    }
  }

  let sep = if opts.zero { 0 } else { 10 }
  let mark = bytes.from_ints([sep])?
  let output = opts.output.get(0) ?? "-"

  if head == 0 {
    if output != "-" {
      if let Err(failure) = fp"{output}".write(b"") {
        gnu.name_error(output, failure)
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

  if wide_range != null {
    let wide = wide_range
    let count_text = wide_to_decimal(wide.count)
    let count_as_int = count_text.parse_int() ?? tio.MAX_COUNT
    let amount = if opts.repeat { head } else if head < count_as_int { head } else { count_as_int }
    var random_bytes = if device { bytes.read_at(device_path, 0, 4096) ?? b"" } else { source }
    var random_at = 0
    var produced = 0
    var output_size = 0
    var failed = false
    var limited = false
    var selected: List[List[Int]] = []
    var output_parts: List[Bytes] = []

    while produced < amount and ! failed and ! limited {
      var found = false
      var chosen = [0, 0, 0, 0, 0, 0, 0, 0]

      while ! found and ! failed {
        let candidate = wide_draw(random_bytes, random_at, wide.count)
        random_at = candidate.pos

        if candidate.ok {
          let value = wide_add(wide.low, candidate.value).value

          if opts.repeat or value not in selected {
            chosen = value
            found = true
          }
        } else if device {
          random_bytes = bytes.read_at(device_path, 0, 4096) ?? b""
          random_at = 0
          failed = random_bytes.is_empty()
        } else {
          failed = true
        }
      }

      if found {
        let text = wide_to_decimal(chosen)
        selected = selected.extend([chosen])
        output_parts = output_parts.extend([@[bytes.from_text(text), mark]])
        output_size += text.byte_len() + 1
        produced += 1

        if opts.repeat and head == tio.MAX_COUNT and output_size > OUTPUT_LIMIT {
          limited = true
        }
      }
    }

    if failed {
      gnu.error(f"{gnu.quote(opts.source ?? device_path.display())}: end of file")
      exit 1
    }

    let result = bytes.concat(output_parts)

    if output == "-" {
      gnu.write_bytes(result)
    } else if let Err(failure) = fp"{output}".write(result) {
      if gnu.errno(failure) == 28 {
        gnu.error(f"write error: {gnu.strerror(failure)}")
      } else {
        gnu.name_error(output, failure)
      }

      exit 1
    }

    if limited {
      gnu.error("unbounded output stopped at 256 KiB: stdout is only flushed when the script ends")
      exit 1
    }

    return
  }

  var items: List[Bytes] = []
  var total = 0
  var later: List[Bytes] = []
  var reservoir = false

  if ! opts.range.is_empty() {
    total = hi - lo + 1
  } else if opts.echo {
    items = [gnu.argument_bytes(item, prepared.raw) for item in opts.operands]
    total = items.len()
  } else {
    let name = opts.operands.get(0) ?? "-"
    let raw_name = gnu.argument_bytes(name, prepared.raw)
    let input = Path.parse_bytes(raw_name)?
    let read = if name == "-" { gnu.read_operand(name) } else { input.read_bytes() }

    guard let data = read else { |failure|
      if name == "-" {
        gnu.name_error(name, failure)
      } else {
        gnu.error(f"{gnu.quote_bytes(raw_name, always: false)}: {gnu.strerror(failure)}")
      }

      exit 1
    }

    items = split_records(data, sep)
    total = items.len()

    # With a line limit GNU reads anything but a regular file through a
    # reservoir of that many lines: each later line draws a slot and replaces
    # its line when the slot is inside the reservoir (and a last draw is spent
    # at the end), then the survivors are permuted as usual.
    let place = if name == "-" { /dev/stdin } else { input }
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
    gnu.error(f"{gnu.quote(opts.source ?? device_path.display())}: end of file")
    exit 1
  }

  if output == "-" {
    gnu.write_bytes(bytes.concat(out))
  } else if let Err(failure) = fp"{output}".write(bytes.concat(out)) {
    if gnu.errno(failure) == 28 {
      gnu.error(f"write error: {gnu.strerror(failure)}")
    } else {
      gnu.name_error(output, failure)
    }

    exit 1
  }

  if failed {
    gnu.error(f"{gnu.quote(opts.source ?? device_path.display())}: end of file")
    exit 1
  }

  if limited {
    gnu.error("unbounded output stopped at 256 KiB: stdout is only flushed when the script ends")
    exit 1
  }
}
