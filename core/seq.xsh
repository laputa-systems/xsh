#!/bin/xsh
use lib.gnu

const USAGE = """Usage: seq [OPTION]... LAST
  or:  seq [OPTION]... FIRST LAST
  or:  seq [OPTION]... FIRST INCREMENT LAST
Print numbers from FIRST to LAST, in steps of INCREMENT.

Mandatory arguments to long options are mandatory for short options too.
  -f, --format=FORMAT      use printf style floating-point FORMAT
  -s, --separator=STRING   use STRING to separate numbers (default: \\n)
  -t, --terminator=STRING  use STRING to end the output (default: \\n)
  -w, --equal-width        equalize width by padding with leading zeroes
      --help        display this help and exit
      --version     output version information and exit

If FIRST or INCREMENT is omitted, it defaults to 1.  That is, an
omitted INCREMENT defaults to 1 even when LAST is smaller than FIRST.
The sequence of numbers ends when the sum of the current number and
INCREMENT would become greater than LAST.
FIRST, INCREMENT, and LAST are interpreted as floating point values.
INCREMENT is usually positive if FIRST is smaller than LAST, and
INCREMENT is usually negative if FIRST is greater than LAST.
INCREMENT must not be 0; none of FIRST, INCREMENT and LAST may be NaN.
FORMAT must be suitable for printing one argument of type 'double';
it defaults to %.PRECf if FIRST, INCREMENT, and LAST are all fixed point
decimal numbers with maximum precision PREC, and to %g otherwise.
"""

type SeqOptions = {
  format: Str?,
  separator: Str,
  terminator: Str,
  equal_width: Bool,
  help: Bool,
  version: Bool,
  numbers: List[Str],
}

# A parsed argument. `kind` is `fin`, `inf`, or `bad` (`why` says which parse
# error). A finite value is `-?digits / 10^scale` with `scale >= 0`; `neg` is
# kept for zero so `-0` prints as given. `ints` and `fracs` are the digit
# counts GNU derives from the text for -w and the default precision; `fracs`
# is -1 for a hex float, which has no decimal precision.
type Num = {
  kind: Str,
  why: Str,
  neg: Bool,
  digits: Str,
  scale: Int,
  ints: Int,
  fracs: Int,
}

# One printf conversion with the literal text around it. A `precision` of -1
# is unset.
type Spec = {
  prefix: Str,
  suffix: Str,
  left: Bool,
  plus: Bool,
  space: Bool,
  alt: Bool,
  zero: Bool,
  width: Int,
  precision: Int,
  conv: Str,
}

type Signed = {neg: Bool, digits: Str}

# Exponents beyond this are rejected (positive) or read as zero (negative), as
# GNU does for values outside the long double range; widths and precisions
# beyond it are rejected.
const EXP_LIMIT = 100000
const SIZE_LIMIT = 10000000

pure zeros(count: Int) -> Str {
  var out = ""
  var chunk = "0"
  var left = count

  while left > 0 {
    if left % 2 == 1 {
      out = out + chunk
    }

    left = left / 2

    if left > 0 {
      chunk = chunk + chunk
    }
  }

  out
}

pure strip_zeros(text: Str) -> Str {
  var at = 0
  let total = text.byte_len()

  while at < total - 1 and text.byte_slice(at, length: 1) == "0" {
    at += 1
  }

  text.byte_slice(at)
}

pure unsigned_cmp(left: Str, right: Str) -> Int {
  let a = left.byte_len()
  let b = right.byte_len()

  return -1 when a < b
  return 1 when a > b
  return -1 when left < right
  return 1 when left > right

  0
}

# Base 1e9 limbs, least significant first.
pure limbs(text: Str) -> List[Int] {
  var out: List[Int] = []
  var end = text.byte_len()

  while end > 0 {
    let start = if end > 9 { end - 9 } else { 0 }
    out += [text.byte_slice(start, length: end - start).parse_int() ?? 0]
    end = start
  }

  out
}

