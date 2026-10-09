#!/bin/xsh
use lib.gnu

const USAGE = """Usage: printf FORMAT [ARGUMENT]...
Print ARGUMENT(s) according to FORMAT, or execute according to OPTION:

      --help     display this help and exit
      --version  output version information and exit

FORMAT controls the output as in C printf.  A backslash escape in FORMAT is
interpreted, and the format is reused as often as necessary to consume all
arguments.
"""

const INT_MIN = -9223372036854775807 - 1

type Digits = {next: Int, value: Int, found: Bool}
type ParsedSpec = {
  start: Int,
  end: Int,
  shown: Str,
  position: Int?,
  flags: Str,
  width: Int?,
  width_star: Bool,
  width_position: Int?,
  precision: Int?,
  precision_star: Bool,
  precision_position: Int?,
  conversion: Str,
}
type Escape = {out: Bytes, next: Int, stop: Bool, failure: Str}
type SpecOutput = {out: Bytes, cursor: Int, max_position: Int, stop: Bool, failed: Bool, failure: Str, warnings: List[Str]}
type PassOutput = {out: Bytes, cursor: Int, has_spec: Bool, stop: Bool, failed: Bool, failure: Str, failure_start: Int, failure_length: Int, warnings: List[Str]}
type Number = {value: Int, valid: Bool, complete: Bool, overflow: Bool, char_tail: Bytes}
type FixedOutput = {text: Str, valid: Bool}

pure repeat_text(text: Str, count: Int) -> Str {
  return "" when count <= 0
  bytes.concat([bytes.from_text(text) for _ in range(count)]).utf8() ?? ""
}

pure digit(data: Bytes, at: Int) -> Int {
  let byte = data.byte_at(at) ?? -1
  return byte - 48 when byte >= 48 and byte <= 57
  -1
}

pure scan_digits(data: Bytes, at: Int) -> Digits {
  var next = at
  var value = 0
  var found = false
  while digit(data, next) >= 0 {
    found = true
    value = if value > 214748364 { 2147483648 } else { value * 10 + digit(data, next) }
    next += 1
  }
  {next: next, value: value, found: found}
}

pure parse_spec(format: Bytes, start: Int) -> ParsedSpec {
  var at = start + 1
  var position: Int? = null
  let positional = scan_digits(format, at)
  if positional.found and (format.byte_at(positional.next) ?? -1) == 36 {
    position = positional.value
    at = positional.next + 1
  }

  var flags = ""
  while at < format.len() and "#0- +'".find(format.slice(at, length: 1).utf8() ?? "") != null {
    flags = f"{flags}{format.slice(at, length: 1).utf8() ?? ""}"
    at += 1
  }

  var width: Int? = null
  var width_star = false
  var width_position: Int? = null
  if (format.byte_at(at) ?? -1) == 42 {
    width_star = true
    at += 1
    let star_position = scan_digits(format, at)
    if star_position.found and (format.byte_at(star_position.next) ?? -1) == 36 {
      width_position = star_position.value
      at = star_position.next + 1
    }
  } else {
    let parsed = scan_digits(format, at)
    if parsed.found { width = parsed.value; at = parsed.next }
  }

  var precision: Int? = null
  var precision_star = false
  var precision_position: Int? = null
  if (format.byte_at(at) ?? -1) == 46 {
    at += 1
    if (format.byte_at(at) ?? -1) == 42 {
      precision_star = true
      at += 1
      let star_position = scan_digits(format, at)
      if star_position.found and (format.byte_at(star_position.next) ?? -1) == 36 {
        precision_position = star_position.value
        at = star_position.next + 1
      }
    } else {
      let parsed = scan_digits(format, at)
      precision = if parsed.found { parsed.value } else { 0 }
      at = parsed.next
    }
  }

  while at < format.len() and "hlLjzt".find(format.slice(at, length: 1).utf8() ?? "") != null {
    at += 1
  }
  let conversion = if at < format.len() { format.slice(at, length: 1).utf8() ?? "" } else { "" }
  let end = if at < format.len() { at + 1 } else { at }
  {start: start, end: end, shown: format.slice(start, length: end - start).utf8() ?? "%", position: position, flags: flags, width: width, width_star: width_star, width_position: width_position, precision: precision, precision_star: precision_star, precision_position: precision_position, conversion: conversion}
}

pure hex_digit(byte: Int) -> Int {
  return byte - 48 when byte >= 48 and byte <= 57
  return byte - 65 + 10 when byte >= 65 and byte <= 70
  return byte - 97 + 10 when byte >= 97 and byte <= 102
  -1
}

pure codepoint_bytes(value: Int) -> Bytes {
  if value < 0 or value > 1114111 or (value >= 55296 and value <= 57343) { return b"" }
  if value < 128 { return bytes.from_ints([value]) ?? b"" }
  if value < 2048 {
    return bytes.from_ints([192 + value / 64, 128 + value % 64]) ?? b""
  }
  if value < 65536 {
    return bytes.from_ints([224 + value / 4096, 128 + value / 64 % 64, 128 + value % 64]) ?? b""
  }
  bytes.from_ints([240 + value / 262144, 128 + value / 4096 % 64, 128 + value / 64 % 64, 128 + value % 64]) ?? b""
}

pure codepoint_at(data: Bytes, at: Int, width: Int) -> Int {
  let first = data.byte_at(at) ?? 0
  return first when width == 1
  if width == 2 { return (first - 192) * 64 + (data.byte_at(at + 1) ?? 128) - 128 }
  if width == 3 { return (first - 224) * 4096 + ((data.byte_at(at + 1) ?? 128) - 128) * 64 + (data.byte_at(at + 2) ?? 128) - 128 }
  (first - 240) * 262144 + ((data.byte_at(at + 1) ?? 128) - 128) * 4096 + ((data.byte_at(at + 2) ?? 128) - 128) * 64 + (data.byte_at(at + 3) ?? 128) - 128
}

