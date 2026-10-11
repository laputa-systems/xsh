#!/bin/xsh
use lib.gnu
error AppletError = Usage : Usage

type PrintfSpec = {
  end: Int,
  flags: Str,
  width: Int,
  width_dynamic: Bool,
  precision: Int?,
  precision_dynamic: Bool,
  position: Int?,
  width_position: Int?,
  precision_position: Int?,
  conversion: Str,
  valid: Bool,
}

type Escape = {data: Bytes, next: Int, stop: Bool, issue: Str?}
type Rendered = {data: Bytes, stop: Bool, failed: Bool, prefix_flushed: Bool}
type Pass = {data: Bytes, next_argument: Int, conversions: Int, stop: Bool, failed: Bool, prefix_flushed: Bool}
type PrintfArgument = {data: Bytes, text: Str}
type DecimalScan = {value: Int, next: Int}
type IntegerParse = {value: Int, issue: Str?, warning: Str?}
type FloatParse = {value: Float, issue: Str?}
type PrintfOutput = {data: Bytes, failed: Bool, next_argument: Int, stopped: Bool}

pure usage(applet_name: Str, summary: Str) -> Str {
  f"usage: xsh applets/{applet_name}.xsh -- {summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  AppletError.Usage(usage(applet_name, summary))
}

pure scan_decimal(text: Bytes, at: Int) -> DecimalScan {
  var value = 0
  var next = at

  while next < text.len() and (text.byte_at(next) ?? 0) >= 48 and (text.byte_at(next) ?? 0) <= 57 {
    let digit = (text.byte_at(next) ?? 48) - 48
    let max_int = 9223372036854775807
    value = if value > (max_int - digit) / 10 { max_int } else { value * 10 + digit }
    next += 1
  }

  {value: value, next: next}
}

pure indexed_argument(first_argument: Int, position: Int, argument_count: Int) -> Int {
  if position < 0 { -1 } else if first_argument >= argument_count or position >= argument_count - first_argument { argument_count } else { first_argument + position }
}

pure consumed_arguments(next_argument: Int, index: Int, argument_count: Int) -> Int {
  if index < 0 { next_argument } else if index >= argument_count { argument_count } else if index + 1 > next_argument { index + 1 } else { next_argument }
}

pure ascii_byte(text: Bytes, at: Int) -> Str {
  text[at..at + 1].utf8() ?? ""
}

pure parse_spec(text: Bytes, start: Int) -> PrintfSpec {
  var at = start + 1
  var position: Int? = null
  var invalid_position_end: Int? = null
  let leading = scan_decimal(text, at)
  if leading.next > at and leading.next < text.len() and ascii_byte(text, leading.next) == "$" {
    position = leading.value - 1
    if leading.value == 0 { invalid_position_end = leading.next + 1 }
    at = leading.next + 1
  }
  var flags = ""

  while at < text.len() and "-+ #0'".find(ascii_byte(text, at)) != null {
    let flag = ascii_byte(text, at)
    if flags.find(flag) == null { flags += flag }
    at += 1
  }

  var width = 0
  var width_dynamic = false
  var width_position: Int? = null
  if at < text.len() and ascii_byte(text, at) == "*" {
    width_dynamic = true
    at += 1
    let numbered = scan_decimal(text, at)
    if numbered.next > at and numbered.next < text.len() and ascii_byte(text, numbered.next) == "$" {
      width_position = numbered.value - 1
      if numbered.value == 0 { invalid_position_end = numbered.next + 1 }
      at = numbered.next + 1
    }
  } else {
    let scanned = scan_decimal(text, at)
    width = scanned.value
    at = scanned.next
  }

  var precision: Int? = null
  var precision_dynamic = false
  var precision_position: Int? = null
  if at < text.len() and ascii_byte(text, at) == "." {
    at += 1
    precision = 0
    if at < text.len() and ascii_byte(text, at) == "*" {
      precision_dynamic = true
      at += 1
      let numbered = scan_decimal(text, at)
      if numbered.next > at and numbered.next < text.len() and ascii_byte(text, numbered.next) == "$" {
        precision_position = numbered.value - 1
        if numbered.value == 0 { invalid_position_end = numbered.next + 1 }
        at = numbered.next + 1
      }
    } else {
      let scanned = scan_decimal(text, at)
      precision = scanned.value
      at = scanned.next
    }
  }

  while at < text.len() and "hlLzjt".find(ascii_byte(text, at)) != null { at += 1 }

  let conversion = if at < text.len() { ascii_byte(text, at) } else { "" }
  let known_conversion = conversion in ["d", "i", "o", "u", "x", "X", "f", "F", "e", "E", "g", "G", "a", "A", "s", "c", "b", "q"]
  let invalid_zero_flag = flags.find("0") != null and conversion in ["s", "c"]
  let invalid_conversion_flag = (flags.find("#") != null and conversion in ["c", "d", "i", "s", "u"]) or (flags.find("'") != null and conversion in ["a", "A", "c", "e", "E", "o", "s", "x", "X"])
  let invalid_character_precision = conversion == "c" and precision != null
  let invalid_quote_parameters = conversion == "q" and (flags != "" or width != 0 or width_dynamic or precision != null or precision_dynamic)
  let invalid_escape_parameters = conversion == "b" and (flags != "" or width != 0 or width_dynamic or precision != null or precision_dynamic)
  let valid = known_conversion and invalid_position_end == null and ! invalid_zero_flag and ! invalid_conversion_flag and ! invalid_character_precision and ! invalid_quote_parameters and ! invalid_escape_parameters

  {end: invalid_position_end ?? (if conversion == "" { at } else { at + 1 }), flags: flags, width: width, width_dynamic: width_dynamic, precision: precision, precision_dynamic: precision_dynamic, position: position, width_position: width_position, precision_position: precision_position, conversion: conversion, valid: valid}
}

pure digit_value(character: Str) -> Int? {
  let lower = character.lower()
  let value = "0123456789abcdef".find(lower)
  value
}