pure from_limbs(parts: List[Int]) -> Str {
  var out = ""
  var at = parts.len() - 1

  while at >= 0 {
    let piece = f"{parts[at]}"
    out = if out == "" { piece } else { out + zeros(9 - piece.byte_len()) + piece }
    at -= 1
  }

  strip_zeros(out)
}

pure unsigned_add(left: Str, right: Str) -> Str {
  let a = limbs(left)
  let b = limbs(right)
  var out: List[Int] = []
  var carry = 0
  var at = 0
  let count = if a.len() > b.len() { a.len() } else { b.len() }

  while at < count or carry > 0 {
    let sum = (if at < a.len() { a[at] } else { 0 }) + (if at < b.len() { b[at] } else { 0 }) + carry
    out += [sum % 1000000000]
    carry = sum / 1000000000
    at += 1
  }

  from_limbs(out)
}

# LEFT - RIGHT for LEFT >= RIGHT.
pure unsigned_sub(left: Str, right: Str) -> Str {
  let a = limbs(left)
  let b = limbs(right)
  var out: List[Int] = []
  var borrow = 0
  var at = 0

  while at < a.len() {
    var gap = a[at] - (if at < b.len() { b[at] } else { 0 }) - borrow

    if gap < 0 {
      gap += 1000000000
      borrow = 1
    } else {
      borrow = 0
    }

    out += [gap]
    at += 1
  }

  from_limbs(out)
}

# TEXT * FACTOR + PLUS for a factor below 2^31.
pure mul_small_add(text: Str, factor: Int, plus: Int) -> Str {
  var out: List[Int] = []
  var carry = plus

  for part in limbs(text) {
    let product = part * factor + carry
    out += [product % 1000000000]
    carry = product / 1000000000
  }

  while carry > 0 {
    out += [carry % 1000000000]
    carry = carry / 1000000000
  }

  from_limbs(out)
}

# TEXT * BASE^EXPONENT, for BASE 2 or 5.
pure mul_pow(text: Str, base: Int, exponent: Int) -> Str {
  let widest = if base == 2 { 30 } else { 13 }
  var out = text
  var left = exponent

  while left > 0 {
    let step = if left > widest { widest } else { left }
    var factor = 1

    repeat step times {
      factor *= base
    }

    out = mul_small_add(out, factor, 0)
    left -= step
  }

  out
}

pure signed_add(a: Signed, b: Signed) -> Signed {
  return {neg: a.neg, digits: unsigned_add(a.digits, b.digits)} when a.neg == b.neg

  let order = unsigned_cmp(a.digits, b.digits)

  return {neg: false, digits: "0"} when order == 0
  return {neg: a.neg, digits: unsigned_sub(a.digits, b.digits)} when order > 0

  {neg: b.neg, digits: unsigned_sub(b.digits, a.digits)}
}

# -1, 0, or 1; a negative zero equals zero.
pure signed_cmp(a: Signed, b: Signed) -> Int {
  let a_neg = a.neg and a.digits != "0"
  let b_neg = b.neg and b.digits != "0"

  return -1 when a_neg and ! b_neg
  return 1 when b_neg and ! a_neg

  let order = unsigned_cmp(a.digits, b.digits)

  if a_neg { -order } else { order }
}

pure hex_value(text: Str) -> Str {
  var out = "0"

  for index in range(text.byte_len()) {
    out = mul_small_add(out, 16, "0123456789abcdef".find(text.byte_slice(index, length: 1).lower()) ?? 0)
  }

  out
}

pure bad(why: Str) -> Num {
  {kind: "bad", why: why, neg: false, digits: "0", scale: 0, ints: 0, fracs: 0}
}

