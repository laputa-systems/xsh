#!/bin/xsh
use lib.gnu
use lib.textio_a1 as tio

const USAGE = """Usage: numfmt [OPTION]... [NUMBER]...
Reformat NUMBER(s), or the numbers from standard input if none are specified.

Mandatory arguments to long options are mandatory for short options too.
      --debug          print warnings about invalid input
  -d, --delimiter=X    use X instead of whitespace for field delimiter
      --field=FIELDS   replace the numbers in these input fields (default=1);
                         see FIELDS below
      --format=FORMAT  use printf style floating-point FORMAT;
                         see FORMAT below for details
      --from=UNIT      auto-scale input numbers to UNITs; default is 'none';
                         see UNIT below
      --from-unit=N    specify the input unit size (instead of the default 1)
      --grouping       use locale-defined grouping of digits, e.g. 1,000,000
                         (which means it has no effect in the C/POSIX locale)
      --header[=N]     print (without converting) the first N header lines;
                         N defaults to 1 if not specified
      --invalid=MODE   failure mode for invalid numbers: MODE can be:
                         abort (the default), fail, warn, ignore
      --padding=N      pad the output to N characters; positive N will
                         right-align; negative N will left-align;
                         padding is ignored if the output is wider than N;
                         the default is to automatically pad if a whitespace
                         is found
      --round=METHOD   use METHOD for rounding when scaling; METHOD can be:
                         up, down, from-zero (the default), towards-zero,
                         nearest
      --suffix=SUFFIX  add SUFFIX to output numbers, and accept optional
                         SUFFIX in input numbers
      --to=UNIT        auto-scale output numbers to UNITs; see UNIT below
      --to-unit=N      the output unit size (instead of the default 1)
      --unit-separator=STRING  use STRING to separate the number from any unit
                         when printing; by default, no separator is used
  -z, --zero-terminated    line delimiter is NUL, not newline
      --help        display this help and exit
      --version     output version information and exit

UNIT options:
  none       No auto-scaling is done.  Suffixes will trigger an error.
  auto       Accept optional single/two letter suffix:
               1K  = 1000,
               1Ki = 1024,
               1M  = 1000000,
               1Mi = 1048576,
  si         Accept optional single letter suffix:
               1K = 1000,
               1M = 1000000,
               ...
  iec        Accept optional single letter suffix:
               1K = 1024,
               1M = 1048576,
               ...
  iec-i      Accept optional two-letter suffix:
               1Ki = 1024,
               1Mi = 1048576,
               ...

FIELDS supports cut(1) style field ranges:
  N    N'th field, counted from 1
  N-   from N'th field, to end of line
  N-M  from N'th to M'th field (inclusive)
  -M   from first to M'th field (inclusive)
  -    all fields
Multiple fields/ranges can be separated with commas

FORMAT must be suitable for printing one floating-point argument '%f'.
Optional quote (%'f) will enable --grouping (if supported by current locale).
Optional width value (%10f) will pad output.  Optional zero (%010f) width
will zero pad the number.  Optional negative values (%-10f) will left align.
Optional precision (%.1f) will override the input determined precision.
"""

type NumfmtOptions = {
  debug: Bool,
  delimiter: Str?,
  field: Str,
  format: Str?,
  from: Str,
  from_unit: Str,
  grouping: Bool,
  header: Str,
  invalid: Str,
  padding: Str?,
  round: Str,
  suffix: Str?,
  to: Str,
  to_unit: Str,
  unit_separator: Str?,
  zero: Bool,
  help: Bool,
  version: Bool,
  numbers: List[Str],
}

# Magnitudes are little-endian base 10^9 limbs without high zero limbs; zero
# is the empty list.
const BASE = 1000000000

pure trim_limbs(parts: List[Int]) -> List[Int] {
  var end = parts.len()

  while end > 0 and parts[end - 1] == 0 {
    end -= 1
  }

  if end == parts.len() { parts } else { parts[..end] }
}

pure big_from(text: Str) -> List[Int] {
  var out: List[Int] = []
  var end = text.byte_len()

  while end > 0 {
    let start = if end > 9 { end - 9 } else { 0 }
    out += [text.byte_slice(start, length: end - start).parse_int() ?? 0]
    end = start
  }

  trim_limbs(out)
}

pure big_text(parts: List[Int]) -> Str {
  return "0" when parts.len() == 0

  var out = f"{parts[parts.len() - 1]}"
  var at = parts.len() - 2

  while at >= 0 {
    let piece = f"{parts[at]}"
    out = out + "000000000".byte_slice(0, length: 9 - piece.byte_len()) + piece
    at -= 1
  }

  out
}

pure big_cmp(a: List[Int], b: List[Int]) -> Int {
  return -1 when a.len() < b.len()
  return 1 when a.len() > b.len()

  var at = a.len() - 1

  while at >= 0 {
    return -1 when a[at] < b[at]
    return 1 when a[at] > b[at]

    at -= 1
  }

  0
}

# A - B for A >= B.
pure big_sub(a: List[Int], b: List[Int]) -> List[Int] {
  var out: List[Int] = []
  var borrow = 0
  var at = 0

  while at < a.len() {
    var gap = a[at] - (if at < b.len() { b[at] } else { 0 }) - borrow

    if gap < 0 {
      gap += BASE
      borrow = 1
    } else {
      borrow = 0
    }

    out += [gap]
    at += 1
  }

  trim_limbs(out)
}

pure big_mul_small(a: List[Int], factor: Int) -> List[Int] {
  return [] when factor == 0 or a.len() == 0

  var out: List[Int] = []
  var carry = 0

  for part in a {
    let product = part * factor + carry
    out += [product % BASE]
    carry = product / BASE
  }

  while carry > 0 {
    out += [carry % BASE]
    carry = carry / BASE
  }

  out
}

pure big_mul(a: List[Int], b: List[Int]) -> List[Int] {
  return [] when a.len() == 0 or b.len() == 0

  var out: List[Int] = [0 for slot in range(a.len() + b.len())]

  for i in range(a.len()) {
    var carry = 0
    let factor = a[i]

    if factor != 0 {
      for j in range(b.len()) {
        let current = out[i + j] + factor * b[j] + carry
        out[i + j] = current % BASE
        carry = current / BASE
      }

      out[i + b.len()] += carry
    }
  }

  trim_limbs(out)
}

type Division = {quotient: List[Int], remainder: List[Int]}