pure ascii_digit(text: Str, at: Int) -> Bool {
  let byte = text.byte_at(at) ?? 0
  byte >= 48 and byte <= 57
}

pure utf8_width(lead: Int) -> Int {
  return 2 when lead >= 194 and lead <= 223
  return 3 when lead >= 224 and lead <= 239
  return 4 when lead >= 240 and lead <= 244
  1
}

pure integer_prefix(text: Str, auto_base: Bool) -> Int {
  integer_parse(text, auto_base).value
}

pure integer_parse(text: Str, auto_base: Bool) -> IntegerParse {
  var start = 0
  while start < text.byte_len() and (text.byte_at(start) ?? 0) in [9, 10, 11, 12, 13, 32] { start += 1 }
  let trimmed = text.byte_slice(start)
  if trimmed.starts_with("'") or trimmed.starts_with("\"") {
    let character = trimmed.byte_slice(1)
    if character == "" { return {value: 0, issue: "expected a numeric value", warning: null} }
    let rest = character.byte_slice(utf8_width(character.byte_at(0) ?? 0))
    return {value: codepoint_value(character), issue: null, warning: if rest == "" { null } else { rest }}
  }
  let sign = if trimmed.starts_with("-") { -1 } else { 1 }
  var body = if trimmed.starts_with("-") or trimmed.starts_with("+") { trimmed.byte_slice(1) } else { trimmed }
  var base = 10

  if auto_base and (body.starts_with("0x") or body.starts_with("0X")) {
    base = 16
    body = body.byte_slice(2)
  } else if auto_base and (body.starts_with("0b") or body.starts_with("0B")) {
    base = 2
    body = body.byte_slice(2)
  } else if auto_base and body.starts_with("0") {
    base = 8
  }

  var value = 0
  var found = false
  var at = 0
  var overflow = false
  # The value accumulates toward the sign's limit, so -2^63 is representable
  # even though +2^63 is not.
  let limit = if sign < 0 { -9223372036854775807 - 1 } else { 9223372036854775807 }
  let cutoff = limit / base
  let cutlim = if sign < 0 { -(limit % base) } else { limit % base }

  while at < body.byte_len() {
    let digit = digit_value(body.byte_slice(at, length: 1))
    if digit == null or (digit ?? 99) >= base { break }
    found = true
    let next_digit = digit ?? 0
    if ! overflow {
      if (sign < 0 and (value < cutoff or (value == cutoff and next_digit > cutlim))) or (sign > 0 and (value > cutoff or (value == cutoff and next_digit > cutlim))) {
        overflow = true
        value = limit
      } else if sign < 0 {
        value = value * base - next_digit
      } else {
        value = value * base + next_digit
      }
    }
    at += 1
  }

  let tail = if found { body.byte_slice(at) } else { body }
  if ! found { {value: 0, issue: "expected a numeric value", warning: null} } else if overflow { {value: limit, issue: "Numerical result out of range", warning: null} } else if tail != "" { {value: value, issue: "value not completely converted", warning: null} } else { {value: value, issue: null, warning: null} }
}

pure float_parse(text: Str) -> FloatParse {
  var start = 0
  while start < text.byte_len() and (text.byte_at(start) ?? 0) in [9, 10, 11, 12, 13, 32] { start += 1 }
  let trimmed = text.byte_slice(start)
  if trimmed.starts_with("'") or trimmed.starts_with("\"") {
    let character = trimmed.byte_slice(1)
    if character == "" { return {value: 0.0, issue: "expected a numeric value"} }
    let tail = character.byte_slice(utf8_width(character.byte_at(0) ?? 0))
    return {value: codepoint_value(character).float(), issue: if tail == "" { null } else { "value not completely converted" }}
  }
  let sign_length = if trimmed.starts_with("+") or trimmed.starts_with("-") { 1 } else { 0 }
  let body = trimmed.byte_slice(sign_length)
  let lower = body.lower()
  if lower.starts_with("0x") {
    var at = 2
    var whole = 0.0
    var fraction = 0.0
    var scale = 1.0
    var digits = 0
    while at < body.byte_len() {
      let byte = body.byte_at(at) ?? 0
      let digit = if byte < 128 { digit_value(body.byte_slice(at, length: 1)) } else { null }
      if digit == null or (digit ?? 99) >= 16 { break }
      whole = whole * 16.0 + (digit ?? 0).float()
      at += 1
      digits += 1
    }
    if at < body.byte_len() and body.byte_at(at) == 46 {
      at += 1
      while at < body.byte_len() {
        let byte = body.byte_at(at) ?? 0
        let digit = if byte < 128 { digit_value(body.byte_slice(at, length: 1)) } else { null }
        if digit == null or (digit ?? 99) >= 16 { break }
        scale = scale / 16.0
        fraction += (digit ?? 0).float() * scale
        at += 1
        digits += 1
      }
    }
    var exponent = 0
    if digits > 0 and at < body.byte_len() and (body.byte_at(at) ?? 0) in [80, 112] {
      let marker = at
      at += 1
      let exponent_sign = if at < body.byte_len() and body.byte_at(at) == 45 { -1 } else { 1 }
      if at < body.byte_len() and (body.byte_at(at) ?? 0) in [43, 45] { at += 1 }
      let exponent_start = at
      while at < body.byte_len() and ascii_digit(body, at) { at += 1 }
      if at == exponent_start { at = marker } else { exponent = integer_prefix(body.byte_slice(exponent_start, length: at - exponent_start), false) * exponent_sign }
    }
    let issue: Str? = if digits == 0 or body.byte_slice(at) != "" { "value not completely converted" } else { null }
    let magnitude = (whole + fraction) * 2.0.pow(exponent.float())
    return {value: if trimmed.starts_with("-") { -magnitude } else { magnitude }, issue: issue}
  }
  if lower.starts_with("inf") or lower.starts_with("nan") {
    let end = if lower.starts_with("infinity") { sign_length + 8 } else { sign_length + 3 }
    let value_text = trimmed.byte_slice(0, length: end)
    let issue: Str? = if trimmed.byte_slice(end) == "" { null } else { "value not completely converted" }
    return {value: value_text.parse_float() ?? 0.0, issue: issue}
  }

  var at = sign_length
  var digits = 0
  while at < trimmed.byte_len() and ascii_digit(trimmed, at) { at += 1; digits += 1 }
  if at < trimmed.byte_len() and trimmed.byte_at(at) == 46 {
    at += 1
    while at < trimmed.byte_len() and ascii_digit(trimmed, at) { at += 1; digits += 1 }
  }
  if digits == 0 { return {value: 0.0, issue: "expected a numeric value"} }
  if at < trimmed.byte_len() and (trimmed.byte_at(at) ?? 0) in [69, 101] {
    let exponent_start = at
    at += 1
    if at < trimmed.byte_len() and (trimmed.byte_at(at) ?? 0) in [43, 45] { at += 1 }
    let exponent_digits = at
    while at < trimmed.byte_len() and ascii_digit(trimmed, at) { at += 1 }
    if at == exponent_digits { at = exponent_start }
  }
  let value_text = trimmed.byte_slice(0, length: at)
  let value = value_text.parse_float() ?? 0.0
  let exponent_at = value_text.find("e") ?? value_text.find("E") ?? value_text.byte_len()
  let mantissa = value_text.byte_slice(0, length: exponent_at)
  var nonzero_mantissa = false
  for index in range(mantissa.byte_len()) {
    let digit = mantissa.byte_at(index) ?? 0
    if digit >= 49 and digit <= 57 { nonzero_mantissa = true }
  }
  let out_of_range = value.format() in ["Infinity", "-Infinity"] or (value.abs() == 0.0 and nonzero_mantissa)
  let issue: Str? = if trimmed.byte_slice(at) != "" { "value not completely converted" } else if out_of_range { "Numerical result out of range" } else { null }
  {value: value, issue: issue}
}