# A finite value normalized to a scale of at least 0.
pure finite(neg: Bool, digits: Str, scale: Int, ints: Int, fracs: Int) -> Num {
  let clean = strip_zeros(digits)

  return {kind: "fin", why: "", neg: neg, digits: clean, scale: scale, ints: ints, fracs: fracs} when scale >= 0

  let grown = if clean == "0" { "0" } else { clean + zeros(-scale) }

  {kind: "fin", why: "", neg: neg, digits: grown, scale: 0, ints: ints, fracs: fracs}
}

# The integer value of an exponent text, or null when it does not fit 64 bits.
pure exponent_value(text: Str) -> Int? {
  match text.parse_int() {
    Ok(value) => value
    Err(_) => null
  }
}

pure parse_hex(body: Str, neg: Bool) -> Num {
  let parts = rx"^0[xX]([0-9a-fA-F]*)(?:\.([0-9a-fA-F]*))?(?:[pP]([+-]?[0-9]+))?$".captures(body)

  return bad("float") when parts.len() == 0 or (parts[1] == "" and parts[2] == "")

  let exponent = parts[3]
  let power = if exponent == "" { 0 } else { exponent_value(exponent) ?? (if exponent.starts_with("-") { -99999999 } else { 99999999 }) }

  return bad("float") when power > EXP_LIMIT

  let fracs = if exponent == "" and body.find(".") == null { 0 } else { -1 }
  let shift = power - 4 * parts[2].byte_len()
  let mantissa = hex_value(parts[1] + parts[2])

  return finite(neg, "0", 0, 0, fracs) when mantissa == "0" or power < -EXP_LIMIT
  return finite(neg, mul_pow(mantissa, 2, shift), 0, 0, fracs) when shift >= 0

  finite(neg, mul_pow(mantissa, 5, -shift), -shift, 0, fracs)
}

pure parse_number(raw: Str) -> Num {
  let text = rx"^[ \t\n\v\f\r]*".replace(raw, "")
  let neg = text.starts_with("-")
  let body = if neg or text.starts_with("+") { text.byte_slice(1) } else { text }
  let word = body.lower()

  return {kind: "inf", why: "", neg: neg, digits: "0", scale: 0, ints: 0, fracs: 0} when word == "inf" or word == "infinity"
  return bad("nan") when word == "nan"
  return parse_hex(body, neg) when word.starts_with("0x")

  let parts = rx"^([0-9]*)(?:\.([0-9]*))?(?:[eE]([+-]?[0-9]+))?$".captures(body)

  return bad("float") when parts.len() == 0 or (parts[1] == "" and parts[2] == "")

  let whole = parts[1]
  let fraction = parts[2]
  let parsed: Int? = if parts[3] == "" { 0 } else { exponent_value(parts[3]) }
  let overflow = parsed == null
  let exp = parsed ?? 0
  let huge = overflow or exp > EXP_LIMIT or exp < -EXP_LIMIT

  return bad("float") when huge and ! parts[3].starts_with("-")

  # Digit counts as GNU derives them from the text: the sign counts as an
  # integral digit, `.5` counts a leading zero, and an exponent that does not
  # fit 64 bits counts as 0.
  let counted = if overflow { 0 } else { exp }
  let ints = (if neg { 1 } else { 0 }) + (if whole == "" { 1 } else { whole.byte_len() }) + (if counted > 0 { counted } else { 0 })
  let fracs = if counted < fraction.byte_len() { fraction.byte_len() - counted } else { 0 }

  return finite(neg, "0", 0, ints, if fracs > EXP_LIMIT { EXP_LIMIT } else { fracs }) when huge

  finite(neg, whole + fraction, fraction.byte_len() - exp, ints, fracs)
}