# A / B and A % B for nonzero B.
pure big_divmod(a: List[Int], b: List[Int]) -> Division {
  return {quotient: [], remainder: a} when big_cmp(a, b) < 0

  if b.len() == 1 {
    var quotient: List[Int] = [0 for slot in range(a.len())]
    var rest = 0
    var at = a.len() - 1

    while at >= 0 {
      let current = rest * BASE + a[at]
      quotient[at] = current / b[0]
      rest = current % b[0]
      at -= 1
    }

    return {quotient: trim_limbs(quotient), remainder: if rest == 0 { [] } else { [rest] }}
  }

  let width = b.len()
  let divisor_top = b[width - 1].float() * 1000000000.0 + b[width - 2].float()
  var quotient: List[Int] = [0 for slot in range(a.len())]
  var rest: List[Int] = []
  var at = a.len() - 1

  while at >= 0 {
    rest = trim_limbs([a[at]] + rest)

    if big_cmp(rest, b) >= 0 {
      let top = if rest.len() == width {
        rest[width - 1].float() * 1000000000.0 + rest[width - 2].float()
      } else {
        (rest[width].float() * 1000000000.0 + rest[width - 1].float()) * 1000000000.0 + rest[width - 2].float()
      }
      var guess = (top / divisor_top).floor() ?? 0

      guess = if guess >= BASE { BASE - 1 } else { guess }

      var product = big_mul_small(b, guess)

      while big_cmp(product, rest) > 0 {
        guess -= 1
        product = big_sub(product, b)
      }

      var left = big_sub(rest, product)

      while big_cmp(left, b) >= 0 {
        guess += 1
        left = big_sub(left, b)
      }

      quotient[at] = guess
      rest = left
    }

    at -= 1
  }

  {quotient: trim_limbs(quotient), remainder: rest}
}


type Whole = {neg: Bool, mag: List[Int]}

# `-?digits`, normalized so that zero is never negative.
pure whole_from(text: Str) -> Whole {
  let neg = text.starts_with("-")
  let mag = big_from(if neg { text.byte_slice(1) } else { text })

  {neg: neg and mag.len() > 0, mag: mag}
}

pure whole_text(value: Whole) -> Str {
  if value.neg { f"-{big_text(value.mag)}" } else { big_text(value.mag) }
}

# Whether digits (and a sign) fit a signed 128-bit integer.
pure fits_i128(value: Whole) -> Bool {
  let limit = big_from(if value.neg { "170141183460469231731687303715884105728" } else { "170141183460469231731687303715884105727" })

  big_cmp(value.mag, limit) <= 0
}

# One parsed number: an exact integer (as far as 128 bits go) or a float.
type Num = {exact: Bool, whole: Whole, value: Float}

const MAX_I128_DIGITS = 39

pure float_text(text: Str) -> Float {
  text.parse_float() ?? 0.0
}

pure exact_num(value: Whole) -> Num {
  {exact: true, whole: value, value: float_text(whole_text(value))}
}

pure float_num(value: Float) -> Num {
  {exact: false, whole: {neg: false, mag: []}, value: value}
}

# The unit scales: 1000^k or 1024^k for k in 0..=10 (up to 1e30).
pure scale_bases(iec: Bool) -> List[Float] {
  var out: List[Float] = []
  var current = 1.0

  repeat 11 times {
    out += [current]
    current = current * (if iec { 1024.0 } else { 1000.0 })
  }

  out
}

pure suffix_letters() -> List[Str] {
  ["K", "M", "G", "T", "P", "E", "Z", "Y", "R", "Q"]
}

# C-style escaping for a name in a message: control characters, backslash and
# the single quote become escapes.
pure c_escape(text: Str) -> Str {
  var out = ""

  for index in range(text.count_chars()) {
    let char = text[index..index + 1]

    if char == "\n" {
      out += "\\n"
    } else if char == "\t" {
      out += "\\t"
    } else if char == "\\" {
      out += "\\\\"
    } else if char == "'" {
      out += "\\'"
    } else {
      out += char
    }
  }

  out
}

pure ffloor(x: Float) -> Float {
  return x when x >= 4503599627370496.0 or x <= -4503599627370496.0
  return x when x != x

  (x.floor() ?? 0).float()
}

pure fceil(x: Float) -> Float {
  return x when x >= 4503599627370496.0 or x <= -4503599627370496.0
  return x when x != x

  let rounded = (x.ceil() ?? 0).float()

  if rounded == 0.0 and x < 0.0 { 0.0 * -1.0 } else { rounded }
}

pure fround(x: Float) -> Float {
  return x when x >= 4503599627370496.0 or x <= -4503599627370496.0
  return x when x != x

  let rounded = (x.round() ?? 0).float()

  if rounded == 0.0 and x < 0.0 { 0.0 * -1.0 } else { rounded }
}

# The value rounded to a whole number by METHOD.
pure round_method(method: Str, x: Float) -> Float {
  if method == "up" {
    fceil(x)
  } else if method == "down" {
    ffloor(x)
  } else if method == "from-zero" {
    if x < 0.0 { ffloor(x) } else { fceil(x) }
  } else if method == "towards-zero" {
    if x < 0.0 { fceil(x) } else { ffloor(x) }
  } else {
    fround(x)
  }
}

pure round_with_precision(x: Float, method: Str, precision: Int) -> Float {
  let scale = 10.0.pow(precision.float())

  return x when scale == scale * 2.0 or scale != scale

  round_method(method, scale * x) / scale
}

pure div_round(n: Float, d: Float, method: Str) -> Float {
  let v = n / d

  if v.abs() < 10.0 { round_method(method, 10.0 * v) / 10.0 } else { round_method(method, v) }
}

# Rust's `{}` for a float: whole values print without a point.
pure float_display(x: Float) -> Str {
  f"{x}"
}

# GNU's `%g`-style scientific form (six digits, trimmed zeros, signed exponent).
pure scientific(x: Float) -> Str {
  return "0e+0" when x == 0.0

  var exponent = 0
  var mantissa = x.abs()

  while mantissa >= 10.0 {
    mantissa = mantissa / 10.0
    exponent += 1
  }

  while mantissa < 1.0 {
    mantissa = mantissa * 10.0
    exponent -= 1
  }

  var digits = mantissa.format(5)

  if digits.starts_with("10") {
    digits = "1.00000"
    exponent += 1
  }

  var end = digits.byte_len()

  while end > 0 and digits.byte_slice(end - 1, length: 1) == "0" {
    end -= 1
  }

  if end > 0 and digits.byte_slice(end - 1, length: 1) == "." {
    end -= 1
  }

  let sign = if x < 0.0 { "-" } else { "" }

  f"{sign}{digits.byte_slice(0, length: end)}e{if exponent < 0 { "-" } else { "+" }}{if exponent < 0 { -exponent } else { exponent }}"
}

# Locale separators: the decimal point and the digit grouping string.
type Separators = {decimal: Str, grouping: Str}

proc locale_separators() [env] -> Separators {
  var name = ""

  for variable in ["LC_ALL", "LC_NUMERIC", "LANG"] {
    let found = env.get_or(variable, "") ?? ""

    if found != "" {
      name = found
      break
    }
  }

  let language = name.lower()

  return {decimal: ",", grouping: "\u{202f}"} when language.starts_with("fr")
  return {decimal: ",", grouping: "."} when language.starts_with("de") or language.starts_with("es") or language.starts_with("it") or language.starts_with("nl") or language.starts_with("pt")
  return {decimal: ".", grouping: ","} when language.starts_with("en")

  {decimal: ".", grouping: ""}
}

# Insert the grouping string every three integer digits.
pure apply_grouping(text: Str, separators: Separators) -> Str {
  return text when separators.grouping == ""

  let neg = text.starts_with("-")
  let rest = if neg { text.byte_slice(1) } else { text }
  let point = rest.find(separators.decimal)
  let whole = if let at = point { rest.byte_slice(0, length: at) } else { rest }
  let fraction = if let at = point { rest.byte_slice(at) } else { "" }

  return text when whole.byte_len() < 4

  var out = ""
  for index in range(whole.byte_len()) {
    if index > 0 and (whole.byte_len() - index) % 3 == 0 {
      out += separators.grouping
    }

    out += whole.byte_slice(index, length: 1)
  }

  f"{if neg { "-" } else { "" }}{out}{fraction}"
}