pure codepoint_value(text: Str) -> Int {
  let raw = bytes.from_text(text)
  let first = raw.byte_at(0) ?? 0
  if first < 128 { return first }
  let width = utf8_width(first)
  let second = (raw.byte_at(1) ?? 128) - 128
  if width == 2 { return (first - 192) * 64 + second }
  let third = (raw.byte_at(2) ?? 128) - 128
  if width == 3 { return (first - 224) * 4096 + second * 64 + third }
  let fourth = (raw.byte_at(3) ?? 128) - 128
  (first - 240) * 262144 + second * 4096 + third * 64 + fourth
}

pure radix_text(value: Int, base: Int, uppercase: Bool) -> Str {
  return "0" when value == 0
  let alphabet = if uppercase { "0123456789ABCDEF" } else { "0123456789abcdef" }
  var number = value
  var output = ""

  while number > 0 {
    output = alphabet.byte_slice(number % base, length: 1) + output
    number = number / base
  }

  output
}

pure decimal_subtract(left: Str, right: Str) -> Str {
  let alphabet = "0123456789"
  var output = ""
  var borrow = 0
  var at = left.byte_len() - 1
  while at >= 0 {
    let a = alphabet.find(left.byte_slice(at, length: 1)) ?? 0
    let right_at = at - (left.byte_len() - right.byte_len())
    let b = if right_at >= 0 { alphabet.find(right.byte_slice(right_at, length: 1)) ?? 0 } else { 0 }
    var digit = a - b - borrow
    if digit < 0 { digit += 10; borrow = 1 } else { borrow = 0 }
    output = alphabet.byte_slice(digit, length: 1) + output
    at -= 1
  }
  while output.byte_len() > 1 and output.starts_with("0") { output = output.byte_slice(1) }
  output
}

pure unsigned_negative(value: Int, base: Int, uppercase: Bool) -> Str {
  let magnitude = if value == -9223372036854775807 - 1 { "9223372036854775808" } else { f"{-value}" }
  if base == 10 { return decimal_subtract("18446744073709551616", magnitude) }
  if base == 16 {
    let prior = if value == -9223372036854775807 - 1 { 9223372036854775807 } else { -value - 1 }
    let digits = pad_text(radix_text(prior, 16, false), 16, false, "0")
    let lower = "0123456789abcdef"
    let upper = "fedcba9876543210"
    var output = ""
    for at in range(16) {
      let digit = lower.find(digits.byte_slice(at, length: 1)) ?? 0
    output += upper.byte_slice(digit, length: 1)
    }
    if uppercase { output.upper() } else { output }
  } else {
    let octal = if value == -9223372036854775807 - 1 { "1000000000000000000000" } else { radix_text(-value - 1, 8, false) }
    let digits = pad_text(octal, 22, false, "0")
    var output = ""
    for at in range(22) {
      let digit = "01234567".find(digits.byte_slice(at, length: 1)) ?? 0
      let complement = if at == 0 { 1 - digit } else { 7 - digit }
      output += "01234567".byte_slice(complement, length: 1)
    }
    output
  }
}

pure pad_text(text: Str, width: Int, left: Bool, fill: Str) -> Str {
  let count = width - text.count_chars()
  return text when count <= 0
  var padding = ""
  repeat count times { padding += fill }
  if left { text + padding } else { padding + text }
}

proc write_repeat(fill: Bytes, count: Int) [process, env, io, error] {
  return when count <= 0
  let full_size = 65536
  let atoms: List[Bytes] = collect { repeat full_size times { yield fill } }
  let full = bytes.concat(atoms)
  var remaining = count
  while remaining > 0 {
    let size = if remaining < full_size { remaining } else { full_size }
    gnu.write_bytes(if size == full_size { full } else { full[0..size] })
    remaining -= size
  }
}