pure escape(data: Bytes, start: Int, additional: Bool) -> Escape {
  if start + 1 >= data.len() {
    return {out: b"\\", next: data.len(), stop: false, failure: ""}
  }
  let char = data.slice(start + 1, length: 1).utf8() ?? ""
  let value = match char {
    "a" => bytes.from_ints([7]) ?? b""
    "b" => bytes.from_ints([8]) ?? b""
    "e" => bytes.from_ints([27]) ?? b""
    "f" => bytes.from_ints([12]) ?? b""
    "n" => b"\n"
    "r" => b"\r"
    "t" => b"\t"
    "v" => bytes.from_ints([11]) ?? b""
    "\\" => b"\\"
    "\"" => b"\""
    "'" => b"'"
    "c" => b""
    else => b""
  }
  if char == "c" { return {out: b"", next: start + 2, stop: true, failure: ""} }
  if char in ["a", "b", "e", "f", "n", "r", "t", "v", "\\", "\"", "'"] {
    return {out: value, next: start + 2, stop: false, failure: ""}
  }

  if char == "x" or char == "u" or char == "U" {
    let max_digits = if char == "x" { 2 } else if char == "u" { 4 } else { 8 }
    var at = start + 2
    var number = 0
    var count = 0
    while at < data.len() and count < max_digits and hex_digit(data.byte_at(at) ?? -1) >= 0 {
      number = number * 16 + hex_digit(data.byte_at(at) ?? -1)
      at += 1
      count += 1
    }
    if char == "x" {
      return {out: b"", next: at, stop: false, failure: "missing hexadecimal number in escape"} when count == 0
      return {out: bytes.from_ints([number % 256]) ?? b"", next: at, stop: false, failure: ""}
    }
    if count != max_digits { return {out: b"", next: at, stop: false, failure: "missing hexadecimal number in escape"} }
    let encoded = codepoint_bytes(number)
    let shown = data.slice(start, length: at - start).utf8() ?? ""
    return {out: encoded, next: at, stop: false, failure: if encoded.len() == 0 and number != 0 { f"invalid universal character name {shown}" } else { "" }}
  }

  let octal_first = data.byte_at(start + 1) ?? -1
  if octal_first >= 48 and octal_first <= 55 {
    var at = start + 1
    var number = 0
    var count = 0
    let initial_zero = additional and octal_first == 48
    if initial_zero { at += 1 }
    while at < data.len() and count < 3 and (data.byte_at(at) ?? -1) >= 48 and (data.byte_at(at) ?? -1) <= 55 {
      number = number * 8 + (data.byte_at(at) ?? 48) - 48
      count += 1
      at += 1
    }
    return {out: bytes.from_ints([number % 256]) ?? b"", next: at, stop: false, failure: ""}
  }

  {out: data.slice(start, length: 2), next: start + 2, stop: false, failure: ""}
}

pure parse_number(raw: Bytes, automatic_base: Bool) -> Number {
  var at = 0
  while at < raw.len() and (raw.byte_at(at) ?? 0) in [9, 10, 11, 12, 13, 32] { at += 1 }
  return {value: 0, valid: false, complete: false, overflow: false, char_tail: b""} when at >= raw.len()

  let first = raw.byte_at(at) ?? -1
  if first == 39 or first == 34 {
    let body_at = at + 1
    return {value: 0, valid: false, complete: true, overflow: false, char_tail: b""} when body_at >= raw.len()
    let lead = raw.byte_at(body_at) ?? 0
    let candidate = if lead >= 194 and lead <= 223 { 2 } else if lead >= 224 and lead <= 239 { 3 } else if lead >= 240 and lead <= 244 { 4 } else { 1 }
    var width = candidate
    if body_at + width > raw.len() { width = 1 }
    if width > 1 {
      var offset = 1
      while offset < width {
        let continuation = raw.byte_at(body_at + offset) ?? 0
        if continuation < 128 or continuation > 191 { width = 1; break }
        offset += 1
      }
      let second = raw.byte_at(body_at + 1) ?? 0
      if (lead == 224 and second < 160) or (lead == 237 and second > 159) or (lead == 240 and second < 144) or (lead == 244 and second > 143) { width = 1 }
    }
    let tail_at = body_at + width
    return {value: codepoint_at(raw, body_at, width), valid: true, complete: true, overflow: false, char_tail: raw.slice(tail_at)}
  }

  var sign = 1
  if first == 45 { sign = -1; at += 1 } else if first == 43 { at += 1 }

  var base = 10
  var prefixed = false
  if automatic_base and (raw.slice(at, length: raw.len() - at).starts_with(b"0x") or raw.slice(at, length: raw.len() - at).starts_with(b"0X")) {
    base = 16
    prefixed = true
    at += 2
  } else if automatic_base and (raw.slice(at, length: raw.len() - at).starts_with(b"0b") or raw.slice(at, length: raw.len() - at).starts_with(b"0B")) {
    base = 2
    prefixed = true
    at += 2
  } else if automatic_base and raw.slice(at, length: raw.len() - at).starts_with(b"0") and raw.len() > at + 1 {
    base = 8
    at += 1
  }

  var value = 0
  var found = false
  var overflow = false
  let limit = if sign < 0 { INT_MIN } else { 9223372036854775807 }
  let cutoff = limit / base
  let cutlim = if sign < 0 { -(limit % base) } else { limit % base }
  while at < raw.len() {
    let byte = raw.byte_at(at) ?? -1
    let digit_value = if byte >= 48 and byte <= 57 { byte - 48 } else if byte >= 65 and byte <= 70 { byte - 65 + 10 } else if byte >= 97 and byte <= 102 { byte - 97 + 10 } else { -1 }
    if digit_value < 0 or digit_value >= base { break }
    found = true
    if ! overflow {
      if (sign < 0 and (value < cutoff or (value == cutoff and digit_value > cutlim))) or (sign > 0 and (value > cutoff or (value == cutoff and digit_value > cutlim))) {
        overflow = true
        value = limit
      } else if sign < 0 {
        value = value * base - digit_value
      } else {
        value = value * base + digit_value
      }
    }
    at += 1
  }
  return {value: 0, valid: true, complete: false, overflow: false, char_tail: b""} when prefixed and ! found
  {value: value, valid: found, complete: at == raw.len(), overflow: overflow, char_tail: b""}
}

pure digit_text(number: Int, base: Int, uppercase: Bool) -> Str {
  if number == 0 { return "0" }
  let alphabet = if uppercase { "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ" } else { "0123456789abcdefghijklmnopqrstuvwxyz" }
  var rest = number
  var reversed: List[Str] = []
  while rest > 0 {
    let digit_index = rest % base
    reversed += [alphabet.byte_slice(digit_index, length: 1)]
    rest = rest / base
  }
  var out = ""
  var index = reversed.len()
  while index > 0 { out = f"{out}{reversed[index - 1]}"; index -= 1 }
  out
}