pure suffix_index(letter: Str) -> Int {
  return 0 when letter == "K" or letter == "k"

  ("MGTPEZYRQ".find(letter) ?? -2) + 1
}

pure float_syntax(text: Str) -> Bool {
  rx"^-?([0-9]+\.?[0-9]*|\.[0-9]+)$".matches(text)
}

# The characters Rust's `char::is_whitespace` accepts, for a regex class.
const BLANKS = "\\s\\x{85}\\x{a0}\\x{1680}\\x{2000}-\\x{200a}\\x{2028}\\x{2029}\\x{202f}\\x{205f}\\x{3000}"

pure blank_regex(pattern: Str) -> Regex {
  regex.compile(pattern.replace("BL", BLANKS)) ?? rx"x"
}

pure trim_end(text: Str) -> Str {
  blank_regex("[BL]+$").replace(text, "")
}

pure trim_start(text: Str) -> Str {
  blank_regex("^[BL]+").replace(text, "")
}

pure is_digit(char: Str) -> Bool {
  char != "" and "0123456789".find(char) != null
}

# The numeric prefix of `s` that reads as a number, or null.
pure find_numeric_beginning(s: Str, decimal: Str) -> Str? {
  return null when s == ""
  return decimal when s.starts_with(decimal)

  var seen_decimal = false
  var at = 0
  let total = s.byte_len()

  while at < total {
    let byte = s.byte_at(at) ?? 0

    if byte == 45 and at == 0 {
      at += 1
    } else if byte >= 48 and byte <= 57 {
      at += 1
    } else if ! seen_decimal and s.byte_slice(at).starts_with(decimal) {
      seen_decimal = true
      at += decimal.byte_len()
    } else {
      let number = s.byte_slice(0, length: at).replace(decimal, ".")

      return null when ! float_syntax(number)

      return s.byte_slice(0, length: at)
    }
  }

  s
}

# The leading number of `s` with the unit suffix the unit allows, or null.
pure find_valid_number_with_suffix(s: Str, unit: Str, decimal: Str) -> Str? {
  guard let numeric = find_numeric_beginning(s, decimal) else {
    return null
  }

  return numeric when unit == "none"

  let length = numeric.byte_len()
  let suffix = s[length..length + 1]
  let after = s[length + 1..length + 2]
  let accepts_i = unit == "auto" or unit == "iec-i"

  return numeric when suffix == "" or suffix_index(suffix) < 0
  return s.byte_slice(0, length: length + 2) when after == "i" and accepts_i

  s.byte_slice(0, length: length + 1)
}

# `5 K` with a unit separator: the length of the number, separator and suffix.
pure valid_end_with_unit_separator(s: Str, valid: Str, unit: Str, separator: Str) -> Int? {
  let rest = s.byte_slice(valid.byte_len())

  return null when ! rest.starts_with(separator)

  let after = rest.byte_slice(separator.byte_len())
  let letter = after[0..1]

  return null when letter == "" or suffix_index(letter) < 0

  let is_iec = after[1..2] == "i" and (unit == "auto" or unit == "iec-i")

  valid.byte_len() + separator.byte_len() + 1 + (if is_iec { 1 } else { 0 })
}

pure valid_prefix_len(s: Str, unit: Str, separator: Str, decimal: Str) -> Int {
  guard let number = find_valid_number_with_suffix(s, unit, decimal) else {
    return 0
  }

  if separator != "" and number == (find_numeric_beginning(s, decimal) ?? "") {
    return valid_end_with_unit_separator(s, number, unit, separator) ?? number.byte_len()
  }

  number.byte_len()
}

proc detailed_error(s: Str, unit: Str, separator: Str, decimal: Str) [env] -> Str {
  return "invalid number: ''" when s == ""

  guard let prefix = find_valid_number_with_suffix(s, unit, decimal) else {
    return f"invalid number: {gnu.quote(s)}"
  }

  return f"invalid suffix in input: {gnu.quote(s)}" when prefix == "."
  return f"invalid number: {gnu.quote(s)}" when prefix.ends_with(".")

  let valid = s.byte_slice(0, length: valid_prefix_len(s, unit, separator, decimal))

  if valid != s and float_syntax(valid.replace(decimal, ".")) {
    let next = s[valid.byte_len()..valid.byte_len() + 1]

    return f"invalid suffix in input: {gnu.quote(s)}" when next == "+" or next == "-"
    return f"rejecting suffix in input: '{valid}{s.byte_slice(valid.byte_len())}' (consider using --from)" when next != "" and suffix_index(next) >= 0

    return f"invalid suffix in input: {gnu.quote(s)}"
  }

  if valid != s {
    let trailing = trim_start(s.byte_slice(valid.byte_len()))

    return f"invalid suffix in input {gnu.quote(s)}: {gnu.quote(trailing)}"
  }

  ""
}

type NumberPart = {num: Num, err: Str}

proc parse_number_part(s: Str, input: Str, decimal: Str) [env] -> NumberPart {
  let none = float_num(0.0)

  return {num: none, err: f"invalid number: {gnu.quote(input)}"} when s.ends_with(decimal) or s.starts_with("+")
  return {num: none, err: f"invalid suffix in input: {gnu.quote(input)}"} when s.find("e") != null or s.find("E") != null

  if rx"^-?[0-9]+$".matches(s) {
    let whole = whole_from(s)

    return {num: exact_num(whole), err: ""} when fits_i128(whole)
  }

  return {num: none, err: f"invalid number: {gnu.quote(input)}"} when decimal != "." and s.find(".") != null

  let normalized = if decimal == "." { s } else { s.replace(decimal, ".") }

  return {num: none, err: f"invalid number: {gnu.quote(input)}"} when ! float_syntax(normalized)

  {num: float_num(float_text(normalized)), err: ""}
}

type SuffixParse = {num: Num, suffix: Int, with_i: Bool, err: Str}

proc parse_suffix(s: Str, unit: Str, separator: Str, explicit: Bool, decimal: Str) [env] -> SuffixParse {
  let none = float_num(0.0)
  let trimmed = trim_end(s)

  return {num: none, suffix: -1, with_i: false, err: "invalid number: ''"} when trimmed == ""

  let with_i = trimmed.ends_with("i")

  return {num: none, suffix: -1, with_i: with_i, err: f"invalid suffix in input: {gnu.quote(s)}"} when with_i and unit != "auto" and unit != "iec-i"

  let body = if with_i { trimmed.byte_slice(0, length: trimmed.byte_len() - 1) } else { trimmed }
  let last = if body == "" { "" } else { body[body.count_chars() - 1..body.count_chars()] }
  let suffix = if last == "" { -1 } else { suffix_index(last) }

  return {num: none, suffix: -1, with_i: with_i, err: f"invalid number: {gnu.quote(s)}"} when suffix < 0 and (last == "" or ! is_digit(last) or with_i)

  let number_part = if suffix >= 0 { body[..body.count_chars() - 1] } else { body }

  if suffix >= 0 {
    var cut = 0

    if explicit {
      if number_part.ends_with(separator) {
        cut = separator.byte_len()
      } else if separator != "" {
        return {num: none, suffix: -1, with_i: with_i, err: f"invalid suffix in input: {gnu.quote(s)}"}
      }
    } else {
      cut = number_part.byte_len() - trim_end(number_part).byte_len()

      return {num: none, suffix: -1, with_i: with_i, err: f"invalid suffix in input: {gnu.quote(s)}"} when cut > 1
    }

    let parsed = parse_number_part(number_part.byte_slice(0, length: number_part.byte_len() - cut), s, decimal)

    return {num: parsed.num, suffix: suffix, with_i: with_i, err: parsed.err}
  }

  let parsed = parse_number_part(number_part, s, decimal)

  {num: parsed.num, suffix: -1, with_i: false, err: parsed.err}
}