# DIGITS without its last DROP digits, rounded half to even.
pure round_digits(digits: Str, drop: Int) -> Str {
  return digits when drop <= 0

  let padded = if digits.byte_len() < drop + 1 { zeros(drop + 1 - digits.byte_len()) + digits } else { digits }
  let keep = padded.byte_len() - drop
  let kept = padded.byte_slice(0, length: keep)
  let tail = padded.byte_slice(keep)
  let first = tail.byte_slice(0, length: 1)
  let rest = tail.byte_slice(1)
  let beyond = rest != zeros(rest.byte_len())
  let odd = (kept.byte_slice(keep - 1, length: 1).parse_int() ?? 0) % 2 == 1
  let up = first > "5" or (first == "5" and (beyond or odd))

  return strip_zeros(if up { unsigned_add(kept, "1") } else { kept })
}

# The magnitude with exactly PLACES fractional digits, as text.
pure fixed_text(digits: Str, scale: Int, places: Int) -> Str {
  let coefficient = if scale <= places { digits + zeros(places - scale) } else { round_digits(digits, scale - places) }

  return coefficient when places == 0

  let padded = if coefficient.byte_len() < places + 1 { zeros(places + 1 - coefficient.byte_len()) + coefficient } else { coefficient }
  let cut = padded.byte_len() - places

  f"{padded.byte_slice(0, length: cut)}.{padded.byte_slice(cut)}"
}

# DIGITS rounded to SIG significant digits: the digit text and the decimal
# exponent of the first digit.
type Sci = {mantissa: Str, exponent: Int}

pure sci_parts(digits: Str, scale: Int, sig: Int) -> Sci {
  return {mantissa: zeros(sig), exponent: 0} when digits == "0"

  let count = digits.byte_len()
  var exponent = count - 1 - scale
  var mantissa = if count > sig { round_digits(digits, count - sig) } else { digits + zeros(sig - count) }

  if mantissa.byte_len() > sig {
    mantissa = mantissa.byte_slice(0, length: sig)
    exponent += 1
  }

  {mantissa: mantissa, exponent: exponent}
}

pure exponent_text(value: Int, upper: Bool) -> Str {
  let size = if value < 0 { -value } else { value }
  let digits = if size < 10 { f"0{size}" } else { f"{size}" }

  f"{if upper { "E" } else { "e" }}{if value < 0 { "-" } else { "+" }}{digits}"
}

pure trim_zeros_right(text: Str) -> Str {
  var end = text.byte_len()

  while end > 0 and text.byte_slice(end - 1, length: 1) == "0" {
    end -= 1
  }

  text.byte_slice(0, length: end)
}

# `d.ddde+XX`; `strip` drops trailing fractional zeros (%g without `#`).
pure sci_body(parts: Sci, upper: Bool, alt: Bool, strip: Bool) -> Str {
  let lead = parts.mantissa.byte_slice(0, length: 1)
  let rest = if strip { trim_zeros_right(parts.mantissa.byte_slice(1)) } else { parts.mantissa.byte_slice(1) }
  let point = if rest != "" or alt { "." } else { "" }

  f"{lead}{point}{rest}{exponent_text(parts.exponent, upper)}"
}

pure strip_fraction(text: Str) -> Str {
  return text when text.find(".") == null

  var end = text.byte_len()

  while text.byte_slice(end - 1, length: 1) == "0" {
    end -= 1
  }

  if text.byte_slice(end - 1, length: 1) == "." {
    end -= 1
  }

  text.byte_slice(0, length: end)
}