proc write_field(prefix: Bytes, before: Bytes, body: Bytes, after: Bytes, padding: Int, fill: Bytes, left: Bool) [process, env, io, error] {
  gnu.write_bytes(prefix)
  gnu.write_bytes(before)
  if left {
    gnu.write_bytes(body)
    gnu.write_bytes(after)
    write_repeat(fill, padding)
  } else {
    write_repeat(fill, padding)
    gnu.write_bytes(body)
    gnu.write_bytes(after)
  }
}

pure byte_character(value: Int) -> Result[Bytes] {
  bytes.from_ints([value])
}

pure lossless_text(data: Bytes) -> Result[Str] {
  var output = ""
  var at = 0
  while at < data.len() {
    let byte = data.byte_at(at) ?? 0
    let width = utf8_width(byte)
    let end = at + width
    if byte < 128 {
      output += data[at..end].utf8()?
      at = end
    } else if width > 1 and end <= data.len() {
      if let Ok(character) = data[at..end].utf8() {
        output += character
        at = end
      } else {
        output += codepoint_text(byte)?
        at += 1
      }
    } else {
      output += codepoint_text(byte)?
      at += 1
    }
  }
  output
}

pure printf_argument(data: Bytes) -> Result[PrintfArgument] {
  {data: data, text: lossless_text(data)?}
}

pure codepoint_text(value: Int) -> Result[Str] {
  if value < 128 { return bytes.from_ints([value])?.utf8() }
  if value < 2048 {
    return bytes.from_ints([192 + value / 64, 128 + value % 64])?.utf8()
  }
  if value < 65536 {
    return bytes.from_ints([224 + value / 4096, 128 + value / 64 % 64, 128 + value % 64])?.utf8()
  }
  if value <= 1114111 {
    return bytes.from_ints([240 + value / 262144, 128 + value / 4096 % 64, 128 + value / 64 % 64, 128 + value % 64])?.utf8()
  }
  ""
}

pure float_is_nan(value: Float) -> Bool {
  value.format() == "NaN"
}

pure float_is_infinite(value: Float) -> Bool {
  value.format() in ["Infinity", "-Infinity"]
}

pure zero_text(count: Int) -> Str {
  return "" when count <= 0
  bytes.concat([b"0" for _ in range(count)]).utf8() ?? ""
}

pure increment_decimal(digits: Str) -> Str {
  var out = ""
  var carry = 1
  var index = digits.byte_len() - 1
  while index >= 0 {
    let value = (digits.byte_at(index) ?? 48) - 48 + carry
    carry = value / 10
    out = f"{value % 10}{out}"
    index -= 1
  }
  if carry > 0 { out = f"1{out}" }
  out
}

# Formats a plain decimal literal from its own digits, rounding half to even.
# A double holds about 17 significant digits, so beyond that precision the
# binary value would show digits the literal never contained. Returns the
# magnitude without a sign, or null for anything that is not a plain decimal
# literal (hex, inf, nan, surrounding blanks), which keeps the binary path and
# its diagnostics.
pure exact_decimal_text(text: Str, places: Int) -> Str? {
  return null when ! rx"^[+-]?([0-9]+[.]?[0-9]*|[.][0-9]+)([eE][+-]?[0-9]+)?$".matches(text)
  var unsigned = text
  if unsigned.starts_with("-") or unsigned.starts_with("+") { unsigned = unsigned.byte_slice(1) }
  var exponent = 0
  let exponent_at = unsigned.find("e") ?? unsigned.find("E") ?? unsigned.byte_len()
  if exponent_at < unsigned.byte_len() {
    exponent = integer_prefix(unsigned.byte_slice(exponent_at + 1), false)
    unsigned = unsigned.byte_slice(0, exponent_at)
  }
  return null when exponent > 100000 or exponent < -100000
  let dot_at = unsigned.find(".")
  let whole = if let at = dot_at { unsigned.byte_slice(0, at) } else { unsigned }
  let fraction = if let at = dot_at { unsigned.byte_slice(at + 1) } else { "" }

  var digits = f"{whole}{fraction}"
  var point = whole.byte_len() + exponent
  if point < 0 {
    digits = f"{zero_text(-point)}{digits}"
    point = 0
  }
  if point > digits.byte_len() { digits = f"{digits}{zero_text(point - digits.byte_len())}" }

  var integer = digits.byte_slice(0, point)
  if integer == "" { integer = "0" }
  var kept_fraction = digits.byte_slice(point)
  if kept_fraction.byte_len() > places {
    let dropped = (kept_fraction.byte_at(places) ?? 48) - 48
    let rest_nonzero = kept_fraction.byte_slice(places + 1).replace("0", with: "") != ""
    let kept = f"{integer}{kept_fraction.byte_slice(0, places)}"
    let last_odd = ((kept.byte_at(kept.byte_len() - 1) ?? 48) - 48) % 2 == 1
    let round_up = dropped > 5 or (dropped == 5 and (rest_nonzero or last_odd))
    let rounded = if round_up { increment_decimal(kept) } else { kept }
    integer = rounded.byte_slice(0, rounded.byte_len() - places)
    kept_fraction = rounded.byte_slice(rounded.byte_len() - places)
  } else {
    kept_fraction = f"{kept_fraction}{zero_text(places - kept_fraction.byte_len())}"
  }

  while integer.byte_len() > 1 and integer.starts_with("0") { integer = integer.byte_slice(1) }
  f"{integer}{if places > 0 { "." + kept_fraction } else { "" }}"
}

proc utf8_locale() [env] -> Bool {
  var locale = ""
  for name in ["LC_ALL", "LC_CTYPE", "LANG"] {
    let value = env.get_or(name, "") ?? ""
    if value != "" { locale = value; break }
  }
  let lower = locale.lower()
  lower.find("utf-8") != null or lower.find("utf8") != null
}