type Scaled = {value: Float, err: Str}

proc remove_suffix(value: Float, suffix: Int, with_i: Bool, unit: Str) [env] -> Scaled {
  return {value: value, err: ""} when suffix < 0

  let index = suffix + 1
  let letter = suffix_letters()[suffix]

  if ! with_i and (unit == "auto" or unit == "si") {
    return {value: value * scale_bases(false)[index], err: ""}
  } else if (! with_i and unit == "iec") or (with_i and (unit == "auto" or unit == "iec-i")) {
    return {value: value * scale_bases(true)[index], err: ""}
  } else if ! with_i and unit == "iec-i" {
    return {value: value, err: f"missing 'i' suffix in input: '{float_display(value)}{letter}' (e.g Ki/Mi/Gi)"}
  } else if unit == "none" {
    return {value: value, err: f"rejecting suffix in input: '{float_display(value)}{letter}{if with_i { "i" } else { "" }}' (consider using --from)"}
  }

  {value: value, err: "This suffix is unsupported for specified unit"}
}

type FromUnits = {from: Str, from_unit: Int, separator: Str, explicit: Bool, decimal: Str}

proc transform_from(s: Str, units: FromUnits) [env] -> NumberPart {
  let parsed = parse_suffix(s, units.from, units.separator, units.explicit, units.decimal)

  if parsed.err != "" {
    let detail = detailed_error(s, units.from, units.separator, units.decimal)

    return {num: parsed.num, err: if detail != "" { detail } else { parsed.err }}
  }

  let had_no_suffix = parsed.suffix < 0

  if had_no_suffix and parsed.num.exact {
    let scaled = {neg: parsed.num.whole.neg, mag: big_mul(parsed.num.whole.mag, big_from(f"{units.from_unit}"))}

    return {num: exact_num(scaled), err: ""} when fits_i128(scaled)
  }

  let scaled = remove_suffix(parsed.num.value * units.from_unit.float(), parsed.suffix, parsed.with_i, units.from)

  return {num: float_num(0.0), err: scaled.err} when scaled.err != ""

  let value = scaled.value
  let adjusted = if units.from == "none" or had_no_suffix {
    if value == 0.0 { 0.0 } else { value }
  } else if value < 0.0 {
    0.0 - fceil(value.abs())
  } else {
    fceil(value)
  }

  {num: float_num(adjusted), err: ""}
}

type Suffixed = {value: Float, suffix: Str, err: Str}

# Scale `n` to the unit it belongs in: the value and its suffix letter (with
# `i` for iec-i), or the error for a number past the last unit.
proc consider_suffix(n: Float, unit: Str, method: Str, precision: Int) -> Suffixed {
  return {value: n, suffix: "", err: ""} when unit == "none"

  let iec = unit == "iec" or unit == "iec-i"
  let bases = scale_bases(iec)
  let size = n.abs()

  return {value: n, suffix: "", err: ""} when size <= bases[1] - 1.0

  var index = 1

  while index < 10 and size >= bases[index + 1] {
    index += 1
  }

  return {value: n, suffix: "", err: "Number is too big and unsupported"} when index == 10 and size >= bases[10] * 1000.0

  let effective = if iec and precision > 3 { 3 } else { precision }
  let scaled = if precision > 0 { round_with_precision(n / bases[index], method, effective) } else { div_round(n, bases[index], method) }
  let letters = suffix_letters()
  let tail = if unit == "iec-i" { "i" } else { "" }

  if scaled.abs() >= bases[1] {
    return {value: n, suffix: "", err: "Number is too big and unsupported"} when index == 10

    return {value: scaled / bases[1], suffix: letters[index] + tail, err: ""}
  }

  let letter = if unit == "si" and index == 1 { "k" } else { letters[index - 1] }

  {value: scaled, suffix: letter + tail, err: ""}
}

type Rendered = {text: Str, err: Str}
type ByteRendered = {text: Bytes, err: Str}

proc to_unit_text(num: Num, to: Str, to_unit: Int, method: Str, precision: Int, separator: Str, specified: Bool, decimal: Str) [env] -> Rendered {
  # A whole number that divides evenly prints exactly, whatever its size.
  if to == "none" and num.exact {
    let unit = big_from(f"{to_unit}")
    let division = big_divmod(num.whole.mag, unit)

    if division.remainder.len() == 0 {
      let scaled = {neg: num.whole.neg and division.quotient.len() > 0, mag: division.quotient}
      let magnitude = if scaled.mag.len() == 0 { [1] } else { scaled.mag }
      var power = [1]

      repeat if precision > 19 { 19 } else { precision } times {
        power = big_mul_small(power, 10)
      }

      if big_cmp(big_mul(magnitude, power), big_from("10000000000000000000")) >= 0 {
        return {text: "", err: f"value/precision too large to be printed: '{scientific(float_text(whole_text(scaled)))}/{precision}' (consider using --to)"}
      }

      let digits = whole_text(scaled)

      return {text: if precision == 0 { digits } else { f"{digits}{decimal}{zeros(precision)}" }, err: ""}
    }
  }

  let s = num.value
  let scaled = consider_suffix(s / to_unit.float(), to, method, precision)

  return {text: "", err: scaled.err} when scaled.err != ""

  let i2 = scaled.value
  let tail = scaled.suffix
  let wide = precision <= 65535

  var text = ""

  if tail == "" and to == "none" and wide {
    text = round_with_precision(i2, method, precision).format(precision)
  } else if tail == "" and specified and wide {
    text = round_with_precision(i2, method, 0).format(precision)
  } else if tail == "" {
    text = i2.format(0)
  } else if precision > 0 and wide {
    text = f"{i2.format(precision)}{separator}{tail}"
  } else if specified {
    text = f"{i2.format(0)}{separator}{tail}"
  } else if i2.abs() < 10.0 {
    text = f"{i2.format(1)}{separator}{tail}"
  } else {
    text = f"{i2.format(0)}{separator}{tail}"
  }

  {text: if decimal == "." { text } else { text.replace(".", decimal) }, err: ""}
}

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

# The parts of a --format string.
type Format = {
  grouping: Bool,
  padding: Int?,
  precision: Int?,
  prefix: Str,
  suffix: Str,
  zero_padding: Bool,
}

type FormatParse = {format: Format, err: Str}

pure plain_format() -> Format {
  {grouping: false, padding: null, precision: null, prefix: "", suffix: "", zero_padding: false}
}