# One printf float conversion of `neg digits / 10^scale`.
pure format_number(neg: Bool, digits: Str, scale: Int, spec: Spec) -> Str {
  let upper = spec.conv == "E" or spec.conv == "G" or spec.conv == "F"
  let lower = spec.conv.lower()
  let given = if spec.precision < 0 { 6 } else { spec.precision }
  var body = ""

  if lower == "f" {
    body = fixed_text(digits, scale, given)

    if given == 0 and spec.alt {
      body = body + "."
    }
  } else if lower == "e" {
    body = sci_body(sci_parts(digits, scale, given + 1), upper, spec.alt, false)
  } else {
    let sig = if given == 0 { 1 } else { given }
    let parts = sci_parts(digits, scale, sig)

    if parts.exponent < -4 or parts.exponent >= sig {
      body = sci_body(parts, upper, spec.alt, ! spec.alt)
    } else {
      let places = sig - 1 - parts.exponent
      body = fixed_text(digits, scale, places)

      if ! spec.alt {
        body = strip_fraction(body)
      }
    }
  }

  let sign = if neg { "-" } else if spec.plus { "+" } else if spec.space { " " } else { "" }
  let size = sign.byte_len() + body.byte_len()

  return f"{sign}{body}" when size >= spec.width
  return f"{sign}{body}{pad_spaces(spec.width - size)}" when spec.left
  return f"{sign}{zeros(spec.width - size)}{body}" when spec.zero

  f"{pad_spaces(spec.width - size)}{sign}{body}"
}

pure pad_spaces(count: Int) -> Str {
  zeros(count).replace("0", " ")
}

# A parsed -f format, or the message that rejects it.
type Parsed = {message: Str, spec: Spec}

# `shown` is the quoted format for messages.
pure parse_format(format: Str, shown: Str) -> Parsed {
  let empty = {prefix: "", suffix: "", left: false, plus: false, space: false, alt: false, zero: false, width: 0, precision: -1, conv: "g"}
  var prefix = ""
  var at = 0

  loop {
    let found = format.find("%", at) ?? -1

    return {message: f"format {shown} has no % directive", spec: empty} when found < 0

    prefix += format.byte_slice(at, length: found - at)

    if format.byte_slice(found + 1, length: 1) == "%" {
      prefix += "%"
      at = found + 2
    } else {
      at = found + 1
      break
    }
  }

  return {message: f"format {shown} ends in %", spec: empty} when at >= format.byte_len()

  let rest = format.byte_slice(at)
  let parts = rx"^([-+ #0']*)([0-9]*)(\.?)([0-9]*)(L?)(.?)".captures(rest)
  let flags = parts[1]
  let conv = parts[6]

  return {message: f"invalid width: '{parts[2]}'", spec: empty} when parts[2].byte_len() > 8 or (parts[2].parse_int() ?? 0) > SIZE_LIMIT
  return {message: f"invalid precision: '{parts[4]}'", spec: empty} when parts[4].byte_len() > 8 or (parts[4].parse_int() ?? 0) > SIZE_LIMIT
  return {message: f"{format}: invalid conversion specification", spec: empty} when conv == "c"
  return {message: f"invalid format {shown}, directive must be %[0]['][-][N][.][N]f", spec: empty} when conv == "" or ! ("aAeEfFgG".find(conv) != null)
  return {message: f"format {shown}: the %{conv} conversion is not supported", spec: empty} when conv == "a" or conv == "A"

  var suffix = ""
  var from = parts[0].byte_len()
  let tail = rest.byte_slice(from)

  var cursor = 0

  loop {
    let found = tail.find("%", cursor) ?? -1

    if found < 0 {
      suffix += tail.byte_slice(cursor)
      break
    }

    suffix += tail.byte_slice(cursor, length: found - cursor)

    return {message: f"format {shown} has too many % directives", spec: empty} when tail.byte_slice(found + 1, length: 1) != "%"

    suffix += "%"
    cursor = found + 2
  }

  let spec = {
    prefix: prefix,
    suffix: suffix,
    left: flags.find("-") != null,
    plus: flags.find("+") != null,
    space: flags.find(" ") != null,
    alt: flags.find("#") != null,
    zero: flags.find("0") != null,
    width: parts[2].parse_int() ?? 0,
    precision: if parts[3] == "" { -1 } else { parts[4].parse_int() ?? 0 },
    conv: conv,
  }

  {message: "", spec: spec}
}