pure scan_escape(text: Bytes, slash: Int, zero_prefix: Bool = false, utf8: Bool = true) -> Result[Escape] {
  let next = slash + 1
  if next >= text.len() { return {data: bytes.from_text("\\"), next: next, stop: false, issue: null} }
  let code = text[next..next + 1].utf8() ?? ""

  match code {
    "a" => {data: bytes.from_text("\u{7}"), next: next + 1, stop: false, issue: null}
    "b" => {data: bytes.from_text("\u{8}"), next: next + 1, stop: false, issue: null}
    "e" => {data: bytes.from_text("\u{1b}"), next: next + 1, stop: false, issue: null}
    "f" => {data: bytes.from_text("\u{c}"), next: next + 1, stop: false, issue: null}
    "n" => {data: bytes.from_text("\n"), next: next + 1, stop: false, issue: null}
    "r" => {data: bytes.from_text("\r"), next: next + 1, stop: false, issue: null}
    "t" => {data: bytes.from_text("\t"), next: next + 1, stop: false, issue: null}
    "v" => {data: bytes.from_text("\u{b}"), next: next + 1, stop: false, issue: null}
    "c" => {data: b"", next: next + 1, stop: true, issue: null}
    "\\" | "'" | "\"" => {data: bytes.from_text(code), next: next + 1, stop: false, issue: null}
    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" => {
      var at = next
      var value = 0
      var count = 0
      let limit = if zero_prefix and code == "0" { 4 } else { 3 }
      while at < text.len() and count < limit and "01234567".find(text[at..at + 1].utf8() ?? "") != null {
        value = value * 8 + ("01234567".find(text[at..at + 1].utf8() ?? "") ?? 0)
        at += 1
        count += 1
      }
      {data: byte_character(value % 256)?, next: at, stop: false, issue: null}
    }
    "x" => {
      var at = next + 1
      var value = 0
      var count = 0
      while at < text.len() and count < 2 {
        let digit = digit_value(text[at..at + 1].utf8() ?? "")
        if digit == null { break }
        value = value * 16 + (digit ?? 0)
        at += 1
        count += 1
      }
      if count == 0 {
        {data: b"", next: at, stop: false, issue: "missing hexadecimal number in escape"}
      } else {
        {data: byte_character(value)?, next: at, stop: false, issue: null}
      }
    }
    "u" | "U" => {
      let digits = if code == "u" { 4 } else { 8 }
      let start = next + 1
      let end = start + digits
      if end > text.len() {
        {data: b"", next: text.len(), stop: false, issue: "missing hexadecimal number in escape"}
      } else {
        let raw = text[start..end]
        var value = 0
        var valid = true
        for at in range(digits) {
          let digit = digit_value(raw[at..at + 1].utf8() ?? "")
          if digit == null { valid = false } else { value = value * 16 + (digit ?? 0) }
        }
        let invalid_scalar = value > 1114111 or (value >= 55296 and value <= 57343)
        if ! valid or invalid_scalar {
          {data: b"", next: end, stop: false, issue: "invalid universal character name " + "\\" + code + (raw.utf8() ?? "").lower()}
        } else {
          {data: bytes.from_text(if utf8 or value < 128 { codepoint_text(value)? } else { "\\" + (if value <= 65535 { "u" + pad_text(radix_text(value, 16, false), 4, false, "0") } else { "U" + pad_text(radix_text(value, 16, false), 8, false, "0") }) }), next: end, stop: false, issue: null}
        }
      }
    }
    else => {data: bytes.concat([b"\\", text[next..next + 1]]), next: next + 1, stop: false, issue: null}
  }
}

pure unescape_bytes(text: Bytes, utf8: Bool = true) -> Result[Escape] {
  var output: List[Bytes] = []
  var at = 0

  while at < text.len() {
    let byte = text.byte_at(at) ?? 0
    if byte == 92 {
      let escaped = scan_escape(text, at, true, utf8)?
      if let issue = escaped.issue { return {data: bytes.concat(output), next: escaped.next, stop: false, issue: issue} }
      if escaped.stop { return {data: bytes.concat(output), next: escaped.next, stop: true, issue: null} }
      output += [escaped.data]
      at = escaped.next
    } else {
      output += [text[at..at + 1]]
      at += 1
    }
  }

  {data: bytes.concat(output), next: at, stop: false, issue: null}
}

pure scientific(value: Float, precision: Int, upper: Bool) -> Str {
  return if upper { "NAN" } else { "nan" } when float_is_nan(value)
  return if upper { "INF" } else { "inf" } when float_is_infinite(value)
  let negative = value < 0.0
  var exponent = 0
  var mantissa = value.abs()

  if mantissa != 0.0 and mantissa == mantissa {
    while mantissa >= 10.0 { mantissa = mantissa / 10.0; exponent += 1 }
    while mantissa < 1.0 { mantissa = mantissa * 10.0; exponent -= 1 }
  }

  var digits = mantissa.format(precision)
  if (digits.parse_float() ?? 0.0) >= 10.0 { digits = (mantissa / 10.0).format(precision); exponent += 1 }
  let exponent_sign = if exponent < 0 { "-" } else { "+" }
  let abs_exponent = if exponent < 0 { -exponent } else { exponent }
  let exponent_digits = if abs_exponent < 10 { "0" + f"{abs_exponent}" } else { f"{abs_exponent}" }
  f"{if negative { "-" } else { "" }}{digits}{if upper { "E" } else { "e" }}{exponent_sign}{exponent_digits}"
}