pure twos_complement_hex(magnitude: Int, uppercase: Bool) -> Str {
  let digits = digit_text(magnitude, 16, false)
  let padded = f"{repeat_text("0", 16 - digits.byte_len())}{digits}"
  let alphabet = if uppercase { "0123456789ABCDEF" } else { "0123456789abcdef" }
  var carry = 1
  var out = ""
  var index = 15
  while index >= 0 {
    let nibble = hex_digit(padded.byte_at(index) ?? 48)
    let value = 15 - nibble + carry
    carry = if value >= 16 { 1 } else { 0 }
    out = f"{alphabet.byte_slice(value % 16, length: 1)}{out}"
    index -= 1
  }
  out
}

pure unsigned_decimal_from_negative(magnitude: Int) -> Str {
  let top = "18446744073709551616"
  let source = digit_text(magnitude, 10, false)
  let padded = f"{repeat_text("0", top.byte_len() - source.byte_len())}{source}"
  var borrow = 0
  var out = ""
  var index = top.byte_len() - 1
  while index >= 0 {
    var value = (top.byte_slice(index, length: 1).parse_int() ?? 0) - (padded.byte_slice(index, length: 1).parse_int() ?? 0) - borrow
    borrow = if value < 0 { 1 } else { 0 }
    if value < 0 { value += 10 }
    out = f"{value}{out}"
    index -= 1
  }
  var start = 0
  while start < out.byte_len() - 1 and out.byte_slice(start, length: 1) == "0" { start += 1 }
  out.byte_slice(start)
}

type DecimalDivision = {quotient: Str, remainder: Int}

pure divide_decimal(source: Str, divisor: Int) -> DecimalDivision {
  var quotient = ""
  var remainder = 0
  for index in range(source.byte_len()) {
    let value = remainder * 10 + (source.byte_slice(index, length: 1).parse_int() ?? 0)
    let digit_value = value / divisor
    remainder = value % divisor
    quotient = f"{quotient}{digit_value}"
  }
  var start = 0
  while start < quotient.byte_len() - 1 and quotient.byte_slice(start, length: 1) == "0" { start += 1 }
  {quotient: quotient.byte_slice(start), remainder: remainder}
}

pure unsigned_base_from_decimal(value: Str, base: Int, uppercase: Bool) -> Str {
  if value == "0" { return "0" }
  let alphabet = if uppercase { "0123456789ABCDEF" } else { "0123456789abcdef" }
  var rest = value
  var reversed: List[Str] = []
  while rest != "0" {
    let division = divide_decimal(rest, base)
    reversed += [alphabet.byte_slice(division.remainder, length: 1)]
    rest = division.quotient
  }
  var out = ""
  var index = reversed.len()
  while index > 0 { out = f"{out}{reversed[index - 1]}"; index -= 1 }
  out
}

pure increment_decimal(value: Str) -> Str {
  var carry = 1
  var out = ""
  var index = value.byte_len() - 1
  while index >= 0 {
    let digit_value = (value.byte_slice(index, length: 1).parse_int() ?? 0) + carry
    carry = if digit_value >= 10 { 1 } else { 0 }
    out = f"{digit_value % 10}{out}"
    index -= 1
  }
  if carry > 0 { out = f"1{out}" }
  out
}

pure fixed_decimal_text(raw: Bytes, places: Int, flags: Str) -> FixedOutput {
  let source = raw.utf8() ?? ""
  let trimmed = source.trim()
  if ! rx"^[+-]?(([0-9]+([.][0-9]*)?)|([.][0-9]+))([eE][+-]?[0-9]+)?$".matches(trimmed) { return {text: "", valid: false} }
  var value = trimmed
  var sign = ""
  if value.starts_with("-") { sign = "-"; value = value.byte_slice(1) } else if value.starts_with("+") { value = value.byte_slice(1) }
  var exponent = 0
  let lower_exponent_at = value.find("e")
  let upper_exponent_at = value.find("E")
  if lower_exponent_at != null or upper_exponent_at != null {
    let exponent_at = if lower_exponent_at != null { lower_exponent_at ?? 0 } else { upper_exponent_at ?? 0 }
    exponent = value.byte_slice(exponent_at + 1).parse_int() ?? 0
    value = value.byte_slice(0, exponent_at)
  }
  let dot_at = value.find(".") ?? value.byte_len()
  var digits = ""
  for index in range(value.byte_len()) {
    let char = value.byte_slice(index, length: 1)
    if char != "." { digits = f"{digits}{char}" }
  }
  var decimal_pos = dot_at + exponent
  if digits == "" { digits = "0" }
  if digits.replace("0", "") == "" {
    let fraction = repeat_text("0", places)
    let point = if places > 0 or flags.find("#") != null { "." } else { "" }
    let signed = if sign != "" { sign } else if flags.find("+") != null { "+" } else if flags.find(" ") != null { " " } else { "" }
    return {text: f"{signed}0{point}{fraction}", valid: true}
  }
  var leading = 0
  while leading < digits.byte_len() - 1 and digits.byte_slice(leading, length: 1) == "0" { leading += 1 }
  digits = digits.byte_slice(leading)
  decimal_pos -= leading
  let target = decimal_pos + places
  var scaled = "0"
  if target > 0 {
    let keep = if target < digits.byte_len() { target } else { digits.byte_len() }
    scaled = digits.byte_slice(0, keep)
    if target > keep { scaled = f"{scaled}{repeat_text("0", target - keep)}" }
    if target < digits.byte_len() and (digits.byte_slice(target, length: 1).parse_int() ?? 0) >= 5 {
      scaled = increment_decimal(scaled)
    }
  } else if target == 0 and (digits.byte_slice(0, length: 1).parse_int() ?? 0) >= 5 {
    scaled = "1"
  }
  while scaled.byte_len() < places + 1 { scaled = f"0{scaled}" }
  let split = scaled.byte_len() - places
  var integer = scaled.byte_slice(0, split)
  let fraction = scaled.byte_slice(split)
  var integer_start = 0
  while integer_start < integer.byte_len() - 1 and integer.byte_slice(integer_start, length: 1) == "0" { integer_start += 1 }
  integer = integer.byte_slice(integer_start)
  let point = if places > 0 or flags.find("#") != null { "." } else { "" }
  let signed = if sign != "" { sign } else if flags.find("+") != null { "+" } else if flags.find(" ") != null { " " } else { "" }
  {text: f"{signed}{integer}{point}{fraction}", valid: true}
}