# The first argument that sits at an option position and looks like a number
# (`-1`, `-.5`) ends option parsing, as in GNU seq; `--` is inserted before it.
pure protect_numbers(argv: List[Str]) -> List[Str] {
  var at = 0

  while at < argv.len() {
    let item = argv[at]

    break when item == "--" or ! item.starts_with("-") or item == "-"
    break when rx"^-[0-9]".matches(item) or rx"^-\.[0-9]".matches(item)

    var takes = false

    if item.starts_with("--") {
      let name = item.byte_slice(2)
      takes = name.find("=") == null and name != "" and ("format".starts_with(name) or "separator".starts_with(name) or "terminator".starts_with(name))
    } else {
      for index in range(1, item.byte_len()) {
        let letter = item.byte_slice(index, length: 1)

        if letter == "f" or letter == "s" or letter == "t" {
          takes = index == item.byte_len() - 1
          break
        }

        break when letter != "w"
      }
    }

    at += if takes { 2 } else { 1 }
  }

  return argv when at >= argv.len() or argv[at] == "--" or ! argv[at].starts_with("-") or argv[at] == "-"

  [@argv[..at], "--", @argv[at..]]
}

proc number_argument(text: Str) [process, env] -> Num {
  let value = parse_number(text)

  if value.kind == "bad" {
    gnu.usage_error(f"invalid {if value.why == "nan" { "'not-a-number'" } else { "floating point" }} argument: {gnu.quote(text)}")
  }

  value
}

pure value_of(number: Num, scale: Int) -> Signed {
  {neg: number.neg, digits: if number.scale < scale { number.digits + zeros(scale - number.scale) } else { number.digits }}
}

pure infinity_line(negative: Bool, spec: Spec) -> Str {
  let sign = if negative { "-" } else if spec.plus { "+" } else if spec.space { " " } else { "" }
  let text = sign + "inf"
  let padding = if spec.width > text.byte_len() { pad_spaces(spec.width - text.byte_len()) } else { "" }
  let value = if spec.left { text + padding } else { padding + text }

  spec.prefix + value + spec.suffix
}

proc seq_write(text: Bytes) [process, env, io] {
  if let Err(failure) = io.write_stdout_bytes(text) {
    gnu.write_failed(failure)
  }
}

# Infinite sequences must reach the reader as they are produced. The host
# runtime normally ignores SIGPIPE so finite writers can report errors; seq
# restores the utility default while it is streaming an endless result.
proc stream_endless(first: Num, step: Num, spec: Spec, separator: Bytes) [process, env, error, io] {
  process.set_signal_action("PIPE", "default")?

  var emitted = false
  let scale = if first.scale > step.scale { first.scale } else { step.scale }
  let plain = spec.conv == "f" and spec.precision == scale and spec.prefix == "" and spec.suffix == "" and ! spec.left and ! spec.plus and ! spec.space and ! spec.alt
  let increment = if step.kind == "fin" { value_of(step, scale) } else { {neg: step.neg, digits: "0"} }
  var current = value_of(first, scale)

  if first.kind == "inf" {
    let line = infinity_line(first.neg, spec)

    while true {
      seq_write(bytes.concat([if emitted { separator } else { b"" }, bytes.from_text(line)]))
      emitted = true
    }
  }

  if step.kind == "inf" {
    let line = if plain { plain_line(current, scale, spec.width) } else { spec.prefix + format_number(current.neg, current.digits, scale, spec) + spec.suffix }
    seq_write(bytes.from_text(line))
    let infinite = infinity_line(step.neg, spec)

    while true {
      seq_write(bytes.concat([separator, bytes.from_text(infinite)]))
    }
  }

  while true {
    let line = if plain { plain_line(current, scale, spec.width) } else { spec.prefix + format_number(current.neg, current.digits, scale, spec) + spec.suffix }
    seq_write(bytes.concat([if emitted { separator } else { b"" }, bytes.from_text(line)]))
    emitted = true
    current = signed_add(current, increment)
  }
}