# `[PREFIX]%[0]['][-][N][.][N]f[SUFFIX]`; a `%%` in the prefix is one `%`, and
# in the suffix stays `%%`, as GNU's does.
pure parse_format(text: Str) -> FormatParse {
  let quoted = f"'{c_escape(text)}'"
  let total = text.count_chars()
  var at = 0
  var prefix = ""
  var doubles = 0
  var format = plain_format()

  while at < total {
    let char = text[at..at + 1]

    if char == "%" and at + 1 < total and text[at + 1..at + 2] == "%" {
      prefix += "%%"
      doubles += 1
      at += 2
    } else if char == "%" {
      at += 1
      break
    } else {
      prefix += char
      at += 1
    }
  }

  repeat doubles times {
    prefix = prefix[..prefix.count_chars() - 1]
  }

  if at >= total {
    return {format: format, err: if prefix == text { f"format {quoted} has no % directive" } else { f"format {quoted} ends in %" }}
  }

  var grouping = false
  var zero = false

  while at < total and text[at..at + 1] in [" ", "'", "0"] {
    let flag = text[at..at + 1]

    if flag == "'" {
      grouping = true
    } else if flag == "0" {
      zero = true
    }

    at += 1
  }

  let directive = f"invalid format {quoted}, directive must be %[0]['][-][N][.][N]f"
  var padding = ""

  if at < total and text[at..at + 1] == "-" {
    at += 1

    return {format: format, err: directive} when at >= total or ! is_digit(text[at..at + 1])

    padding = "-"
  }

  while at < total and is_digit(text[at..at + 1]) {
    padding += text[at..at + 1]
    at += 1
  }

  var width: Int? = null

  if padding != "" {
    guard let value = padding.parse_int() else {
      return {format: format, err: f"invalid format {quoted} (width overflow)"}
    }

    width = value
  }

  var precision: Int? = null

  if at < total and text[at..at + 1] == "." {
    at += 1

    return {format: format, err: f"invalid precision in format {quoted}"} when at < total and text[at..at + 1] in [" ", "+", "-"]

    var digits = ""

    while at < total and is_digit(text[at..at + 1]) {
      digits += text[at..at + 1]
      at += 1
    }

    if digits == "" {
      precision = 0
    } else {
      guard let value = digits.parse_int() else {
        return {format: format, err: f"invalid precision in format {quoted}"}
      }

      precision = value
    }
  }

  return {format: format, err: directive} when at >= total or text[at..at + 1] != "f"

  at += 1

  var suffix = ""

  while at < total {
    let char = text[at..at + 1]

    if char != "%" {
      suffix += char
      at += 1
    } else if at + 1 < total and text[at + 1..at + 2] == "%" {
      suffix += "%%"
      at += 2
    } else {
      return {format: format, err: f"format {quoted} has too many % directives"}
    }
  }

  {format: {grouping: grouping, padding: width, precision: precision, prefix: prefix, suffix: suffix, zero_padding: zero}, err: ""}
}

# Everything the conversion needs, from the options.
type Settings = {
  from: Str,
  to: Str,
  from_unit: Int,
  to_unit: Int,
  padding: Int,
  header: Int,
  lows: List[Int],
  highs: List[Int],
  delimiter: Str?,
  round: Str,
  suffix: Str?,
  separator: Str,
  explicit_separator: Bool,
  grouping: Bool,
  format: Format,
  invalid: Str,
  zero: Bool,
  debug: Bool,
  separators: Separators,
}

pure pad_string(text: Str, width: Int, fill: Str, right: Bool) -> Str {
  let size = text.count_chars()

  return text when size >= width

  let padding = if fill == "0" { zeros(width - size) } else { zeros(width - size).replace("0", fill) }

  if right { padding + text } else { text + padding }
}

pure parse_implicit_precision(s: Str, decimal: Str) -> Int {
  let point = s.find(decimal)

  return 0 when point == null

  let rest = s.byte_slice((point ?? 0) + decimal.byte_len())
  var count = 0

  while count < rest.byte_len() and is_digit(rest.byte_slice(count, length: 1)) {
    count += 1
  }

  count
}

pure last_is_alphabetic(s: Str) -> Bool {
  return false when s == ""

  rx"[A-Za-z\x{c0}-\x{10ffff}]$".matches(s)
}

proc format_string(source: Str, settings: Settings, implicit: Int?) [env] -> Rendered {
  let stripped = if settings.suffix != null and source.ends_with(settings.suffix ?? "") { source.byte_slice(0, length: source.byte_len() - (settings.suffix ?? "").byte_len()) } else { source }
  let decimal = settings.separators.decimal
  var specified = true
  var precision = 0

  if let given = settings.format.precision {
    precision = given
  } else if settings.to == "none" and ! last_is_alphabetic(stripped) {
    precision = parse_implicit_precision(stripped, decimal)
  } else {
    specified = false
  }

  let units = {from: settings.from, from_unit: settings.from_unit, separator: settings.separator, explicit: settings.explicit_separator, decimal: decimal}
  let parsed = transform_from(stripped, units)

  return {text: "", err: parsed.err} when parsed.err != ""

  let rendered = to_unit_text(parsed.num, settings.to, settings.to_unit, settings.round, precision, settings.separator, specified, decimal)

  return rendered when rendered.err != ""

  let grouped = if settings.grouping { apply_grouping(rendered.text, settings.separators) } else { rendered.text }
  let user_suffix = settings.suffix ?? ""
  let with_suffix = grouped + user_suffix
  let padding = settings.format.padding ?? implicit ?? settings.padding
  let base = implicit ?? settings.padding
  var padded = with_suffix

  if padding == 0 {
    padded = with_suffix
  } else if padding > 0 and settings.format.zero_padding {
    let scaled = if user_suffix != "" and with_suffix.ends_with(user_suffix) { with_suffix.byte_slice(0, length: with_suffix.byte_len() - user_suffix.byte_len()) } else { with_suffix }
    let tail_suffix = if user_suffix != "" and with_suffix.ends_with(user_suffix) { user_suffix } else { "" }
    let pieces = rx"(?s)^(.*[0-9])(.*)$".captures(scaled)
    let number = if pieces.len() == 0 { scaled } else { pieces[1] }
    let unit = if pieces.len() == 0 { "" } else { pieces[2] }
    let trailing = unit + tail_suffix
    var zero_padded = ""

    if number.starts_with("-") or number.starts_with("+") {
      zero_padded = number.byte_slice(0, length: 1) + pad_string(number.byte_slice(1), padding - 1, "0", true) + trailing
    } else {
      zero_padded = pad_string(number, padding, "0", true) + trailing
    }

    if base == 0 {
      padded = zero_padded
    } else if base > 0 {
      padded = pad_string(zero_padded, base, " ", true)
    } else {
      padded = pad_string(zero_padded, -base, " ", false)
    }
  } else if padding > 0 {
    padded = pad_string(with_suffix, padding, " ", true)
  } else {
    padded = pad_string(with_suffix, -padding, " ", false)
  }

  {text: f"{settings.format.prefix}{padded}{settings.format.suffix}", err: ""}
}