pure pad_bytes(value: Bytes, width: Int, flags: Str, numeric: Bool, prefix_len: Int, precision_set: Bool) -> Bytes {
  let count = width - value.len()
  return value when count <= 0 or count > 1000000

  let left = flags.find("-") != null
  let zero = numeric and ! left and ! precision_set and flags.find("0") != null
  let byte = if zero { 48 } else { 32 }
  let padding = bytes.from_ints([byte for _ in range(count)]) ?? b""
  if left { bytes.concat([value, padding]) } else if zero and prefix_len > 0 { bytes.concat([value.slice(0, length: prefix_len), padding, value.slice(prefix_len)]) } else { bytes.concat([padding, value]) }
}

pure trim_decimal(text: Str) -> Str {
  var out = text
  return out when out.find(".") == null
  while out.ends_with("0") { out = out.byte_slice(0, out.byte_len() - 1) }
  if out.ends_with(".") { out = out.byte_slice(0, out.byte_len() - 1) }
  out
}

pure exponent_text(exponent: Int, uppercase: Bool) -> Str {
  let marker = if uppercase { "E" } else { "e" }
  let sign = if exponent < 0 { "-" } else { "+" }
  let magnitude = if exponent < 0 { -exponent } else { exponent }
  let digits = digit_text(magnitude, 10, false)
  f"{marker}{sign}{if digits.byte_len() < 2 { "0" } else { "" }}{digits}"
}

pure parse_float_value(raw: Bytes) -> Float {
  let source = raw.utf8() ?? ""
  let text = source.trim()
  var start = 0
  while start < raw.len() and (raw.byte_at(start) ?? 0) in [9, 10, 11, 12, 13, 32] { start += 1 }
  let parsed_character = parse_number(raw, true)
  if (raw.byte_at(start) ?? -1) in [34, 39] and parsed_character.valid {
    return parsed_character.value.float()
  }
  let lower = text.lower()
  if lower.starts_with("0x") or lower.starts_with("+0x") or lower.starts_with("-0x") {
    var at = 0
    var sign = 1.0
    if text.starts_with("-") { sign = -1.0; at = 1 } else if text.starts_with("+") { at = 1 }
    at += 2
    var value = 0.0
    var fraction = 1.0
    var decimal = false
    var exponent = 0
    while at < text.byte_len() {
      let char = text.byte_slice(at, length: 1)
      if char == "." { decimal = true; at += 1; continue }
      if char == "p" or char == "P" {
        exponent = text.byte_slice(at + 1).parse_int() ?? 0
        break
      }
      let d = hex_digit(bytes.from_text(char).byte_at(0) ?? -1)
      if d < 0 { break }
      if decimal { fraction = fraction / 16.0; value += d.float() * fraction } else { value = value * 16.0 + d.float() }
      at += 1
    }
    return sign * value * 2.0.pow(exponent.float())
  }
  var end = text.byte_len()
  var at = 0
  if text.byte_slice(0, length: 1) in ["+", "-"] { at = 1 }
  var digits = 0
  var dot = false
  while at < end {
    let char = text.byte_slice(at, length: 1)
    if char >= "0" and char <= "9" { digits += 1; at += 1; continue }
    if char == "." and ! dot { dot = true; at += 1; continue }
    break
  }
  if digits == 0 { return 0.0 }
  if at < end and text.byte_slice(at, length: 1) in ["e", "E"] {
    let exponent_at = at
    at += 1
    if at < end and text.byte_slice(at, length: 1) in ["+", "-"] { at += 1 }
    let exponent_start = at
    while at < end and text.byte_slice(at, length: 1) >= "0" and text.byte_slice(at, length: 1) <= "9" { at += 1 }
    if at == exponent_start { at = exponent_at }
  }
  text.byte_slice(0, at).parse_float() ?? 0.0
}

pure float_range(raw: Bytes) -> Str {
  var at = 0
  while at < raw.len() and (raw.byte_at(at) ?? 0) in [9, 10, 11, 12, 13, 32] { at += 1 }
  if (raw.byte_at(at) ?? -1) in [34, 39] { return "" }
  let source = raw.slice(at, length: raw.len() - at).utf8() ?? ""
  let text = source.trim()
  let lower = text.lower()
  let is_hex = lower.starts_with("0x") or lower.starts_with("+0x") or lower.starts_with("-0x")
  let marker = if is_hex { "p" } else { "e" }
  if lower.find(marker) == null { return "" }
  let exponent_at = lower.find(marker) ?? 0
  let significand = lower.byte_slice(0, exponent_at)
  var sig_at = 0
  var nonzero = false
  while sig_at < significand.byte_len() {
    let byte = bytes.from_text(significand.byte_slice(sig_at, length: 1)).byte_at(0) ?? 0
    if (byte >= 49 and byte <= 57) or (is_hex and byte >= 97 and byte <= 102) { nonzero = true }
    sig_at += 1
  }
  if ! nonzero { return "" }
  let exponent_text = lower.byte_slice(exponent_at + 1)
  var exponent_index = 0
  var sign = 1
  if exponent_text.starts_with("-") { sign = -1; exponent_index = 1 } else if exponent_text.starts_with("+") { exponent_index = 1 }
  var magnitude = 0
  var found = false
  while exponent_index < exponent_text.byte_len() {
    let char = exponent_text.byte_slice(exponent_index, length: 1)
    if char < "0" or char > "9" { return "" }
    found = true
    magnitude = if magnitude > 1000 { 1001 } else { magnitude * 10 + (char.parse_int() ?? 0) }
    exponent_index += 1
  }
  if ! found { return "" }
  if sign > 0 and magnitude >= (if is_hex { 1024 } else { 309 }) { return "overflow" }
  if sign < 0 and magnitude >= (if is_hex { 1075 } else { 324 }) { return "underflow" }
  ""
}