# Preserve byte-valued separator and terminator arguments for stdout. The
# parser still consumes the lossy Str view, while cli.argv_bytes exposes the
# original argument vector for output values.
pure sequence_option_bytes(argv: List[Str], raw: List[Bytes], long_name: Str, short_name: Str, fallback: Bytes) -> Bytes {
  var selected = fallback
  var at = 0

  while at < argv.len() {
    let item = argv[at]

    if item == "--" or ! item.starts_with("-") or item == "-" {
      break
    }

    if item.starts_with("--") {
      let equal_at = item.find("=")
      let name = if equal_at == null { item.byte_slice(2) } else { item.byte_slice(2, length: (equal_at ?? 0) - 2) }
      let is_target = name != "" and long_name.starts_with(name)

      if is_target {
        if equal_at == null {
          if at + 1 < raw.len() { selected = raw[at + 1] }
          at += 2
        } else {
          selected = raw[at].slice((equal_at ?? 0) + 1)
          at += 1
        }
      } else if equal_at == null and ("format".starts_with(name) or "separator".starts_with(name) or "terminator".starts_with(name)) {
        at += 2
      } else {
        at += 1
      }
    } else {
      var pos = 1
      var found = false
      var takes_next = false

      while pos < item.byte_len() {
        let letter = item.byte_slice(pos, length: 1)

        if letter == short_name {
          if pos + 1 < item.byte_len() {
            selected = raw[at].slice(pos + 1)
            at += 1
          } else {
            if at + 1 < raw.len() { selected = raw[at + 1] }
            at += 2
          }
          found = true
          break
        }

        if letter == "f" or letter == "s" or letter == "t" {
          takes_next = pos == item.byte_len() - 1
          break
        }

        pos += 1
      }

      if ! found {
        at += if takes_next { 2 } else { 1 }
      }
    }
  }

  selected
}

# A line for the default fixed-point format at a scale equal to the precision.
pure plain_line(value: Signed, places: Int, width: Int) -> Str {
  let sign = if value.neg { "-" } else { "" }
  let padded = if places > 0 and value.digits.byte_len() < places + 1 { zeros(places + 1 - value.digits.byte_len()) + value.digits } else { value.digits }
  let body = if places == 0 { padded } else { f"{padded.byte_slice(0, length: padded.byte_len() - places)}.{padded.byte_slice(padded.byte_len() - places)}" }
  let size = sign.byte_len() + body.byte_len()

  return f"{sign}{body}" when size >= width

  f"{sign}{zeros(width - size)}{body}"
}