pure hex_float(value: Float, precision: Int?, upper: Bool) -> Str {
  return if upper { "NAN" } else { "nan" } when float_is_nan(value)
  return if upper { "INF" } else { "inf" } when float_is_infinite(value)
  let negative = value < 0.0
  let magnitude = value.abs()
  if magnitude == 0.0 { return if upper { "0X0P+0" } else { "0x0p+0" } }
  var exponent = 0
  var mantissa = magnitude
  while mantissa >= 16.0 { mantissa = mantissa / 16.0; exponent += 4 }
  while mantissa < 1.0 { mantissa = mantissa * 16.0; exponent -= 4 }
  let whole = mantissa.floor() ?? 0
  var fraction = mantissa - whole.float()
  let places = if let requested = precision { if requested > 1000 { 1000 } else if requested < 0 { 0 } else { requested } } else { 13 }
  let alphabet = if upper { "0123456789ABCDEF" } else { "0123456789abcdef" }
  var digits = ""
  repeat places times {
    fraction = fraction * 16.0
    let digit = fraction.floor() ?? 0
    digits += alphabet.byte_slice(digit, length: 1)
    fraction -= digit.float()
  }
  if precision == null {
    while digits.ends_with("0") { digits = digits.byte_slice(0, length: digits.byte_len() - 1) }
  }
  let body = f"{radix_text(whole, 16, upper)}{if digits == "" { "" } else { "." + digits }}"
  f"{if negative { "-" } else { "" }}{if upper { "0X" } else { "0x" }}{body}{if upper { "P" } else { "p" }}{if exponent < 0 { "" } else { "+" }}{exponent}"
}

pure trim_fraction(text: Str) -> Str {
  let exponent_at = text.find("e") ?? text.find("E") ?? text.byte_len()
  let mantissa = text.byte_slice(0, length: exponent_at)
  return text when ! ("." in mantissa)
  var end = mantissa.byte_len()
  while end > 0 and mantissa.byte_slice(end - 1, length: 1) == "0" { end -= 1 }
  if end > 0 and mantissa.byte_slice(end - 1, length: 1) == "." { end -= 1 }
  mantissa.byte_slice(0, length: end) + text.byte_slice(exponent_at)
}

pure float_conversion(value: Float, conversion: Str, precision: Int?) -> Str {
  if float_is_nan(value) { return if conversion == conversion.upper() { "NAN" } else { "nan" } }
  if float_is_infinite(value) { return if conversion == conversion.upper() { "INF" } else { "inf" } }
  let requested = precision ?? 6
  let count = if requested < 0 { 0 } else if requested > 1000 { 1000 } else { requested }

  match conversion {
    "f" | "F" => value.format(count)
    "e" | "E" => scientific(value, count, conversion == "E")
    "g" | "G" => {
      let significant = if count == 0 { 1 } else { count }
      var exponent = 0
      var magnitude = value.abs()
      if magnitude != 0.0 and magnitude == magnitude {
        while magnitude >= 10.0 { magnitude = magnitude / 10.0; exponent += 1 }
        while magnitude < 1.0 { magnitude = magnitude * 10.0; exponent -= 1 }
      }
      let text = if exponent < -4 or exponent >= significant { scientific(value, significant - 1, conversion == "G") } else { value.format(if significant - exponent - 1 < 0 { 0 } else { significant - exponent - 1 }) }
      let clean = trim_fraction(text)
      if conversion == "G" { clean.upper() } else { clean }
    }
    "a" => hex_float(value, precision, false)
    "A" => hex_float(value, precision, true)
    else => value.format(count)
  }
}

proc shell_quote(data: Bytes) [env] -> Str {
  gnu.quote_bytes(data, always: false)
}