pure field_selected(settings: Settings, n: Int) -> Bool {
  for index in range(settings.lows.len()) {
    if n >= settings.lows[index] and n <= settings.highs[index] {
      return true
    }
  }

  false
}

type Split = {prefix: Str, field: Str, rest: Str}

pure split_next_field(s: Str) -> Split {
  let parts = blank_regex("(?s)^([BL]*)([^BL]*)(.*)$").captures(s)

  {prefix: parts[1], field: parts[2], rest: parts[3]}
}

# With an explicit blank unit separator a suffix may be a field of its own:
# the separator and the suffix field, or null.
pure mergeable_suffix(rest: Str, settings: Settings) -> Split? {
  return null when ! settings.explicit_separator or settings.separator == "" or ! blank_regex("^[BL]+$").matches(settings.separator)
  return null when ! rest.starts_with(settings.separator)

  let split = split_next_field(rest)

  return null when split.prefix != settings.separator

  let letter = split.field[0..1]

  return null when letter == "" or suffix_index(letter) < 0
  return null when split.field.byte_len() != 1 and ! (split.field.byte_len() == 2 and split.field.ends_with("i"))

  split
}

proc format_whitespace(line: Str, settings: Settings) [env] -> Rendered {
  var haystack = line
  var out = ""
  var n = 0
  var more = true

  while more {
    let split = split_next_field(haystack)
    var field = split.field
    var rest = split.rest

    n += 1

    if field == "" {
      more = false
    } else {
      let extra = mergeable_suffix(rest, settings)

      if let merged = extra {
        field = field + merged.prefix + merged.field
        rest = merged.rest
      }

      if rest == "" {
        more = false
      }
    }

    var prefix = split.prefix

    if field_selected(settings, n) {
      let empty_prefix = prefix == ""

      if n > 1 {
        out += " "
        prefix = prefix.byte_slice(if prefix == "" { 0 } else { prefix[0..1].byte_len() })
      }

      let implicit = if ! empty_prefix and settings.padding == 0 { prefix.byte_len() + field.byte_len() } else { -1 }
      let formatted = format_string(field, settings, if implicit >= 0 { implicit } else { null })

      return {text: out, err: formatted.err} when formatted.err != ""

      out += formatted.text
    } else {
      if settings.zero and prefix.starts_with("\n") {
        out += " "
        prefix = prefix.byte_slice(1)
      }

      out += prefix + field
    }

    haystack = rest
  }

  {text: out, err: ""}
}

proc format_delimited(line: Str, delimiter: Str, settings: Settings) [env] -> Rendered {
  var out = ""
  var n = 0

  let pieces = if delimiter == "" { [line] } else { line.split(delimiter) }

  for field in pieces {
    n += 1

    if n > 1 {
      out += delimiter
    }

    if field_selected(settings, n) {
      let formatted = format_string(trim_start(field), settings, null)

      return {text: out, err: formatted.err} when formatted.err != ""

      out += formatted.text
    } else {
      out += field
    }
  }

  {text: out, err: ""}
}

# The first `e` or `E` followed by a digit marks scientific notation.
pure is_scientific(line: Str) -> Bool {
  let at = rx"[eE]".find(line)

  return false when at.len() == 0

  is_digit(line.byte_slice(at[0].end, length: 1))
}

# Escape control and non-text bytes as GNU does in a message.
pure escape_line(raw: Bytes) -> Str {
  var out = ""

  for index in range(raw.len()) {
    let byte = raw.byte_at(index) ?? 0

    if byte >= 128 or (byte < 32 and byte != 9 and byte != 10 and byte != 13 and byte != 32) or byte == 127 {
      out += f"\\{byte / 64}{byte / 8 % 8}{byte % 8}"
    } else {
      out += (bytes.from_ints([byte]) ?? b"").utf8() ?? ""
    }
  }

  out
}

# One input line (without its terminator): the output text so far and the
# error that stopped it, if any.
proc convert_line(raw: Bytes, settings: Settings) [env] -> Rendered {
  var cut = raw.len()

  for index in range(raw.len()) {
    if raw.byte_at(index) == 0 {
      cut = index
      break
    }
  }

  guard let text = raw[..cut].utf8() else {
    return {text: "", err: f"invalid number: {gnu.quote(escape_line(raw[..cut]))}"}
  }

  if let delimiter = settings.delimiter {
    return format_delimited(text, delimiter, settings)
  }

  return {text: "", err: f"invalid suffix in input: {gnu.quote(text)}"} when is_scientific(text)

  format_whitespace(text, settings)
}

# A unit size: digits with an optional K/M/G/T/P/E (powers of 1000) or
# Ki/Mi/... (powers of 1024) multiplier, at least 1 and below 2^63.
pure parse_unit_size(text: Str) -> Int? {
  let parts = rx"^([0-9]*)(.*)$".captures(text)
  let digits = parts[1]
  let suffix = parts[2]

  return null when digits != "" and rx"^0+$".matches(digits)

  var multiplier = 1

  if suffix != "" {
    let index = "KMGTPE".find(suffix.byte_slice(0, length: 1)) ?? -1

    return null when index < 0 or ! (suffix.byte_len() == 1 or (suffix.byte_len() == 2 and suffix.ends_with("i")))

    repeat index + 1 times {
      multiplier = multiplier * (if suffix.byte_len() == 2 { 1024 } else { 1000 })
    }
  }

  return multiplier when digits == ""
  return null when digits.byte_len() > 18

  let number = digits.parse_int() ?? 0

  return null when number > 9223372036854775807 / multiplier

  number * multiplier
}

type RangeParse = {low: Int, high: Int, err: Str}

pure parse_range(item: Str) -> RangeParse {
  let top = 9223372036854775806
  let none = {low: 0, high: 0}

  let split = item.find("-")

  if split == null {
    return bound_error(item)
  }

  let low = item.byte_slice(0, length: split ?? 0)
  let high = item.byte_slice((split ?? 0) + 1)

  return {...none, err: "invalid range with no endpoint"} when low == "" and high == ""

  if high == "" {
    let bound = bound_value(low)

    return {...none, err: bound.err} when bound.err != ""

    return {low: bound.value, high: top, err: ""}
  }

  if low == "" {
    let bound = bound_value(high)

    return {...none, err: bound.err} when bound.err != ""

    return {low: 1, high: bound.value, err: ""}
  }

  let first = bound_value(low)
  let last = bound_value(high)

  return {...none, err: first.err} when first.err != ""
  return {...none, err: last.err} when last.err != ""
  return {...none, err: "high end of range less than low end"} when first.value > last.value

  {low: first.value, high: last.value, err: ""}
}

type Bound = {value: Int, err: Str}

pure bound_value(part: Str) -> Bound {
  return {value: 0, err: "failed to parse range"} when ! rx"^[0-9]+$".matches(part)
  return {value: 0, err: "byte/character offset is too large"} when part.byte_len() > 18 and ! rx"^0+$".matches(part)

  let value = part.parse_int() ?? 0

  return {value: 0, err: "fields and positions are numbered from 1"} when value == 0

  {value: value, err: ""}
}

pure bound_error(item: Str) -> RangeParse {
  let bound = bound_value(item)

  {low: bound.value, high: bound.value, err: bound.err}
}