pure float_input_error(raw: Bytes) -> Str {
  var start = 0
  while start < raw.len() and (raw.byte_at(start) ?? 0) in [9, 10, 11, 12, 13, 32] { start += 1 }
  let parsed_character = parse_number(raw, true)
  if (raw.byte_at(start) ?? -1) in [34, 39] and parsed_character.valid { return "" }
  if float_range(raw) != "" { return "Numerical result out of range" }
  let input = raw.slice(start, length: raw.len() - start).utf8() ?? ""
  let lower = input.lower()
  if lower in ["inf", "+inf", "-inf", "infinity", "+infinity", "-infinity", "nan", "+nan", "-nan"] { return "" }
  if rx"^[+-]?(([0-9]+([.][0-9]*)?)|([.][0-9]+))([eE][+-]?[0-9]+)?$".matches(input) { return "" }
  if rx"^[+-]?0[xX](([0-9A-Fa-f]+([.][0-9A-Fa-f]*)?)|([.][0-9A-Fa-f]+))([pP][+-]?[0-9]+)?$".matches(input) { return "" }
  if rx"^[+-]?[0-9]".matches(input) or rx"^[+-]?[.][0-9]".matches(input) or rx"^[+-]?0[xX]".matches(input) { return "value not completely converted" }
  "expected a numeric value"
}

pure hex_exponent_text(exponent: Int, uppercase: Bool) -> Str {
  f"p{if exponent < 0 { "-" } else { "+" }}{digit_text(if exponent < 0 { -exponent } else { exponent }, 10, false)}".replace("p", if uppercase { "P" } else { "p" })
}

pure hex_float_text(raw: Bytes, conversion: Str, flags: Str) -> Str {
  let source = (raw.utf8() ?? "").trim()
  let number = parse_float_value(raw)
  let uppercase = conversion == "A"
  let marker = if uppercase { "0X" } else { "0x" }
  let sign = if source.starts_with("-") { "-" } else if flags.find("+") != null { "+" } else if flags.find(" ") != null { " " } else { "" }
  let magnitude = if number < 0.0 { -number } else { number }
  if magnitude == 0.0 { return f"{sign}{marker}0p+0" }
  var scaled = magnitude
  var places = 0
  while places < 13 and (scaled - (scaled.floor() ?? 0).float()).abs() > 0.0000000001 {
    scaled *= 16.0
    places += 1
  }
  let integer = (scaled + 0.5).floor() ?? 0
  let digits = digit_text(integer, 16, uppercase)
  f"{sign}{marker}{digits}{hex_exponent_text(-4 * places, uppercase)}"
}

pure float_text(raw: Bytes, conversion: Str, precision: Int?, flags: Str) -> Str {
  let source = (raw.utf8() ?? "").trim()
  let places = precision ?? 6
  let uppercase = conversion == "F" or conversion == "E" or conversion == "G" or conversion == "A"
  let sign = if source.starts_with("-") { "-" } else if flags.find("+") != null { "+" } else if flags.find(" ") != null { " " } else { "" }
  let range = float_range(raw)
  if range == "overflow" { return f"{sign}{if uppercase { "INF" } else { "inf" }}" }
  if range == "underflow" {
    if conversion in ["a", "A"] { return f"{sign}{if uppercase { "0X" } else { "0x" }}0p+0" }
    if conversion in ["e", "E"] { return f"{sign}0{if places > 0 { "." + repeat_text("0", places) } else { "" }}{exponent_text(0, uppercase)}" }
    if conversion in ["g", "G"] {
      let effective_places = if places == 0 { 1 } else { places }
      return f"{sign}{if flags.find("#") != null { "0." + repeat_text("0", effective_places - 1) } else { "0" }}"
    }
    return f"{sign}0{if places > 0 or flags.find("#") != null { "." } else { "" }}{repeat_text("0", places)}"
  }
  let number = parse_float_value(raw)
  if source.lower().find("nan") != null { return f"{sign}{if uppercase { "NAN" } else { "nan" }}" }
  if source.lower().find("inf") != null { return f"{sign}{if uppercase { "INF" } else { "inf" }}" }
  if conversion in ["a", "A"] { return hex_float_text(raw, conversion, flags) }

  if conversion in ["f", "F"] {
    let safe_places = if places > 100 { 100 } else { places }
    let exact = fixed_decimal_text(raw, places, flags)
    if exact.valid { return exact.text }
    let fixed = number.format(safe_places)
    let text = if places > 100 { f"{fixed}{repeat_text("0", places - 100)}" } else { fixed }
    let with_point = if places > 0 or flags.find("#") == null { text } else { f"{text}." }
    return with_point when with_point.starts_with("-")
    if flags.find("+") != null { return f"+{with_point}" }
    if flags.find(" ") != null { return f" {with_point}" }
    return with_point
  }

  let magnitude = if number < 0.0 { -number } else { number }
  if magnitude == 0.0 {
    let zero_sign = if source.starts_with("-") { "-" } else if flags.find("+") != null { "+" } else if flags.find(" ") != null { " " } else { "" }
    if conversion in ["e", "E"] { return f"{zero_sign}0{if places > 0 { "." + repeat_text("0", places) } else { "" }}{exponent_text(0, uppercase)}" }
    if conversion in ["g", "G"] {
      let zero = if flags.find("#") != null { f"0.{repeat_text("0", if places > 0 { places - 1 } else { 0 })}" } else { "0" }
      return f"{zero_sign}{zero}"
    }
  }

  let log_magnitude = magnitude.ln() / 10.0.ln()
  var exponent = (log_magnitude + 0.0000000001).floor() ?? 0
  if conversion in ["e", "E"] or (conversion in ["g", "G"] and (exponent < -4 or exponent >= (if places == 0 { 1 } else { places }))) {
    var scale = 10.0.pow(exponent.float())
    var mantissa = (magnitude / scale).format(if conversion in ["g", "G"] { if places > 0 { places - 1 } else { 0 } } else { places })
    if mantissa.starts_with("10") {
      exponent += 1
      scale *= 10.0
      mantissa = (magnitude / scale).format(if conversion in ["g", "G"] { if places > 0 { places - 1 } else { 0 } } else { places })
    }
    let clean = if conversion in ["g", "G"] and flags.find("#") == null { trim_decimal(mantissa) } else { mantissa }
    let signed = if number < 0.0 { "-" } else if flags.find("+") != null { "+" } else if flags.find(" ") != null { " " } else { "" }
    return f"{signed}{clean}{exponent_text(exponent, uppercase)}"
  }

  let fixed_places = if conversion in ["g", "G"] { if places > 0 { places - 1 - exponent } else { 0 } } else { places }
  let fixed = number.format(if fixed_places < 0 { 0 } else { fixed_places })
  let cleaned = if conversion in ["g", "G"] and flags.find("#") == null { trim_decimal(fixed) } else { fixed }
  return cleaned when cleaned.starts_with("-")
  if flags.find("+") != null { return f"+{cleaned}" }
  if flags.find(" ") != null { return f" {cleaned}" }
  cleaned
}