proc conversion_text(spec: PrintfSpec, argument: PrintfArgument, width: Int, precision: Int?, previous_output: Bytes) [process, env, error, io] -> Result[Rendered] {
  let left = spec.flags.find("-") != null or width < 0
  let field_width = if width == -9223372036854775807 - 1 { 9223372036854775807 } else if width < 0 { -width } else { width }
  let fill = if spec.flags.find("0") != null and ! left and precision == null { "0" } else { " " }

  if spec.conversion == "s" or spec.conversion == "q" {
    let value = if spec.conversion == "q" { shell_quote(argument.data) } else { argument.text }
    var clipped = value
    if let limit = precision {
      if limit < clipped.count_chars() { clipped = clipped[0..limit] }
    }
    let rendered = if spec.conversion == "s" and precision == null { argument.data } else { bytes.from_text(clipped) }
    let padding_count = field_width - clipped.count_chars()
    if field_width > 1000000 {
      write_field(previous_output, b"", rendered, b"", padding_count, b" ", left)
      return {data: b"", stop: false, failed: false, prefix_flushed: true}
    }
    let padding = bytes.from_text(pad_text("", padding_count, false, " "))
    return {data: if left { bytes.concat([rendered, padding]) } else { bytes.concat([padding, rendered]) }, stop: false, failed: false, prefix_flushed: false}
  }

  if spec.conversion == "c" {
    # The operand is text even when it looks numeric: GNU prints its first byte, or NUL when it is empty.
    let character: Bytes = if argument.data.len() == 0 { b"\0" } else { argument.data[0..1] }
    let padding_count = field_width - 1
    if field_width > 1000000 {
      write_field(previous_output, b"", character, b"", padding_count, b" ", left)
      return {data: b"", stop: false, failed: false, prefix_flushed: true}
    }
    let padding = bytes.from_text(pad_text("", padding_count, false, " "))
    return {data: if left { bytes.concat([character, padding]) } else { bytes.concat([padding, character]) }, stop: false, failed: false, prefix_flushed: false}
  }

  if spec.conversion == "b" {
    let expanded = unescape_bytes(argument.data, utf8_locale())?
    if let issue = expanded.issue {
      gnu.write_bytes(previous_output)
      gnu.error(issue)
      exit 1
    }
    return {data: expanded.data, stop: expanded.stop, failed: false, prefix_flushed: false}
  }

  if "fFeEgGaA".find(spec.conversion) != null {
    let parsed = float_parse(argument.text)
    if let issue = parsed.issue { gnu.error(f"{gnu.quote_value(argument.text)}: {issue}") }
    let number = parsed.value
    let requested_precision = precision ?? 6
    let exact = if spec.conversion in ["f", "F"] and requested_precision > 17 and ! float_is_nan(number) and ! float_is_infinite(number) { exact_decimal_text(argument.text, requested_precision) } else { null }
    let body = if let digits = exact {
      digits
    } else if spec.conversion in ["f", "F"] and requested_precision > 100 and ! float_is_nan(number) and ! float_is_infinite(number) {
      number.format_number(spec.conversion, requested_precision)?
    } else {
      float_conversion(number, spec.conversion, precision)
    }
    let negative = number < 0.0 or (parsed.issue != "expected a numeric value" and argument.text.trim().starts_with("-"))
    let sign = if negative { "-" } else if spec.flags.find("+") != null { "+" } else if spec.flags.find(" ") != null { " " } else { "" }
    let unsigned_body = if body.starts_with("-") { body.byte_slice(1) } else { body }
    let raw = sign + unsigned_body
    let float_fill = if spec.flags.find("0") != null and ! left and ! float_is_nan(number) and ! float_is_infinite(number) { "0" } else { " " }
    let zero_padding = float_fill == "0" and field_width > raw.count_chars()
    let padding_count = field_width - raw.count_chars()
    if field_width > 1000000 {
      write_field(previous_output, bytes.from_text(if zero_padding { sign } else { "" }), bytes.from_text(if zero_padding { unsigned_body } else { raw }), b"", padding_count, bytes.from_text(if zero_padding { "0" } else { " " }), left)
      return {data: b"", stop: false, failed: parsed.issue != null, prefix_flushed: true}
    }
    let padded = if float_fill == "0" and field_width > raw.count_chars() { sign + pad_text(raw.byte_slice(sign.byte_len()), field_width - sign.count_chars(), false, "0") } else { pad_text(raw, field_width, left, float_fill) }
    return {data: bytes.from_text(padded), stop: false, failed: parsed.issue != null, prefix_flushed: false}
  }

  let auto_base = spec.conversion != "d"
  let parsed = integer_parse(argument.text, auto_base)
  if let warning = parsed.warning {
    if env.get("POSIXLY_CORRECT") is Err(_) { gnu.error(f"warning: {warning}: character(s) following character constant have been ignored") }
  }
  if let issue = parsed.issue { gnu.error(f"{gnu.quote_value(argument.text)}: {issue}") }
  let number = parsed.value
  let base = if spec.conversion == "o" { 8 } else if spec.conversion == "x" or spec.conversion == "X" { 16 } else { 10 }
  let unsigned = spec.conversion in ["o", "u", "x", "X"]
  let negative = number < 0 and ! unsigned
  let min_int = number == -9223372036854775807 - 1
  let magnitude = if number < 0 and ! min_int { -number } else if number < 0 { 9223372036854775807 } else { number }
  var digits = if unsigned and parsed.issue == "Numerical result out of range" { if base == 10 { "18446744073709551615" } else if base == 16 { "ffffffffffffffff" } else { "1777777777777777777777" } } else if unsigned and number < 0 { unsigned_negative(number, base, spec.conversion == "X") } else if min_int and base == 10 { "9223372036854775808" } else if base == 10 { f"{magnitude}" } else { radix_text(magnitude, base, spec.conversion == "X") }
  let alternate_octal_zero = base == 8 and spec.flags.find("#") != null
  if precision == 0 and magnitude == 0 and ! alternate_octal_zero { digits = "" }

  if let min_digits = precision {
    if min_digits > digits.byte_len() { digits = pad_text(digits, min_digits, false, "0") }
  }

  let sign = if negative { "-" } else if ! unsigned and spec.flags.find("+") != null { "+" } else if ! unsigned and spec.flags.find(" ") != null { " " } else { "" }
  var prefix = ""
  if spec.flags.find("#") != null and magnitude != 0 {
    if base == 8 and ! digits.starts_with("0") { prefix = "0" }
    if base == 16 { prefix = if spec.conversion == "X" { "0X" } else { "0x" } }
  }
  if base == 8 and spec.flags.find("#") != null and magnitude == 0 and precision == 0 { digits = "0" }
  let raw = sign + prefix + digits
  let zero_padding = fill == "0" and ! left and precision == null and field_width > raw.count_chars()
  let padding_count = field_width - raw.count_chars()
  if field_width > 1000000 {
    write_field(previous_output, bytes.from_text(if zero_padding { sign + prefix } else { "" }), bytes.from_text(if zero_padding { digits } else { raw }), b"", padding_count, bytes.from_text(if zero_padding { "0" } else { " " }), left)
    return {data: b"", stop: false, failed: parsed.issue != null, prefix_flushed: true}
  }
  if zero_padding {
    return {data: bytes.from_text(sign + prefix + pad_text(digits, digits.count_chars() + padding_count, false, "0")), stop: false, failed: parsed.issue != null, prefix_flushed: false}
  }
  {data: bytes.from_text(pad_text(raw, field_width, left, if fill == "0" { " " } else { fill })), stop: false, failed: parsed.issue != null, prefix_flushed: false}
}