proc main(...argv: List[Str]) [process, env, error, io] {
  let raw_argv = cli.argv_bytes()
  let opts: SeqOptions = cli.applet(
    protect_numbers(argv),
    {
      gnu: {status: 1, permute: false},
      format: {form: "-f --format FORMAT"},
      separator: {form: "-s --separator STRING", default: "\n"},
      terminator: {form: "-t --terminator STRING", default: "\n"},
      equal_width: {form: "-w --equal-width", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      numbers: {form: "...NUMBER"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("seq")
    return
  }

  let words = opts.numbers

  if words.len() == 0 {
    gnu.missing_operand()
  }

  if words.len() > 3 {
    gnu.extra_operand(words[3])
  }

  if opts.equal_width and opts.format != null {
    gnu.usage_error("format string may not be specified when printing equal width strings")
  }

  let one = parse_number("1")
  let first = if words.len() > 1 { number_argument(words[0]) } else { one }
  let step = if words.len() > 2 { number_argument(words[1]) } else { one }

  if step.kind == "fin" and step.digits == "0" {
    gnu.usage_error(f"invalid Zero increment value: {gnu.quote(words[1])}")
  }

  let last = number_argument(words[words.len() - 1])
  let separator_bytes = sequence_option_bytes(argv, raw_argv, "separator", "s", bytes.from_text(opts.separator))
  let terminator_bytes = sequence_option_bytes(argv, raw_argv, "terminator", "t", bytes.from_text(opts.terminator))
  var spec = {prefix: "", suffix: "", left: false, plus: false, space: false, alt: false, zero: true, width: 0, precision: -1, conv: "g"}
  var places = -1

  if let format = opts.format {
    let parsed = parse_format(format, gnu.quote(format))

    if parsed.message != "" {
      gnu.error(parsed.message)
      exit 1
    }

    spec = parsed.spec
  } else if first.fracs >= 0 and step.fracs >= 0 and last.fracs >= 0 {
    places = if first.fracs > step.fracs { first.fracs } else { step.fracs }

    var width = 0

    if opts.equal_width {
      width = first.ints
      width = if step.ints > width { step.ints } else { width }
      width = if last.ints > width { last.ints } else { width }
      width += if places > 0 { places + 1 } else { 0 }
    }

    spec = {...spec, zero: true, width: width, precision: places, conv: "f"}
  } else {
    spec = {...spec, zero: false}
  }

  let forward = ! step.neg
  let endless = if forward {
    (last.kind == "inf" and ! last.neg) or (first.kind == "inf" and first.neg)
  } else {
    (last.kind == "inf" and last.neg) or (first.kind == "inf" and ! first.neg)
  }

  if endless {
    stream_endless(first, step, spec, separator_bytes)
    return
  }

  var lines: List[Str] = []
  let empty = first.kind == "inf" or last.kind == "inf"

  if ! empty {
    var scale = if first.scale > last.scale { first.scale } else { last.scale }
    scale = if step.kind == "fin" and step.scale > scale { step.scale } else { scale }

    let from = value_of(first, scale)
    let stop = value_of(last, scale)
    let inc = if step.kind == "fin" { value_of(step, scale) } else { {neg: step.neg, digits: "0"} }
    let plain = spec.conv == "f" and spec.precision == scale and spec.prefix == "" and spec.suffix == "" and ! spec.left and ! spec.plus and ! spec.space and ! spec.alt
    let small = from.digits.byte_len() <= 17 and stop.digits.byte_len() <= 17 and inc.digits.byte_len() <= 17

    if small and step.kind == "fin" {
      var current = if from.neg { 0 - (from.digits.parse_int() ?? 0) } else { from.digits.parse_int() ?? 0 }
      let delta = if inc.neg { 0 - (inc.digits.parse_int() ?? 0) } else { inc.digits.parse_int() ?? 0 }
      let limit = if stop.neg { 0 - (stop.digits.parse_int() ?? 0) } else { stop.digits.parse_int() ?? 0 }
      var negative = from.neg

      if plain and scale == 0 and spec.width == 0 {
        if negative and current == 0 {
          lines += ["-0"]
          current += delta
        }

        while if forward { current <= limit } else { current >= limit } {
          lines += [f"{current}"]
          current += delta
        }
      } else {
        while if forward { current <= limit } else { current >= limit } {
          let value = {neg: negative, digits: f"{if current < 0 { 0 - current } else { current }}"}

          lines += [if plain { plain_line(value, scale, spec.width) } else { spec.prefix + format_number(negative, value.digits, scale, spec) + spec.suffix }]
          current += delta
          negative = current < 0
        }
      }
    } else {
      var current = from
      var count = 0

      while (if forward { signed_cmp(current, stop) <= 0 } else { signed_cmp(current, stop) >= 0 }) and (count == 0 or step.kind == "fin") {
        lines += [if plain { plain_line(current, scale, spec.width) } else { spec.prefix + format_number(current.neg, current.digits, scale, spec) + spec.suffix }]
        current = signed_add(current, inc)
        count += 1
      }
    }
  }

  if lines.len() > 0 {
    var output: List[Bytes] = []

    for index in range(lines.len()) {
      if index > 0 { output += [separator_bytes] }
      output += [bytes.from_text(lines[index])]
    }

    output += [terminator_bytes]
    gnu.write_bytes(bytes.concat(output))
  }
}