proc format_spec(spec: ParsedSpec, args: List[Bytes], cursor: Int, batch_start: Int) [process, env, error] -> SpecOutput {
  var next = cursor
  var max_position = 0
  var flags = spec.flags
  var width = spec.width ?? 0
  var precision_input = b""
  var pending_failure = ""
  if spec.width_star {
    let width_at = if spec.width_position == null { next } else { batch_start + (spec.width_position ?? 1) - 1 }
    if spec.width_position != null and (spec.width_position ?? 0) > max_position { max_position = spec.width_position ?? 0 }
    let parsed = parse_number(args.get(width_at) ?? b"", true)
    width = parsed.value
    if spec.width_position == null { next += 1 }
    if width == INT_MIN { width = 1000001; if flags.find("-") == null { flags = f"{flags}-" } } else if width < 0 { width = -width; if flags.find("-") == null { flags = f"{flags}-" } }
  }

  var precision = spec.precision
  if spec.precision_star {
    let precision_at = if spec.precision_position == null { next } else { batch_start + (spec.precision_position ?? 1) - 1 }
    if spec.precision_position != null and (spec.precision_position ?? 0) > max_position { max_position = spec.precision_position ?? 0 }
    precision_input = args.get(precision_at) ?? b""
    let parsed = parse_number(precision_input, true)
    let raw_precision = precision_input.utf8() ?? ""
    if parsed.overflow and raw_precision != "-9223372036854775808" {
      pending_failure = f"{gnu.quote_value_bytes(precision_input)}: Numerical result out of range"
    } else if parsed.value > 2147483647 {
      pending_failure = f"invalid precision: {gnu.quote_value_bytes(precision_input)}"
    }
    precision = if parsed.value < 0 { null } else { parsed.value }
    if spec.precision_position == null { next += 1 }
  }
  if precision != null and (precision ?? 0) > 1000000 and pending_failure == "" { pending_failure = "invalid precision" }

  let conv = spec.conversion
  if conv == "" {
    return {out: b"", cursor: next, max_position: max_position, stop: true, failed: true, failure: f"{spec.shown}: invalid conversion specification", warnings: []}
  }
  if ! (conv in ["s", "b", "q", "c", "d", "i", "u", "o", "x", "X", "f", "F", "e", "E", "g", "G", "a", "A"]) {
    return {out: b"", cursor: next, max_position: max_position, stop: true, failed: true, failure: f"{spec.shown}: invalid conversion specification", warnings: []}
  }
  let overflow_precision = pending_failure.find("Numerical result out of range") != null
  if pending_failure != "" and ! overflow_precision {
    return {out: b"", cursor: next, max_position: max_position, stop: true, failed: true, failure: pending_failure, warnings: []}
  }
  if overflow_precision { precision = null }
  if spec.position == 0 or spec.width_position == 0 or spec.precision_position == 0 {
    let shown = if spec.position == 0 { f"%{spec.position ?? 0}$" } else { spec.shown }
    return {out: b"", cursor: next, max_position: max_position, stop: true, failed: true, failure: f"{shown}: invalid conversion specification", warnings: []}
  }

  let value_at = if spec.position == null { next } else { batch_start + (spec.position ?? 1) - 1 }
  if spec.position != null and (spec.position ?? 0) > max_position { max_position = spec.position ?? 0 }
  let has_raw = value_at >= 0 and value_at < args.len()
  let raw = args.get(value_at) ?? b""
  if spec.position == null { next += 1 }

  if conv == "q" and (spec.width != null or spec.width_star or precision != null or spec.precision_star or flags != "") {
    return {out: b"", cursor: next, max_position: max_position, stop: true, failed: true, failure: f"{spec.shown}: invalid conversion specification", warnings: []}
  }
  if conv == "b" and (spec.width != null or spec.width_star or precision != null or spec.precision_star or flags != "") {
    return {out: b"", cursor: next, max_position: max_position, stop: true, failed: true, failure: f"{spec.shown}: invalid conversion specification", warnings: []}
  }
  if conv in ["s", "b", "c", "q"] and flags.find("0") != null {
    return {out: b"", cursor: next, max_position: max_position, stop: true, failed: true, failure: f"{spec.shown}: invalid conversion specification", warnings: []}
  }
  if conv == "s" and (flags.find("#") != null or flags.find("'") != null) {
    return {out: b"", cursor: next, max_position: max_position, stop: true, failed: true, failure: f"{spec.shown}: invalid conversion specification", warnings: []}
  }
  if conv == "c" and (flags.find("#") != null or precision != null or spec.precision_star) {
    return {out: b"", cursor: next, max_position: max_position, stop: true, failed: true, failure: f"{spec.shown}: invalid conversion specification", warnings: []}
  }
  if conv in ["d", "i", "u"] and flags.find("#") != null {
    return {out: b"", cursor: next, max_position: max_position, stop: true, failed: true, failure: f"{spec.shown}: invalid conversion specification", warnings: []}
  }

  var rendered = b""
  var numeric = false
  var prefix_len = 0
  var stop = false
  var precision_set = precision != null
  var failed = overflow_precision
  var failure = pending_failure
  var warnings: List[Str] = []

  if conv == "s" {
    rendered = if precision == null { raw } else { raw.slice(0, length: precision ?? 0) }
  } else if conv == "b" {
    var pieces: List[Bytes] = []
    var at = 0
    while at < raw.len() {
      if (raw.byte_at(at) ?? -1) == 92 {
        let decoded = escape(raw, at, true)
        if decoded.failure != "" { return {out: b"", cursor: next, max_position: max_position, stop: true, failed: true, failure: decoded.failure, warnings: []} }
        pieces += [decoded.out]
        at = decoded.next
        if decoded.stop { stop = true; break }
      } else {
        pieces += [raw.slice(at, length: 1)]
        at += 1
      }
    }
    rendered = bytes.concat(pieces)
  } else if conv == "q" {
    let quoted = gnu.quote_bytes(raw, always: false)
    rendered = bytes.from_text(quoted)
    if precision != null { rendered = rendered.slice(0, length: precision ?? 0) }
  } else if conv == "c" {
    let parsed = parse_number(raw, true)
    rendered = if parsed.valid { codepoint_bytes(parsed.value) } else { raw.slice(0, length: 1) }
    if parsed.char_tail.len() > 0 and env.get("POSIXLY_CORRECT") is Err(_) {
      warnings += [f"warning: {parsed.char_tail.utf8() ?? ""}: character(s) following character constant have been ignored"]
    }
  } else if conv in ["d", "i", "u", "o", "x", "X"] {
    numeric = true
    let parsed = parse_number(raw, conv != "d" and conv != "u")
    let original_number = parsed.value
    var number = parsed.value
    if parsed.overflow {
      failed = true
      failure = f"{gnu.quote_value_bytes(raw)}: Numerical result out of range"
    } else if has_raw and ! parsed.valid {
      failed = true
      failure = f"{gnu.quote_value_bytes(raw)}: expected a numeric value"
    } else if has_raw and ! parsed.complete {
      failed = true
      failure = f"{gnu.quote_value_bytes(raw)}: value not completely converted"
    }
    if parsed.char_tail.len() > 0 and env.get("POSIXLY_CORRECT") is Err(_) {
      warnings += [f"warning: {parsed.char_tail.utf8() ?? ""}: character(s) following character constant have been ignored"]
    }
    let negative = original_number < 0 and conv in ["d", "i"]
    let is_minimum = number == INT_MIN
    if number < 0 { number = if is_minimum { 9223372036854775807 } else { -number } }
    let base = if conv == "o" { 8 } else if conv in ["x", "X"] { 16 } else { 10 }
    var digits = digit_text(number, base, conv == "X")
    if parsed.overflow and conv in ["d", "i"] {
      digits = if (raw.utf8() ?? "").starts_with("-") { "9223372036854775808" } else { "9223372036854775807" }
    } else if parsed.overflow and conv in ["u", "o", "x", "X"] {
      digits = if conv == "u" { "18446744073709551615" } else if conv == "o" { "1777777777777777777777" } else { repeat_text(if conv == "X" { "F" } else { "f" }, 16) }
    } else if original_number < 0 and conv in ["x", "X"] {
      digits = twos_complement_hex(number, conv == "X")
    } else if original_number < 0 and conv in ["u", "o"] {
      let unsigned_decimal = unsigned_decimal_from_negative(number)
      digits = if conv == "u" { unsigned_decimal } else { unsigned_base_from_decimal(unsigned_decimal, 8, false) }
    }
    if is_minimum and ! parsed.overflow {
      digits = if conv in ["d", "i"] { "9223372036854775808" } else if conv == "u" { "9223372036854775808" } else if conv == "o" { "1000000000000000000000" } else { if conv == "X" { "8000000000000000" } else { "8000000000000000" } }
    }
    if number == 0 and precision == 0 { digits = "" }
    if precision != null and (precision ?? 0) > digits.byte_len() {
      digits = f"{repeat_text("0", (precision ?? 0) - digits.byte_len())}{digits}"
    }
    let sign = if negative { "-" } else if conv in ["d", "i"] and flags.find("+") != null { "+" } else if conv in ["d", "i"] and flags.find(" ") != null { " " } else { "" }
    let prefix = if flags.find("#") != null and original_number != 0 and conv in ["x", "X"] { if conv == "X" { "0X" } else { "0x" } } else if flags.find("#") != null and conv == "o" and ! digits.starts_with("0") { "0" } else { "" }
    let shown = f"{sign}{prefix}{digits}"
    prefix_len = sign.byte_len() + prefix.byte_len()
    rendered = bytes.from_text(shown)
    precision_set = precision != null
  } else {
    numeric = true
    let shown = float_text(raw, conv, precision, flags)
    if has_raw {
      let parse_error = float_input_error(raw)
      if parse_error != "" { failed = true; failure = f"{gnu.quote_value_bytes(raw)}: {parse_error}" }
    }
    prefix_len = if shown.starts_with("-") or shown.starts_with("+") or shown.starts_with(" ") { 1 } else if shown.starts_with("0x") or shown.starts_with("0X") { 2 } else { 0 }
    rendered = bytes.from_text(shown)
  }

  if precision != null and (conv == "f" or conv == "F") and (precision ?? 0) > 100 {
    if (precision ?? 0) > 1000000 { return {out: b"", cursor: next, max_position: max_position, stop: true, failed: true, failure: "invalid precision", warnings: []} }
  }
  if precision != null and (precision ?? 0) > 1000000 {
    return {out: b"", cursor: next, max_position: max_position, stop: true, failed: true, failure: "invalid precision", warnings: []}
  }
  if width > 1000000 {
    return {out: b"", cursor: next, max_position: max_position, stop: true, failed: true, failure: "write error: No space left on device", warnings: []}
  }
  if conv in ["f", "F", "e", "E", "g", "G", "a", "A"] {
    let lower = raw.utf8() ?? ""
    precision_set = lower.lower().find("nan") != null or lower.lower().find("inf") != null
  }
  rendered = pad_bytes(rendered, width, flags, numeric, prefix_len, precision_set)
  {out: rendered, cursor: next, max_position: max_position, stop: stop, failed: failed, failure: failure, warnings: warnings}
}