proc render_pass(fmt: Bytes, values: List[PrintfArgument], first_argument: Int, prefix: Bytes) [error, io, process, env] -> Pass {
  var output: List[Bytes] = []
  var at = 0
  var prefix_data = prefix
  var prefix_flushed = false
  var argument_index = first_argument
  var next_argument = first_argument
  var conversions = 0
  var failed = false

  while at < fmt.len() {
    let byte = fmt.byte_at(at) ?? 0
    if byte == 92 {
      let escaped = scan_escape(fmt, at, utf8: utf8_locale())?
      if let issue = escaped.issue {
        gnu.write_bytes(bytes.concat([prefix_data, bytes.concat(output)]))
        gnu.error(issue)
        exit 1
      }
      output += [escaped.data]
      if escaped.stop { return {data: bytes.concat(output), next_argument: next_argument, conversions: conversions, stop: true, failed: failed, prefix_flushed: prefix_flushed} }
      at = escaped.next
    } else if byte != 37 {
      output += [fmt[at..at + 1]]
      at += 1
    } else if at + 1 < fmt.len() and fmt.byte_at(at + 1) == 37 {
      output += [bytes.from_text("%")]
      at += 2
    } else {
      let spec = parse_spec(fmt, at)
      if ! spec.valid {
        let shown = lossless_text(fmt[at..if spec.end > at { spec.end } else { at + 1 }])?
        gnu.write_bytes(bytes.concat([prefix_data, bytes.concat(output)]))
        gnu.error(f"{shown}: invalid conversion specification")
        exit 1
      }

      var width = spec.width
      if spec.width_dynamic {
        let index = if spec.width_position == null { argument_index } else { indexed_argument(first_argument, spec.width_position ?? 0, values.len()) }
        let width_text = if index < values.len() { values[index].text } else { "0" }
        let parsed_width = integer_parse(width_text, true)
        if parsed_width.issue == "Numerical result out of range" {
          gnu.error(f"{gnu.quote_value(width_text)}: Numerical result out of range")
          failed = true
        } else if let issue = parsed_width.issue {
          gnu.error(f"{gnu.quote_value(width_text)}: {issue}")
          failed = true
        }
        if parsed_width.value < -2147483648 or parsed_width.value > 2147483647 {
          gnu.write_bytes(bytes.concat([prefix_data, bytes.concat(output)]))
          gnu.error(f"invalid field width: {gnu.quote_value(width_text)}")
          exit 1
        }
        width = parsed_width.value
        if spec.width_position == null {
          argument_index += 1
          next_argument = if argument_index > values.len() { values.len() } else if argument_index > next_argument { argument_index } else { next_argument }
        } else { next_argument = consumed_arguments(next_argument, index, values.len()) }
      }
      if width >= 2147483647 or width <= -2147483647 {
        gnu.write_bytes(bytes.concat([prefix_data, bytes.concat(output)]))
        gnu.error("write error")
        exit 1
      }
      var precision = spec.precision
      if spec.precision_dynamic {
        let index = if spec.precision_position == null { argument_index } else { indexed_argument(first_argument, spec.precision_position ?? 0, values.len()) }
        let precision_argument = if index < values.len() { values[index].text } else { "-1" }
        let parsed_precision = integer_parse(precision_argument, true)
        if parsed_precision.issue == "Numerical result out of range" {
          gnu.error(f"{gnu.quote_value(precision_argument)}: Numerical result out of range")
          failed = true
        } else if let issue = parsed_precision.issue {
          gnu.error(f"{gnu.quote_value(precision_argument)}: {issue}")
          failed = true
        }
        let dynamic = parsed_precision.value
        if dynamic > 2147483647 {
          gnu.error(f"invalid precision: {gnu.quote_value(precision_argument)}")
          exit 1
        }
        precision = if dynamic < 0 { null } else { dynamic }
        if spec.precision_position == null {
          argument_index += 1
          next_argument = if argument_index > values.len() { values.len() } else if argument_index > next_argument { argument_index } else { next_argument }
        } else { next_argument = consumed_arguments(next_argument, index, values.len()) }
      }
      let index = if spec.position == null { argument_index } else { indexed_argument(first_argument, spec.position ?? 0, values.len()) }
      let value: PrintfArgument = if index < values.len() { values[index] } else if spec.conversion in ["d", "i", "o", "u", "x", "X", "f", "F", "e", "E", "g", "G", "a", "A"] { printf_argument(b"0")? } else { printf_argument(b"")? }
      if spec.position == null {
        argument_index += 1
        next_argument = if argument_index > values.len() { values.len() } else if argument_index > next_argument { argument_index } else { next_argument }
      } else { next_argument = consumed_arguments(next_argument, index, values.len()) }
      let rendered = conversion_text(spec, value, width, precision, bytes.concat([prefix_data, bytes.concat(output)]))?
      if rendered.prefix_flushed {
        prefix_flushed = true
        prefix_data = b""
        output = []
      }
      output += [rendered.data]
      failed = failed or rendered.failed
      conversions += 1
      at = spec.end
      if rendered.stop { return {data: bytes.concat(output), next_argument: next_argument, conversions: conversions, stop: true, failed: failed, prefix_flushed: prefix_flushed} }
    }
  }

  {data: bytes.concat(output), next_argument: next_argument, conversions: conversions, stop: false, failed: failed, prefix_flushed: prefix_flushed}
}

proc render(fmt: Bytes, values: List[PrintfArgument]) [error, io, process, env] -> PrintfOutput {
  let first = render_pass(fmt, values, 0, b"")
  var output = first.data
  var failed = first.failed
  var argument = first.next_argument
  if first.stop or first.conversions == 0 { return {data: output, failed: failed, next_argument: argument, stopped: first.stop} }

  while argument < values.len() {
    let next = render_pass(fmt, values, argument, output)
    if next.prefix_flushed { output = b"" }
    output = bytes.concat([output, next.data])
    failed = failed or next.failed
    argument = next.next_argument
    if next.stop or next.conversions == 0 { return {data: output, failed: failed, next_argument: argument, stopped: next.stop} }
  }

  {data: output, failed: failed, next_argument: argument, stopped: false}
}

# Once FORMAT begins, every later argument is data, including option-looking
# strings. The explicit terminator is recognized only before FORMAT.
proc main(...argv: List[Bytes]) [error, io, process, env] {
  var arguments = argv
  if ! arguments.is_empty() {
    if arguments[0] == b"--" {
      arguments = arguments[1..]
    } else if arguments[0] == b"--help" {
      gnu.help("Usage: printf FORMAT [ARGUMENT]...\nPrint arguments according to FORMAT.\n")
      return
    } else if arguments[0] == b"--version" {
      gnu.version("printf")
      return
    }
  }
  if arguments.is_empty() { gnu.missing_operand() }
  let format = arguments[0]
  var values: List[PrintfArgument] = []
  for value in arguments[1..] { values += [printf_argument(value)?] }
  let output = render(format, values)
  gnu.write_bytes(output.data)
  if ! output.stopped and output.next_argument < values.len() {
    gnu.error(f"warning: ignoring excess arguments, starting with {gnu.quote_value(values[output.next_argument].text)}")
  }
  if output.failed { exit 1 }
}