# The unique choice among `names` that `value` abbreviates, `?` for several,
# or "" for none.
pure choose(value: Str, names: List[Str]) -> Str {
  return value when value in names

  let matches = [name for name in names if value != "" and name.starts_with(value)]

  if matches.len() == 1 { matches[0] } else if matches.len() == 0 { "" } else { "?" }
}

proc argument_error(value: Str, option: Str, names: List[Str]) [process, env] -> Unit {
  gnu.error(f"{if choose(value, names) == "?" { "ambiguous" } else { "invalid" }} argument {gnu.quote(value)} for {gnu.quote(option)}")
  eprint "Valid arguments are:"

  for name in names {
    eprint f"  - {gnu.quote(name)}"
  }

  gnu.try_help()
  exit 1
}

proc option_error(message: Str) [process, env] -> Unit {
  gnu.error(message)
  exit 1
}

proc unit_option(value: Str, option: Str, allow_auto: Bool) [process, env] -> Str {
  let names = if allow_auto { ["auto", "si", "iec", "iec-i", "none"] } else { ["si", "iec", "iec-i", "none"] }

  if ! (value in names) {
    gnu.error(f"invalid argument '{value}' for '--{option}'")
    exit 1
  }

  value
}

proc settings_from(opts: NumfmtOptions, byte_delimiter: Bool) [process, env, io] -> Settings {
  let from = unit_option(opts.from, "from", true)
  let to = unit_option(opts.to, "to", false)
  let from_unit = parse_unit_size(opts.from_unit)
  let to_unit = parse_unit_size(opts.to_unit)

  if from_unit == null {
    option_error(f"invalid unit size: {gnu.quote(opts.from_unit)}")
  }

  if to_unit == null {
    option_error(f"invalid unit size: {gnu.quote(opts.to_unit)}")
  }

  var padding = 0

  if let given = opts.padding {
    let value = given.parse_int() ?? 0

    if ! rx"^-?[0-9]+$".matches(given) or value == 0 {
      option_error(f"invalid padding value {gnu.quote(given)}")
    }

    padding = value
  }

  var header = 0

  if opts.header != "" {
    let value = opts.header.parse_int() ?? 0

    if ! rx"^[0-9]+$".matches(opts.header) or value == 0 {
      option_error(f"invalid header value {gnu.quote(opts.header)}")
    }

    header = value
  }

  var lows: List[Int] = []
  var highs: List[Int] = []

  if "-" in opts.field.replace(" ", ",").split(",") {
    lows = [1]
    highs = [9223372036854775806]
  } else {
    for item in opts.field.replace(" ", ",").split(",") {
      let range = parse_range(item)

      if range.err != "" {
        option_error(f"range {gnu.quote(item)} was invalid: {range.err}")
      }

      lows += [range.low]
      highs += [range.high]
    }
  }

  let separators = locale_separators()
  var format = plain_format()

  if let text = opts.format {
    let parsed = parse_format(text)

    if parsed.err != "" {
      option_error(parsed.err)
    }

    format = parsed.format
  }

  if opts.grouping and opts.format != null {
    option_error("--grouping cannot be combined with --format")
  }

  let grouping = opts.grouping or format.grouping

  if grouping and to != "none" {
    option_error("grouping cannot be combined with --to")
  }

  if let delimiter = opts.delimiter {
    if delimiter.count_chars() > 1 and ! byte_delimiter {
      option_error("the delimiter must be a single character")
    }
  }

  let round = choose(opts.round, ["up", "down", "from-zero", "towards-zero", "nearest"])

  if round == "" or round == "?" {
    argument_error(opts.round, "--round", ["up", "down", "from-zero", "towards-zero", "nearest"])
  }

  let invalid = choose(opts.invalid, ["abort", "fail", "warn", "ignore"])

  if invalid == "" or invalid == "?" {
    argument_error(opts.invalid, "--invalid", ["abort", "fail", "warn", "ignore"])
  }

  {
    from: from,
    to: to,
    from_unit: from_unit ?? 1,
    to_unit: to_unit ?? 1,
    padding: padding,
    header: header,
    lows: lows,
    highs: highs,
    delimiter: opts.delimiter,
    round: round,
    suffix: opts.suffix,
    separator: opts.unit_separator ?? "",
    explicit_separator: opts.unit_separator != null,
    grouping: grouping,
    format: format,
    invalid: invalid,
    zero: opts.zero,
    debug: opts.debug,
    separators: separators,
  }
}

# The pinned uutils tests use `-d=CHAR` as a separator spelling, while GNU
# treats `=CHAR` as the attached value. Normalize this nonempty short form to
# match the tested uutils interface; `-d=` still means the delimiter `=`.
pure normalize_delimiter_option(argv: List[Str]) -> List[Str] {
  var normalized: List[Str] = []

  for item in argv {
    if item.starts_with("-d=") and item.byte_len() > 3 {
      normalized += ["-d" + item.byte_slice(3)]
    } else {
      normalized += [item]
    }
  }

  normalized
}

pure long_name_matches(name: Str, full: Str) -> Bool {
  name != "" and full.starts_with(name)
}

pure option_takes_value(name: Str) -> Bool {
  long_name_matches(name, "delimiter") or long_name_matches(name, "field") or
    long_name_matches(name, "format") or long_name_matches(name, "from") or
    long_name_matches(name, "from-unit") or long_name_matches(name, "invalid") or
    long_name_matches(name, "padding") or long_name_matches(name, "round") or
    long_name_matches(name, "suffix") or long_name_matches(name, "to") or
    long_name_matches(name, "to-unit") or long_name_matches(name, "unit-separator")
}

# Preserve argv bytes for the option value even though cli.applet also exposes
# the lossy Str view used for parsing and diagnostics.
pure raw_delimiter(argv: List[Str], raw: List[Bytes], fallback: Bytes) -> Bytes {
  var selected = fallback
  var at = 0

  while at < argv.len() {
    let item = argv[at]

    if item.starts_with("--") {
      let equal_at = item.find("=")
      let name = if equal_at == null { item.byte_slice(2) } else { item.byte_slice(2, length: (equal_at ?? 0) - 2) }

      if long_name_matches(name, "delimiter") {
        if equal_at == null {
          if at + 1 < raw.len() { selected = raw[at + 1] }
          at += 2
        } else {
          selected = raw[at].slice((equal_at ?? 0) + 1)
          at += 1
        }
      } else if option_takes_value(name) and equal_at == null {
        at += 2
      } else {
        at += 1
      }
    } else if item.starts_with("-") and item != "-" {
      var pos = 1
      var consumed = false

      while pos < item.byte_len() {
        let letter = item.byte_slice(pos, length: 1)

        if letter == "d" {
          if pos + 1 < item.byte_len() {
            selected = raw[at].slice(pos + 1)
            at += 1
          } else {
            if at + 1 < raw.len() { selected = raw[at + 1] }
            at += 2
          }
          consumed = true
          break
        }

        pos += 1
      }

      if ! consumed { at += 1 }
    } else {
      at += 1
    }
  }

  selected
}