proc format_pass(format: Bytes, args: List[Bytes], cursor: Int) [process, env, error] -> PassOutput {
  var pieces: List[Bytes] = []
  var at = 0
  var next = cursor
  var has_spec = false
  var stop = false
  var failed = false
  var failure = ""
  var failure_start = -1
  var failure_length = 0
  var warnings: List[Str] = []
  var max_position = 0
  while at < format.len() and ! stop {
    let byte = format.byte_at(at) ?? -1
    if byte == 92 {
      let decoded = escape(format, at, false)
      if decoded.failure != "" { return {out: bytes.concat(pieces), cursor: next, has_spec: has_spec, stop: true, failed: true, failure: decoded.failure, failure_start: at, failure_length: decoded.next - at, warnings: warnings} }
      pieces += [decoded.out]
      at = decoded.next
      stop = decoded.stop
    } else if byte == 37 {
      if (format.byte_at(at + 1) ?? -1) == 37 {
        pieces += [b"%"]
        at += 2
      } else {
        let parsed = parse_spec(format, at)
        let output = format_spec(parsed, args, next, cursor)
        pieces += [output.out]
        next = output.cursor
        if output.max_position > max_position { max_position = output.max_position }
        failed = failed or output.failed
        if output.failed and failure == "" {
          failure = output.failure
          failure_start = at
          failure_length = parsed.end - at
        }
        warnings += output.warnings
        has_spec = has_spec or (parsed.conversion != "" and parsed.conversion != "%")
        stop = output.stop
        at = parsed.end
      }
    } else {
      var end = at + 1
      while end < format.len() and (format.byte_at(end) ?? -1) != 92 and (format.byte_at(end) ?? -1) != 37 { end += 1 }
      pieces += [format.slice(at, length: end - at)]
      at = end
    }
  }
  {out: bytes.concat(pieces), cursor: if cursor + max_position > next { cursor + max_position } else { next }, has_spec: has_spec, stop: stop, failed: failed, failure: failure, failure_start: failure_start, failure_length: failure_length, warnings: warnings}
}

