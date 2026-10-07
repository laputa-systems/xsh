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

type Escape = {text: Str, next: Int, stop: Bool, issue: Str?}
type Rendered = {text: Str, stop: Bool, failed: Bool}
type Pass = {text: Str, next_argument: Int, conversions: Int, stop: Bool, failed: Bool}
type DecimalScan = {value: Int, next: Int}
type IntegerParse = {value: Int, issue: Str?}
type FloatParse = {value: Float, issue: Str?}
type PrintfOutput = {text: Str, failed: Bool, next_argument: Int, stopped: Bool}

pure usage(applet_name: Str, summary: Str) -> Str {
  f"usage: xsh applets/{applet_name}.xsh -- {summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  AppletError.Usage(usage(applet_name, summary))
}

pure scan_decimal(text: Str, at: Int) -> DecimalScan {
  var value = 0
  var next = at

  while next < text.byte_len() and "0123456789".find(text.byte_slice(next, length: 1)) != null {
    let digit = "0123456789".find(text.byte_slice(next, length: 1)) ?? 0
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

pure parse_spec(text: Str, start: Int) -> PrintfSpec {
  var at = start + 1
  var position: Int? = null
  let leading = scan_decimal(text, at)
  if leading.next > at and leading.next < text.byte_len() and text.byte_slice(leading.next, length: 1) == "$" {
    position = leading.value - 1
    at = leading.next + 1
  }
  var flags = ""

  while at < text.byte_len() and "-+ #0'".find(text.byte_slice(at, length: 1)) != null {
    let flag = text.byte_slice(at, length: 1)
    if flags.find(flag) == null { flags += flag }
    at += 1
  }

  var width = 0
  var width_dynamic = false
  var width_position: Int? = null
  if at < text.byte_len() and text.byte_slice(at, length: 1) == "*" {
    width_dynamic = true
    at += 1
    let numbered = scan_decimal(text, at)
    if numbered.next > at and numbered.next < text.byte_len() and text.byte_slice(numbered.next, length: 1) == "$" {
      width_position = numbered.value - 1
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
  if at < text.byte_len() and text.byte_slice(at, length: 1) == "." {
    at += 1
    precision = 0
    if at < text.byte_len() and text.byte_slice(at, length: 1) == "*" {
      precision_dynamic = true
      at += 1
      let numbered = scan_decimal(text, at)
      if numbered.next > at and numbered.next < text.byte_len() and text.byte_slice(numbered.next, length: 1) == "$" {
        precision_position = numbered.value - 1
        at = numbered.next + 1
      }
    } else {
      let scanned = scan_decimal(text, at)
      precision = scanned.value
      at = scanned.next
    }
  }

  while at < text.byte_len() and "hlLzjt".find(text.byte_slice(at, length: 1)) != null { at += 1 }

  let conversion = if at < text.byte_len() { text.byte_slice(at, length: 1) } else { "" }
  let known_conversion = conversion in ["d", "i", "o", "u", "x", "X", "f", "F", "e", "E", "g", "G", "a", "A", "s", "c", "b", "q"]
  let invalid_zero_flag = flags.find("0") != null and conversion in ["s", "c"]
  let invalid_character_precision = conversion == "c" and precision != null
  let invalid_quote_parameters = conversion == "q" and (flags != "" or width != 0 or width_dynamic or precision != null or precision_dynamic)
  let invalid_escape_parameters = conversion == "b" and (flags != "" or width != 0 or width_dynamic or precision != null or precision_dynamic)
  let valid = known_conversion and ! invalid_zero_flag and ! invalid_character_precision and ! invalid_quote_parameters and ! invalid_escape_parameters

  {end: if conversion == "" { at } else { at + 1 }, flags: flags, width: width, width_dynamic: width_dynamic, precision: precision, precision_dynamic: precision_dynamic, position: position, width_position: width_position, precision_position: precision_position, conversion: conversion, valid: valid}
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
  if trimmed.starts_with("'") or trimmed.starts_with("\"") { return {value: codepoint_value(trimmed.byte_slice(1)), issue: null} }
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

  while at < body.byte_len() {
    let digit = digit_value(body.byte_slice(at, length: 1))
    if digit == null or (digit ?? 99) >= base { break }
    found = true
    let next_digit = digit ?? 0
    if value > (9223372036854775807 - next_digit) / base { overflow = true } else { value = value * base + next_digit }
    at += 1
  }

  let tail = if found { body.byte_slice(at) } else { body }
  if ! found { {value: 0, issue: "expected a numeric value"} } else if overflow { {value: if sign < 0 { -9223372036854775807 - 1 } else { 9223372036854775807 }, issue: "Numerical result out of range"} } else if tail != "" { {value: sign * value, issue: "value not completely converted"} } else { {value: sign * value, issue: null} }
}

pure float_parse(text: Str) -> FloatParse {
  var start = 0
  while start < text.byte_len() and (text.byte_at(start) ?? 0) in [9, 10, 11, 12, 13, 32] { start += 1 }
  let trimmed = text.byte_slice(start)
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
  let issue: Str? = if trimmed.byte_slice(at) == "" { null } else { "value not completely converted" }
  {value: value_text.parse_float() ?? 0.0, issue: issue}
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

pure byte_character(value: Int) -> Result[Str] {
  bytes.from_ints([value])?.utf8()
}

pure codepoint_text(value: Int) -> Result[Str] {
  if value < 128 { return byte_character(value) }
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

pure scan_escape(text: Str, slash: Int) -> Result[Escape] {
  let next = slash + 1
  if next >= text.byte_len() { return {text: "\\", next: next, stop: false, issue: null} }
  let code = text.byte_slice(next, length: 1)

  match code {
    "a" => {text: "\u{7}", next: next + 1, stop: false, issue: null}
    "b" => {text: "\u{8}", next: next + 1, stop: false, issue: null}
    "e" => {text: "\u{1b}", next: next + 1, stop: false, issue: null}
    "f" => {text: "\u{c}", next: next + 1, stop: false, issue: null}
    "n" => {text: "\n", next: next + 1, stop: false, issue: null}
    "r" => {text: "\r", next: next + 1, stop: false, issue: null}
    "t" => {text: "\t", next: next + 1, stop: false, issue: null}
    "v" => {text: "\u{b}", next: next + 1, stop: false, issue: null}
    "c" => {text: "", next: next + 1, stop: true, issue: null}
    "\\" | "'" | "\"" => {text: code, next: next + 1, stop: false, issue: null}
    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" => {
      var at = next
      var value = 0
      var count = 0
      while at < text.byte_len() and count < 3 and "01234567".find(text.byte_slice(at, length: 1)) != null {
        value = value * 8 + ("01234567".find(text.byte_slice(at, length: 1)) ?? 0)
        at += 1
        count += 1
      }
      {text: byte_character(value % 256)?, next: at, stop: false, issue: null}
    }
    "x" => {
      var at = next + 1
      var value = 0
      var count = 0
      while at < text.byte_len() and count < 2 {
        let digit = digit_value(text.byte_slice(at, length: 1))
        if digit == null { break }
        value = value * 16 + (digit ?? 0)
        at += 1
        count += 1
      }
      if count == 0 {
        {text: "", next: at, stop: false, issue: "missing hexadecimal number in escape"}
      } else {
        {text: byte_character(value)?, next: at, stop: false, issue: null}
      }
    }
    "u" | "U" => {
      let digits = if code == "u" { 4 } else { 8 }
      let start = next + 1
      let end = start + digits
      if end > text.byte_len() {
        {text: "", next: text.byte_len(), stop: false, issue: "missing hexadecimal number in escape"}
      } else {
        let raw = text.byte_slice(start, length: digits)
        let raw_escape = text.byte_slice(slash, length: end - slash)
        var value = 0
        var valid = true
        for at in range(digits) {
          let digit = digit_value(raw.byte_slice(at, length: 1))
          if digit == null { valid = false } else { value = value * 16 + (digit ?? 0) }
        }
        let invalid_scalar = value > 1114111 or (value >= 55296 and value <= 57343)
        if ! valid or invalid_scalar {
          {text: "", next: end, stop: false, issue: "invalid universal character name " + raw_escape}
        } else {
          {text: codepoint_text(value)?, next: end, stop: false, issue: null}
        }
      }
    }
    else => {text: "\\" + code, next: next + 1, stop: false, issue: null}
  }
}

pure unescape_text(text: Str) -> Result[Escape] {
  var output = ""
  var at = 0

  while at < text.byte_len() {
    let byte = text.byte_at(at) ?? 0
    if byte == 92 {
      let escaped = scan_escape(text, at)?
      if let issue = escaped.issue { return {text: output, next: escaped.next, stop: false, issue: issue} }
      if escaped.stop { return {text: output, next: escaped.next, stop: true, issue: null} }
      output += escaped.text
      at = escaped.next
    } else {
      let width = utf8_width(text.byte_at(at) ?? 0)
      output += text.byte_slice(at, length: width)
      at += width
    }
  }

  {text: output, next: at, stop: false, issue: null}
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
  return text when ! ("." in text)
  var end = text.byte_len()
  while end > 0 and text.byte_slice(end - 1, length: 1) == "0" { end -= 1 }
  if end > 0 and text.byte_slice(end - 1, length: 1) == "." { end -= 1 }
  text.byte_slice(0, length: end)
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

pure shell_quote(text: Str) -> Str {
  return "''" when text == ""
  var simple = true
  for at in range(text.count_chars()) {
    let ch = text[at..at + 1]
    if "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_./-~".find(ch) == null { simple = false }
  }
  return text when simple
  "'" + text.replace("'", with: "'\\''") + "'"
}

proc conversion_text(spec: PrintfSpec, argument: Str, width: Int, precision: Int?, prefix: Str) [process, env, error, io] -> Result[Rendered] {
  let left = spec.flags.find("-") != null or width < 0
  let field_width = if width < 0 { -width } else { width }
  let fill = if spec.flags.find("0") != null and ! left and precision == null { "0" } else { " " }

  if spec.conversion == "s" or spec.conversion == "q" {
    let value = if spec.conversion == "q" { shell_quote(argument) } else { argument }
    var clipped = value
    if let limit = precision {
      if limit < clipped.count_chars() { clipped = clipped[0..limit] }
    }
    return {text: pad_text(clipped, field_width, left, " "), stop: false, failed: false}
  }

  if spec.conversion == "c" {
    let value = if argument.starts_with("'") or argument.starts_with("\"") { argument.byte_slice(1) } else { argument }
    let character = if let Ok(number) = value.parse_int() { codepoint_text(number)? } else { if value == "" { "\u{0}" } else { value[0..1] } }
    return {text: pad_text(character, field_width, left, " "), stop: false, failed: false}
  }

  if spec.conversion == "b" {
    let expanded = unescape_text(argument)?
    if let issue = expanded.issue {
      io.write_stdout(prefix)
      gnu.error(issue)
      exit 1
    }
    return {text: pad_text(expanded.text, field_width, left, " "), stop: expanded.stop, failed: false}
  }

  if "fFeEgGaA".find(spec.conversion) != null {
    let parsed = float_parse(argument)
    if let issue = parsed.issue { gnu.error(f"{gnu.quote_value(argument)}: {issue}") }
    let number = parsed.value
    let body = float_conversion(number, spec.conversion, precision)
    let negative = number < 0.0 or (parsed.issue != "expected a numeric value" and argument.trim().starts_with("-"))
    let sign = if negative { "-" } else if spec.flags.find("+") != null { "+" } else if spec.flags.find(" ") != null { " " } else { "" }
    let unsigned_body = if body.starts_with("-") { body.byte_slice(1) } else { body }
    let raw = sign + unsigned_body
    let float_fill = if spec.flags.find("0") != null and ! left and ! float_is_nan(number) and ! float_is_infinite(number) { "0" } else { " " }
    let padded = if float_fill == "0" and field_width > raw.count_chars() { sign + pad_text(raw.byte_slice(sign.byte_len()), field_width - sign.count_chars(), false, "0") } else { pad_text(raw, field_width, left, float_fill) }
    return {text: padded, stop: false, failed: parsed.issue != null}
  }

  let auto_base = spec.conversion != "d"
  let parsed = integer_parse(argument, auto_base)
  if let issue = parsed.issue { gnu.error(f"{gnu.quote_value(argument)}: {issue}") }
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
  if fill == "0" and ! left and precision == null and field_width > raw.count_chars() {
    return {text: sign + prefix + pad_text(digits, field_width - sign.count_chars() - prefix.count_chars(), false, "0"), stop: false, failed: parsed.issue != null}
  }
  {text: pad_text(raw, field_width, left, if fill == "0" { " " } else { fill }), stop: false, failed: parsed.issue != null}
}

proc render_pass(fmt: Str, values: List[Str], first_argument: Int, prefix: Str) [error, io, process, env] -> Pass {
  var output = ""
  var at = 0
  var argument_index = first_argument
  var next_argument = first_argument
  var conversions = 0
  var failed = false

  while at < fmt.byte_len() {
    let byte = fmt.byte_at(at) ?? 0
    if byte == 92 {
      let escaped = scan_escape(fmt, at)?
      if let issue = escaped.issue {
        io.write_stdout(prefix + output)
        gnu.error(issue)
        exit 1
      }
      output += escaped.text
      if escaped.stop { return {text: output, next_argument: next_argument, conversions: conversions, stop: true, failed: failed} }
      at = escaped.next
    } else if byte != 37 {
      let width = utf8_width(fmt.byte_at(at) ?? 0)
      output += fmt.byte_slice(at, length: width)
      at += width
    } else if at + 1 < fmt.byte_len() and fmt.byte_slice(at + 1, length: 1) == "%" {
      output += "%"
      at += 2
    } else {
      let spec = parse_spec(fmt, at)
      if ! spec.valid {
        let shown = fmt.byte_slice(at, length: if spec.end > at { spec.end - at } else { 1 })
        io.write_stdout(prefix + output)
        gnu.error(f"{shown}: invalid conversion specification")
        exit 1
      }

      var width = spec.width
      if spec.width_dynamic {
        let index = if spec.width_position == null { argument_index } else { indexed_argument(first_argument, spec.width_position ?? 0, values.len()) }
        width = integer_prefix(values.get(index) ?? "0", true)
        if spec.width_position == null {
          argument_index += 1
          next_argument = if argument_index > values.len() { values.len() } else if argument_index > next_argument { argument_index } else { next_argument }
        } else { next_argument = consumed_arguments(next_argument, index, values.len()) }
      }
      var precision = spec.precision
      if spec.precision_dynamic {
        let index = if spec.precision_position == null { argument_index } else { indexed_argument(first_argument, spec.precision_position ?? 0, values.len()) }
        let dynamic = integer_prefix(values.get(index) ?? "-1", true)
        precision = if dynamic < 0 { null } else { dynamic }
        if spec.precision_position == null {
          argument_index += 1
          next_argument = if argument_index > values.len() { values.len() } else if argument_index > next_argument { argument_index } else { next_argument }
        } else { next_argument = consumed_arguments(next_argument, index, values.len()) }
      }
      if width > 1000000 or width < -1000000 {
        io.write_stdout(prefix + output)
        gnu.error("field width too large")
        exit 1
      }

      let index = if spec.position == null { argument_index } else { indexed_argument(first_argument, spec.position ?? 0, values.len()) }
      let value = if index < values.len() { values[index] } else if spec.conversion in ["d", "i", "o", "u", "x", "X", "f", "F", "e", "E", "g", "G", "a", "A"] { "0" } else { "" }
      if spec.position == null {
        argument_index += 1
        next_argument = if argument_index > values.len() { values.len() } else if argument_index > next_argument { argument_index } else { next_argument }
      } else { next_argument = consumed_arguments(next_argument, index, values.len()) }
      let rendered = conversion_text(spec, value, width, precision, prefix + output)?
      output += rendered.text
      failed = failed or rendered.failed
      conversions += 1
      at = spec.end
      if rendered.stop { return {text: output, next_argument: next_argument, conversions: conversions, stop: true, failed: failed} }
    }
  }

  {text: output, next_argument: next_argument, conversions: conversions, stop: false, failed: failed}
}

proc render(fmt: Str, values: List[Str]) [error, io, process, env] -> PrintfOutput {
  let first = render_pass(fmt, values, 0, "")
  var output = first.text
  var failed = first.failed
  var argument = first.next_argument
  if first.stop or first.conversions == 0 { return {text: output, failed: failed, next_argument: argument, stopped: first.stop} }

  while argument < values.len() {
    let next = render_pass(fmt, values, argument, output)
    output += next.text
    failed = failed or next.failed
    argument = next.next_argument
    if next.stop or next.conversions == 0 { return {text: output, failed: failed, next_argument: argument, stopped: next.stop} }
  }

  {text: output, failed: failed, next_argument: argument, stopped: false}
}

# Once FORMAT begins, every later argument is data, including option-looking
# strings. The explicit terminator is recognized only before FORMAT.
proc main(...argv: List[Str]) [error, io, process, env] {
  var arguments = argv
  if ! arguments.is_empty() {
    if arguments[0] == "--" {
      arguments = arguments[1..]
    } else if arguments[0] == "--help" {
      gnu.help("Usage: printf FORMAT [ARGUMENT]...\nPrint arguments according to FORMAT.\n")
      return
    } else if arguments[0] == "--version" {
      gnu.version("printf")
      return
    }
  }
  return Err(usage_error("printf", "FORMAT [ARG...]")) when arguments.is_empty()
  let values = arguments[1..]
  let output = render(arguments[0], values)
  io.write_stdout(output.text)
  if ! output.stopped and output.next_argument < values.len() {
    gnu.error(f"warning: ignoring excess arguments, starting with {gnu.quote_value(values[output.next_argument])}")
  }
  if output.failed { exit 1 }
}