# Return only positional bytes, consuming the options whose values are not
# operands. GNU parsing has already rejected malformed or ambiguous options.
pure raw_operands(argv: List[Str], raw: List[Bytes]) -> List[Bytes] {
  var operands: List[Bytes] = []
  var at = 0
  var options = true

  while at < argv.len() {
    let item = argv[at]

    if options and item == "--" {
      options = false
      at += 1
    } else if options and item.starts_with("--") {
      let equal_at = item.find("=")
      let name = if equal_at == null { item.byte_slice(2) } else { item.byte_slice(2, length: (equal_at ?? 0) - 2) }
      at += if option_takes_value(name) and equal_at == null { 2 } else { 1 }
    } else if options and item.starts_with("-") and item != "-" {
      var pos = 1
      var takes_next = false
      var has_value = false

      while pos < item.byte_len() {
        let letter = item.byte_slice(pos, length: 1)

        if letter == "d" {
          takes_next = pos == item.byte_len() - 1
          has_value = true
          break
        }

        pos += 1
      }

      at += if has_value and takes_next { 2 } else { 1 }
    } else {
      operands += [raw[at]]
      at += 1
    }
  }

  operands
}

pure split_bytes(line: Bytes, delimiter: Bytes) -> List[Bytes] {
  return [line] when delimiter.len() == 0

  var pieces: List[Bytes] = []
  var start = 0
  var at = 0

  while at + delimiter.len() <= line.len() {
    if line[at..at + delimiter.len()] == delimiter {
      pieces += [line[start..at]]
      at += delimiter.len()
      start = at
    } else {
      at += 1
    }
  }

  pieces += [line[start..line.len()]]
  pieces
}

pure invalid_utf8(value: Bytes) -> Bool {
  match value.utf8() {
    Ok(_) => false
    Err(_) => true
  }
}

proc format_delimited_bytes(line: Bytes, delimiter: Bytes, settings: Settings) [env] -> ByteRendered {
  var out: List[Bytes] = []
  let pieces = split_bytes(line, delimiter)

  for index in range(pieces.len()) {
    let field = pieces[index]

    if index > 0 { out += [delimiter] }

    if field_selected(settings, index + 1) {
      guard let text = field.utf8() else {
        return {text: bytes.concat(out), err: f"invalid number: {gnu.quote(escape_line(field))}"}
      }

      let formatted = format_string(trim_start(text), settings, null)
      return {text: bytes.concat(out), err: formatted.err} when formatted.err != ""
      out += [bytes.from_text(formatted.text)]
    } else {
      out += [field]
    }
  }

  {text: bytes.concat(out), err: ""}
}

proc main(...argv: List[Str]) [process, env, error, io] {
  let opts: NumfmtOptions = cli.applet(
    normalize_delimiter_option(argv),
    {
      gnu: {status: 1},
      debug: {form: "--debug", default: false},
      delimiter: {form: "-d --delimiter X"},
      field: {form: "--field FIELDS", default: "1"},
      format: {form: "--format FORMAT"},
      from: {form: "--from UNIT", default: "none"},
      from_unit: {form: "--from-unit N", default: "1"},
      grouping: {form: "--grouping", default: false},
      header: {form: "--header[=N]", default: "", optional_default: "1"},
      invalid: {form: "--invalid MODE", default: "abort"},
      padding: {form: "--padding N"},
      round: {form: "--round METHOD", default: "from-zero"},
      suffix: {form: "--suffix SUFFIX"},
      to: {form: "--to UNIT", default: "none"},
      to_unit: {form: "--to-unit N", default: "1"},
      unit_separator: {form: "--unit-separator STRING"},
      zero: {form: "-z --zero-terminated", default: false},
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
    gnu.version("numfmt")
    return
  }

  let raw_argv = cli.argv_bytes()
  let delimiter_bytes = raw_delimiter(argv, raw_argv, bytes.from_text(opts.delimiter ?? ""))
  let byte_delimiter = opts.delimiter != null and invalid_utf8(delimiter_bytes)
  let settings = settings_from(opts, byte_delimiter)

  if settings.debug {
    if settings.from == "none" and settings.to == "none" and settings.padding == 0 and ! settings.grouping {
      gnu.error("no conversion option specified")
    }

    if settings.grouping and settings.separators.grouping == "" {
      gnu.error("grouping has no effect in this locale")
    }

    if settings.header > 0 and opts.numbers.len() > 0 {
      gnu.error("--header ignored with command-line input")
    }
  }

  let terminator = if settings.zero { "\0" } else { "\n" }
  var lines: List[Bytes] = []
  var ends: List[Bool] = []

  if opts.numbers.len() > 0 {
    lines = if byte_delimiter { raw_operands(argv, raw_argv) } else { [bytes.from_text(item) for item in opts.numbers] }
    ends = [true for item in opts.numbers]
  } else {
    var data = b""

    match io.stdin_bytes() {
      Ok(read) => data = read
      Err(failure) => {
        gnu.error(gnu.strerror(failure))
        exit 1
      }
    }

    var start = 0

    for stop in tio.line_ends(data, settings.zero) {
      lines += [data[start..stop - 1]]
      ends += [true]
      start = stop
    }

    if start < data.len() {
      lines += [data[start..data.len()]]
      ends += [false]
    }
  }

  if byte_delimiter {
    let terminator_bytes = bytes.from_text(if settings.zero { "\0" } else { "\n" })
    var output: List[Bytes] = []
    var failed = false
    var saw_invalid = false

    for index in range(lines.len()) {
      let raw = lines[index]
      let eol = if ends[index] { terminator_bytes } else { b"" }

      if opts.numbers.len() == 0 and index < settings.header {
        output += [raw, eol]
        continue
      }

      let result = format_delimited_bytes(raw, delimiter_bytes, settings)

      if result.err == "" {
        output += [result.text, eol]
      } else if settings.invalid == "abort" {
        output += [result.text]
        gnu.write_bytes(bytes.concat(output))
        gnu.error(result.err)
        exit 2
      } else {
        if settings.invalid == "fail" {
          gnu.error(result.err)
          failed = true
        } else if settings.invalid == "warn" {
          gnu.error(result.err)
        }

        saw_invalid = true
        output += [raw, eol]
      }
    }

    gnu.write_bytes(bytes.concat(output))

    if settings.debug and saw_invalid {
      gnu.error("failed to convert some of the input numbers")
    }

    if failed {
      exit 2
    }

    return
  }

  var out = ""
  var failed = false
  var saw_invalid = false
  var index = 0

  while index < lines.len() {
    let raw = lines[index]
    let eol = if ends[index] { terminator } else { "" }
    let from_stdin = opts.numbers.len() == 0

    if from_stdin and index < settings.header {
      out += (raw.utf8() ?? "") + eol
      index += 1
      continue
    }

    let result = convert_line(raw, settings)

    if result.err == "" {
      out += result.text + eol
    } else if settings.invalid == "abort" {
      gnu.write_text(out + result.text)
      gnu.error(result.err)
      exit 2
    } else {
      if settings.invalid == "fail" {
        gnu.error(result.err)
        failed = true
      } else if settings.invalid == "warn" {
        gnu.error(result.err)
      }

      saw_invalid = true
      out += (raw.utf8() ?? "") + eol
    }

    index += 1
  }

  gnu.write_text(out)

  if settings.debug and saw_invalid {
    gnu.error("failed to convert some of the input numbers")
  }

  if failed {
    exit 2
  }
}