type ErrorSnippet = {start: Int, length: Int, help: Str}

pure shell_word(value: Str) -> Str {
  return value when value.find(" ") == null and value.find("\t") == null and value.find("\n") == null and value.find("\r") == null
  "'" + value.replace("'", "'\\''") + "'"
}

pure invocation_source(argv: List[Str]) -> Str {
  var words = ["printf"]
  for item in argv { words += [shell_word(item)] }
  words.join(" ")
}

pure display_columns(value: Str) -> Int {
  let data = bytes.from_text(value)
  var at = 0
  var columns = 0
  while at < data.len() {
    let byte = data.byte_at(at) ?? 0
    let width = if byte < 128 { 1 } else if byte < 224 { 2 } else if byte < 240 { 3 } else { 4 }
    at += width
    columns += 1
  }
  columns
}

pure rendered_start(argv: List[Str], arg_index: Int, byte_offset: Int) -> Int {
  var offset = display_columns("printf")
  for index in range(argv.len()) {
    offset += 1
    let word = shell_word(argv[index])
    if index == arg_index {
      let value_prefix = argv[index].byte_slice(0, length: byte_offset)
      let rendered_prefix = if word.starts_with("'") { "'" + value_prefix.replace("'", "'\\''") } else { value_prefix }
      return offset + display_columns(rendered_prefix)
    }
    offset += display_columns(word)
  }
  offset
}

pure format_error_snippet(message: Str, start: Int, length: Int) -> ErrorSnippet? {
  let help = "%d, %s, %x, %f and the other C conversions are accepted, plus %b and %q; a literal % is written %%"
  if message.find("invalid conversion specification") != null {
    return {start: start, length: length, help: help}
  }
  if message == "missing hexadecimal number in escape" {
    return {start: start, length: length, help: "\\x takes one or two hexadecimal digits, \\u takes four and \\U takes eight"}
  }
  if message.starts_with("invalid universal character name ") {
    return {start: start, length: length, help: "code points between D800 and DFFF or above 10FFFF are not Unicode characters"}
  }
  null
}

proc uutils_adapter() [env] -> Bool {
  let phrase = env.get_or("XSH_EXECUTION_PHRASE", "") ?? ""
  phrase.ends_with("xsh-uutests printf")
}

proc snippet_error(argv: List[Str], arg_index: Int, message: Str, details: ErrorSnippet) [process, env, io, error] {
  gnu.error(message)
  let source = invocation_source(argv)
  let start = rendered_start(argv, arg_index, details.start)
  eprint f"   ╭─[ printf:1:{start + 1} ]"
  eprint "   │"
  eprint f" 1 │ {source}"
  eprint f"   │ {repeat_text(" ", start)}{repeat_text("─", details.length)}"
  eprint "   │"
  eprint f"   │ Help: {details.help}"
  eprint "───╯"
  exit 1
}

proc main(...argv: List[Str]) [process, env, error, io] {
  let raw = cli.argv_bytes()
  if argv.len() > 0 and argv[0] == "--help" { gnu.help(USAGE); return }
  if argv.len() > 0 and argv[0] == "--version" { gnu.version("printf"); return }
  if argv.len() > 0 and (argv[0].starts_with("--help=") or argv[0].starts_with("--version=")) {
    gnu.usage_error("option does not allow an argument")
  }

  let start = if argv.len() > 0 and argv[0] == "--" { 1 } else { 0 }
  if start >= raw.len() { gnu.missing_operand() }
  let format = raw[start]
  var args: List[Bytes] = []
  var at = start + 1
  while at < raw.len() { args += [raw[at]]; at += 1 }

  var output: List[Bytes] = []
  var cursor = 0
  var first = true
  var stop = false
  var failed = false
  var failure = ""
  var failure_start = -1
  var failure_length = 0
  var warnings: List[Str] = []
  while first or cursor < args.len() {
    let pass = format_pass(format, args, cursor)
    output += [pass.out]
    stop = pass.stop
    failed = failed or pass.failed
    if pass.failed and failure == "" {
      failure = pass.failure
      failure_start = pass.failure_start
      failure_length = pass.failure_length
    }
    warnings += pass.warnings
    first = false
    if stop or ! pass.has_spec or pass.cursor <= cursor { break }
    cursor = pass.cursor
    if cursor >= args.len() { break }
  }
  gnu.write_bytes(bytes.concat(output))
  if let Err(failure) = io.flush_stdout() { gnu.write_failed(failure) }
  if failure != "" {
    if uutils_adapter() and unix.isatty(2) and failure_start >= 0 {
      if let details = format_error_snippet(failure, failure_start, failure_length) {
        snippet_error(argv, start, failure, details)
      }
    }
    gnu.error(failure)
  }
  for warning in warnings { gnu.error(warning) }
  if failed { exit 1 }

  if cursor < args.len() and ! stop {
    gnu.error(f"warning: ignoring excess arguments, starting with {gnu.quote_value_bytes(args[cursor])}")
  }
}
