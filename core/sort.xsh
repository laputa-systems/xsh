#!/bin/xsh
use lib.gnu
use lib.text_a2 as text

type SortOptions = {
  reverse: Bool,
  unique: Bool,
  numeric: Bool,
  human_numeric: Bool,
  general_numeric: Bool,
  fold_case: Bool,
  dictionary: Bool,
  ignore_nonprinting: Bool,
  blank: Bool,
  stable: Bool,
  version_sort: Bool,
  sort_mode: List[Str],
  debug: Bool,
  key: Str,
  delimiter: Str,
  output: List[Str],
  check: Str,
  short_check: Bool,
  silent_check: Bool,
  zero_terminated: Bool,
  version: Bool,
  paths: List[Str],
}

type NumericSortKey = {number: Str, raw: Str}
type HumanNumericSortKey = {key: Str, raw: Str}
type GeneralNumericSortKey = {key: Str, raw: Str}
type TextSortKey = {key: Str, raw: Str}

pure numeric_key(line: Str) -> Str {
  numeric_order_key(line)
}

pure numeric_sort_key(line: Str, stable: Bool) -> NumericSortKey {
  {number: numeric_key(line), raw: if stable { "" } else { line }}
}

## GNU sort compares the suffix order before the decimal value, so 1000M sorts before 1G.
pure human_numeric_sort_key(line: Str, stable: Bool) -> HumanNumericSortKey {
  let value = line.trim()
  let prefix = decimal_prefix(value, true)
  let unsigned = if prefix.starts_with("+") { prefix.byte_slice(1) } else { prefix }
  let number = numeric_order_key(unsigned)
  let unit_byte = if prefix == "" { 0 } else { value.byte_at(prefix.byte_len()) ?? 0 }
  let unit = if number == "1" {
    0
  } else if prefix.starts_with("-") {
    0 - human_unit_order(unit_byte)
  } else {
    human_unit_order(unit_byte)
  }
  {key: f"{padded_decimal(unit + 10, 2)}{number}", raw: if stable { "" } else { line }}
}

pure padded_decimal(value: Int, width: Int) -> Str {
  let digits = f"{value}"
  var padding = ""
  while padding.byte_len() + digits.byte_len() < width { padding += "0" }
  padding + digits
}

pure decimal_prefix(line: Str, allow_plus: Bool) -> Str {
  let value = line.trim()
  if value.starts_with("+") and ! allow_plus { return "" }
  let sign_length = if value.starts_with("-") or value.starts_with("+") { 1 } else { 0 }
  let input = bytes.from_text(value)
  var at = sign_length
  var digits = 0
  while at < input.len() and is_ascii_digit(input.byte_at(at) ?? 0) {
    at += 1
    digits += 1
  }
  if at < input.len() and input.byte_at(at) == 46 {
    at += 1
    while at < input.len() and is_ascii_digit(input.byte_at(at) ?? 0) {
      at += 1
      digits += 1
    }
  }
  if digits == 0 { return "" }
  value.byte_slice(0, length: at)
}

pure numeric_prefix(line: Str) -> Str {
  decimal_prefix(line, false)
}

pure human_numeric_prefix(line: Str) -> Str {
  let value = line.trim()
  let prefix = decimal_prefix(value, true)
  if prefix == "" { return "" }
  let unit = value.byte_at(prefix.byte_len()) ?? 0
  if human_unit_order(unit) > 0 {
    prefix + value.byte_slice(prefix.byte_len(), length: 1)
  } else {
    prefix
  }
}

pure human_unit_order(unit: Int) -> Int {
  if unit in [75, 107] { return 1 }
  if unit == 77 { return 2 }
  if unit == 71 { return 3 }
  if unit == 84 { return 4 }
  if unit == 80 { return 5 }
  if unit == 69 { return 6 }
  if unit == 90 { return 7 }
  if unit == 89 { return 8 }
  if unit == 82 { return 9 }
  if unit == 81 { return 10 }
  0
}

pure human_numeric_field_sort_key(line: Str, delimiter: Str, field: Int, opts: SortOptions, stable: Bool) -> HumanNumericSortKey {
  let parts = if delimiter == "" { line.trim().words() } else { line.split(delimiter) }
  let text = parts.get(field) ?? ""
  let selected = text.split("") |> drop(key_character_offset(opts.key)).join("")
  let key = human_numeric_sort_key(selected, true)
  {key: key.key, raw: if stable { "" } else { line }}
}

pure invert_decimal_digits(digits: Str) -> Str {
  var inverted = ""
  for at in range(digits.byte_len()) {
    inverted += f"{9 - ((digits.byte_at(at) ?? 48) - 48)}"
  }
  inverted
}

pure numeric_exponent_key(exponent: Int) -> Str {
  if exponent < 0 {
    let magnitude = 0 - exponent
    "0" + invert_decimal_digits(padded_decimal(magnitude, 19))
  } else if exponent == 0 {
    "1" + padded_decimal(0, 19)
  } else {
    "2" + padded_decimal(exponent, 19)
  }
}

pure invert_numeric_exponent_key(key: Str) -> Str {
  var inverted = ""
  for at in range(key.byte_len()) {
    let byte = key.byte_at(at) ?? 48
    if at == 0 {
      inverted += if byte == 48 { "2" } else if byte == 50 { "0" } else { "1" }
    } else {
      inverted += f"{9 - (byte - 48)}"
    }
  }
  inverted
}

## Encode decimal magnitude and sign as text so numeric sorting never rounds or narrows the input.
pure numeric_order_key(line: Str) -> Str {
  let prefix = numeric_prefix(line)
  if prefix == "" { return "1" }
  let negative = prefix.starts_with("-")
  let sign_length = if negative { 1 } else { 0 }
  let input = bytes.from_text(prefix)
  var digits = ""
  var decimal_position = 0
  var after_decimal = false
  for at in range(sign_length, input.len()) {
    let byte = input.byte_at(at) ?? 0
    if byte == 46 {
      after_decimal = true
    } else {
      digits += prefix.byte_slice(at, length: 1)
      if ! after_decimal { decimal_position += 1 }
    }
  }

  var first_significant = 0
  while first_significant < digits.byte_len() and digits.byte_slice(first_significant, length: 1) == "0" {
    first_significant += 1
  }
  if first_significant == digits.byte_len() { return "1" }
  let decimal_exponent = decimal_position - first_significant - 1
  var significant = digits.byte_slice(first_significant)
  while significant.ends_with("0") and significant.byte_len() > 1 {
    significant = significant.byte_slice(0, length: significant.byte_len() - 1)
  }

  if negative {
    "0" + invert_numeric_exponent_key(numeric_exponent_key(decimal_exponent)) + invert_decimal_digits(significant) + ":"
  } else {
    "2" + numeric_exponent_key(decimal_exponent) + significant + "!"
  }
}

pure hex_digit_value(byte: Int) -> Int {
  if is_ascii_digit(byte) { return byte - 48 }
  if byte >= 65 and byte <= 70 { return byte - 55 }
  if byte >= 97 and byte <= 102 { return byte - 87 }
  -1
}

pure general_hex_value(value: Str, sign_length: Int) -> Float {
  let input = bytes.from_text(value)
  var at = sign_length + 2
  var number = 0.0
  var fractional = false
  var fraction_place = 1.0 / 16.0
  var digits = 0
  while at < input.len() {
    let byte = input.byte_at(at) ?? 0
    if byte == 46 and ! fractional {
      fractional = true
    } else {
      let digit = hex_digit_value(byte)
      if digit < 0 { break }
      digits += 1
      if fractional {
        number += digit.float() * fraction_place
        fraction_place /= 16.0
      } else {
        number = number * 16.0 + digit.float()
      }
    }
    at += 1
  }
  if digits == 0 { return 0.0 }

  if at < input.len() and (input.byte_at(at) ?? 0) in [80, 112] {
    at += 1
    let exponent_start = at
    if at < input.len() and (input.byte_at(at) ?? 0) in [43, 45] { at += 1 }
    let exponent_digits = at
    while at < input.len() and is_ascii_digit(input.byte_at(at) ?? 0) { at += 1 }
    if at > exponent_digits {
      let exponent = value.byte_slice(exponent_start, length: at - exponent_start).parse_int() ?? 0
      number *= 2.0.pow(exponent.float())
    }
  }

  if sign_length == 1 and value.starts_with("-") { 0.0 - number } else { number }
}

pure general_numeric_prefix(line: Str) -> Str {
  let value = line.trim()
  let lower = value.lower()
  var sign_length = 0
  if value.starts_with("-") or value.starts_with("+") { sign_length = 1 }

  let unsigned = lower.byte_slice(sign_length)
  if unsigned.starts_with("nan") { return value.byte_slice(0, length: sign_length + 3) }
  if unsigned.starts_with("infinity") { return value.byte_slice(0, length: sign_length + 8) }
  if unsigned.starts_with("inf") { return value.byte_slice(0, length: sign_length + 3) }

  let input = bytes.from_text(value)
  if unsigned.starts_with("0x") {
    var at = sign_length + 2
    var digits = 0
    var fractional = false
    while at < input.len() {
      let byte = input.byte_at(at) ?? 0
      if byte == 46 and ! fractional {
        fractional = true
      } else {
        if hex_digit_value(byte) < 0 { break }
        digits += 1
      }
      at += 1
    }
    let mantissa_end = at
    if at < input.len() and (input.byte_at(at) ?? 0) in [80, 112] {
      at += 1
      if at < input.len() and (input.byte_at(at) ?? 0) in [43, 45] { at += 1 }
      let exponent_start = at
      while at < input.len() and is_ascii_digit(input.byte_at(at) ?? 0) { at += 1 }
      if at == exponent_start { at = mantissa_end }
    }
    if digits == 0 { at = sign_length + 1 }
    return value.byte_slice(0, length: at)
  }

  var at = sign_length
  var digits = 0
  while at < input.len() and is_ascii_digit(input.byte_at(at) ?? 0) {
    at += 1
    digits += 1
  }
  if at < input.len() and input.byte_at(at) == 46 {
    at += 1
    while at < input.len() and is_ascii_digit(input.byte_at(at) ?? 0) {
      at += 1
      digits += 1
    }
  }
  if digits == 0 { return "" }

  let mantissa_end = at
  if at < input.len() and (input.byte_at(at) ?? 0) in [69, 101] {
    at += 1
    if at < input.len() and (input.byte_at(at) ?? 0) in [43, 45] { at += 1 }
    let exponent_start = at
    while at < input.len() and is_ascii_digit(input.byte_at(at) ?? 0) { at += 1 }
    if at == exponent_start { at = mantissa_end }
  }

  value.byte_slice(0, length: at)
}

pure general_numeric_value(line: Str) -> Float {
  let prefix = general_numeric_prefix(line)
  if prefix == "" { return 0.0 }
  var sign_length = 0
  if prefix.starts_with("-") or prefix.starts_with("+") { sign_length = 1 }
  if prefix.lower().byte_slice(sign_length).starts_with("0x") {
    general_hex_value(prefix, sign_length)
  } else {
    prefix.parse_float() ?? 0.0
  }
}

## A biased exponent and padded significand preserve numeric order because sort-by has no Float key.
pure general_numeric_sort_key(line: Str, stable: Bool) -> GeneralNumericSortKey {
  let number = general_numeric_value(line)
  let special = number.format()
  if special == "NaN" { return {key: "0", raw: if stable { "" } else { line }} }
  if special == "-Infinity" { return {key: "1", raw: if stable { "" } else { line }} }
  if special == "Infinity" { return {key: "5", raw: if stable { "" } else { line }} }
  let display = if let Ok(exact) = number.format_number("g", 17) { exact } else { special }
  if display in ["0", "-0"] { return {key: "3", raw: if stable { "" } else { line }} }

  let negative = display.starts_with("-")
  let unsigned = if negative { display.byte_slice(1) } else { display }
  var exponent_at = 0
  while exponent_at < unsigned.byte_len() and unsigned.byte_slice(exponent_at, length: 1) not in ["e", "E"] {
    exponent_at += 1
  }
  let mantissa = unsigned.byte_slice(0, length: exponent_at)
  let exponent = if exponent_at < unsigned.byte_len() { unsigned.byte_slice(exponent_at + 1).parse_int() ?? 0 } else { 0 }
  var digits = ""
  var decimal_position = 0
  var after_decimal = false
  for at in range(mantissa.byte_len()) {
    let byte = mantissa.byte_at(at) ?? 0
    if byte == 46 {
      after_decimal = true
    } else {
      digits += mantissa.byte_slice(at, length: 1)
      if ! after_decimal { decimal_position += 1 }
    }
  }

  var first_significant = 0
  while first_significant < digits.byte_len() and digits.byte_slice(first_significant, length: 1) == "0" {
    first_significant += 1
  }
  let significant = digits.byte_slice(first_significant)
  var normalized = significant
  while normalized.ends_with("0") and normalized.byte_len() > 1 {
    normalized = normalized.byte_slice(0, length: normalized.byte_len() - 1)
  }
  while normalized.byte_len() < 17 { normalized += "0" }

  let decimal_exponent = decimal_position - first_significant - 1 + exponent
  let positive_exponent = decimal_exponent + 400
  let ordered_exponent = if negative { 999 - positive_exponent } else { positive_exponent }
  var significance = normalized
  if negative {
    significance = ""
    for at in range(normalized.byte_len()) {
      significance += f"{9 - ((normalized.byte_at(at) ?? 48) - 48)}"
    }
  }
  {key: f"{if negative { "2" } else { "4" }}{padded_decimal(ordered_exponent, 3)}{significance}", raw: if stable { "" } else { line }}
}

pure numeric_field_sort_key(line: Str, delimiter: Str, field: Int, opts: SortOptions) -> NumericSortKey {
  {number: numeric_field_key(line, delimiter, field, opts), raw: if opts.stable or opts.unique { "" } else { line }}
}

pure key_index(spec: Str) -> Int {
  (((spec.split(",").get(0) ?? "1").split(".").get(0) ?? "1").parse_int() ?? 1) - 1
}

pure key_character_offset(spec: Str) -> Int {
  let start = (spec.split(",").get(0) ?? "").split(".").get(1) ?? ""
  let input = bytes.from_text(start)
  var end = 0
  while end < input.len() and is_ascii_digit(input.byte_at(end) ?? 0) { end += 1 }
  if end == 0 { return 0 }
  let position = start.byte_slice(0, length: end).parse_int() ?? 1
  if position > 0 { position - 1 } else { 0 }
}

pure key_reversed(spec: Str) -> Bool {
  (spec.split(",").get(0) ?? "").ends_with("r")
}

pure is_ascii_digit(byte: Int) -> Bool {
  byte >= 48 and byte <= 57
}

pure is_ascii_letter(byte: Int) -> Bool {
  (byte >= 65 and byte <= 90) or (byte >= 97 and byte <= 122)
}

pure version_hex_byte(byte: Int) -> Str {
  let digits = "0123456789ABCDEF"
  digits.byte_slice(byte / 16, length: 1) + digits.byte_slice(byte % 16, length: 1)
}

## Encode natural version chunks into a lexically sortable key without narrowing numeric runs.
pure version_key(line: Str) -> Str {
  if line == "" { return "00" }
  if line == "." { return "01" }
  if line == ".." { return "02" }

  var remainder = line
  var key = ""
  while remainder.starts_with(".") {
    key += "03"
    remainder = remainder.byte_slice(1)
  }
  key += "04"

  let input = bytes.from_text(remainder)
  var at = 0
  while at < input.len() {
    let byte = input.byte_at(at) ?? 0
    if is_ascii_digit(byte) {
      var end = at + 1
      while end < input.len() and is_ascii_digit(input.byte_at(end) ?? 0) { end += 1 }

      var significant = at
      while significant < end and input.byte_at(significant) == 48 { significant += 1 }
      let significant_length = end - significant
      key += "01"
      for _ in range(significant_length) { key += "1" }
      key += "0"
      while significant < end {
        key += remainder.byte_slice(significant, length: 1)
        significant += 1
      }
      at = end
    } else if byte == 126 {
      key += "00"
      at += 1
    } else {
      key += if is_ascii_letter(byte) { "02" } else { "03" }
      key += version_hex_byte(byte)
      at += 1
    }
  }
  key + "01"
}

pure version_sort_key(line: Str, stable: Bool) -> TextSortKey {
  {key: version_key(line), raw: if stable { "" } else { line }}
}

pure version_field_sort_key(line: Str, delimiter: Str, field: Int, opts: SortOptions) -> TextSortKey {
  let parts = if delimiter == "" { line.trim().words() } else { line.split(delimiter) }
  {key: version_key(parts.get(field) ?? ""), raw: if opts.stable { "" } else { line }}
}

pure selected_sort_mode(opts: SortOptions) -> Str {
  opts.sort_mode.get(opts.sort_mode.len() - 1) ?? ""
}

pure is_version_sort(opts: SortOptions) -> Bool {
  opts.version_sort or selected_sort_mode(opts).starts_with("v")
}

pure is_general_numeric_sort(opts: SortOptions) -> Bool {
  let mode = selected_sort_mode(opts)
  if mode == "" { opts.general_numeric } else { mode in ["g", "general-numeric"] }
}

pure is_numeric_sort(opts: SortOptions) -> Bool {
  let mode = selected_sort_mode(opts)
  if mode == "" { opts.numeric } else { mode in ["n", "numeric"] }
}

pure is_human_numeric_mode(mode: Str) -> Bool {
  mode != "" and (mode in ["h", "human"] or "human-numeric".starts_with(mode))
}

pure is_human_numeric_sort(opts: SortOptions) -> Bool {
  let mode = selected_sort_mode(opts)
  if mode == "" { opts.human_numeric } else { is_human_numeric_mode(mode) }
}

pure character_order_key(text: Str, dictionary: Bool, ignore_nonprinting: Bool, fold_case: Bool) -> Str {
  if ! dictionary and ! ignore_nonprinting {
    return if fold_case { text.upper() } else { text }
  }

  let input = bytes.from_text(text)
  var key = ""
  for index in range(input.len()) {
    let byte = input.byte_at(index) ?? 0
    let blank = byte == 9 or byte == 32
    let alphanumeric = (byte >= 48 and byte <= 57) or (byte >= 65 and byte <= 90) or (byte >= 97 and byte <= 122)
    let in_dictionary = ! dictionary or blank or alphanumeric
    let printable = byte >= 32 and byte <= 126
    let in_printable = ! ignore_nonprinting or printable
    if in_dictionary and in_printable {
      key += text.byte_slice(index, length: 1)
    }
  }
  if fold_case { key.upper() } else { key }
}

pure field_key(line: Str, delimiter: Str, field: Int, opts: SortOptions) -> Str {
  let parts = if delimiter == "" { line.trim().words() } else { line.split(delimiter) }
  character_order_key(parts.get(field) ?? "", opts.dictionary, opts.ignore_nonprinting, opts.fold_case)
}

pure field_sort_key(line: Str, delimiter: Str, field: Int, opts: SortOptions) -> TextSortKey {
  {key: field_key(line, delimiter, field, opts), raw: if opts.stable { "" } else { line }}
}

pure numeric_field_key(line: Str, delimiter: Str, field: Int, opts: SortOptions) -> Str {
  let parts = if delimiter == "" { line.trim().words() } else { line.split(delimiter) }
  let text = parts.get(field) ?? ""
  let key = text.split("") |> drop(key_character_offset(opts.key)).join("")
  numeric_key(key)
}

pure trim_leading_blanks(line: Str) -> Str {
  var at = 0
  while at < line.byte_len() and line.byte_slice(at, length: 1) in [" ", "\t"] { at += 1 }
  line.byte_slice(at)
}

pure blank_text_key(line: Str, opts: SortOptions) -> Str {
  let key = trim_leading_blanks(line)
  character_order_key(key, opts.dictionary, opts.ignore_nonprinting, opts.fold_case)
}

pure blank_sort_key(line: Str, opts: SortOptions) -> TextSortKey {
  {key: blank_text_key(line, opts), raw: if opts.stable { "" } else { line }}
}

## GNU sort uses the full line as a last-resort key after the blank-skipping key.
pure blank_sorted(lines: List[Str], reverse: Bool, opts: SortOptions) -> List[Str] {
  if opts.stable {
    if reverse {
      lines |> sort-by(desc: true) blank_text_key(., opts)
    } else {
      lines |> sort-by blank_text_key(., opts)
    }
  } else {
  let fallback = if reverse { lines |> sort-by(desc: true) . } else { lines |> sort }
  if reverse {
    fallback |> sort-by(desc: true) blank_text_key(., opts)
  } else {
    fallback |> sort-by blank_text_key(., opts)
  }
  }
}

pure pair_is_ordered(left: Str, right: Str, opts: SortOptions, has_key: Bool, key_field: Int) -> Bool {
  let pair = [left, right]
  let ordered = if is_human_numeric_sort(opts) and has_key {
    if opts.reverse { pair |> sort-by(desc: true) human_numeric_field_sort_key(., opts.delimiter, key_field, opts, opts.stable or opts.unique) } else { pair |> sort-by human_numeric_field_sort_key(., opts.delimiter, key_field, opts, opts.stable or opts.unique) }
  } else if is_human_numeric_sort(opts) {
    if opts.reverse { pair |> sort-by(desc: true) human_numeric_sort_key(., opts.stable or opts.unique) } else { pair |> sort-by human_numeric_sort_key(., opts.stable or opts.unique) }
  } else if is_general_numeric_sort(opts) {
    if opts.reverse { pair |> sort-by(desc: true) general_numeric_sort_key(., opts.stable or opts.unique) } else { pair |> sort-by general_numeric_sort_key(., opts.stable or opts.unique) }
  } else if is_version_sort(opts) and has_key {
    if opts.reverse { pair |> sort-by(desc: true) version_field_sort_key(., opts.delimiter, key_field, opts) } else { pair |> sort-by version_field_sort_key(., opts.delimiter, key_field, opts) }
  } else if is_version_sort(opts) {
    if opts.reverse { pair |> sort-by(desc: true) version_sort_key(., opts.stable) } else { pair |> sort-by version_sort_key(., opts.stable) }
  } else if is_numeric_sort(opts) and has_key and key_reversed(opts.key) {
    if key_reversed(opts.key) != opts.reverse { pair |> sort-by(desc: true) field_sort_key(., opts.delimiter, key_field, opts) } else { pair |> sort-by field_sort_key(., opts.delimiter, key_field, opts) }
  } else if is_numeric_sort(opts) and has_key {
    if opts.reverse { pair |> sort-by(desc: true) numeric_field_sort_key(., opts.delimiter, key_field, opts) } else { pair |> sort-by numeric_field_sort_key(., opts.delimiter, key_field, opts) }
  } else if has_key {
    if opts.reverse { pair |> sort-by(desc: true) field_sort_key(., opts.delimiter, key_field, opts) } else { pair |> sort-by field_sort_key(., opts.delimiter, key_field, opts) }
  } else if is_numeric_sort(opts) {
    if opts.reverse { pair |> sort-by(desc: true) numeric_sort_key(., opts.stable or opts.unique) } else { pair |> sort-by numeric_sort_key(., opts.stable or opts.unique) }
  } else if opts.blank {
    blank_sorted(pair, opts.reverse, opts)
  } else if opts.fold_case or opts.dictionary or opts.ignore_nonprinting {
    if opts.reverse {
      pair |> sort-by(desc: true) { |line| {key: character_order_key(line, opts.dictionary, opts.ignore_nonprinting, opts.fold_case), raw: if opts.stable { "" } else { line }} }
    } else {
      pair |> sort-by { |line| {key: character_order_key(line, opts.dictionary, opts.ignore_nonprinting, opts.fold_case), raw: if opts.stable { "" } else { line }} }
    }
  } else if opts.reverse {
    pair |> sort-by(desc: true) .
  } else {
    pair |> sort
  }
  ordered[0] == left
}

pure same_sort_key(left: Str, right: Str, opts: SortOptions, has_key: Bool, key_field: Int) -> Bool {
  if is_human_numeric_sort(opts) and has_key {
    let left_key = human_numeric_field_sort_key(left, opts.delimiter, key_field, opts, true)
    let right_key = human_numeric_field_sort_key(right, opts.delimiter, key_field, opts, true)
    left_key.key == right_key.key
  } else if is_human_numeric_sort(opts) {
    let left_key = human_numeric_sort_key(left, true)
    let right_key = human_numeric_sort_key(right, true)
    left_key.key == right_key.key
  } else if is_general_numeric_sort(opts) {
    let left_key = general_numeric_sort_key(left, true)
    let right_key = general_numeric_sort_key(right, true)
    left_key.key == right_key.key
  } else if is_version_sort(opts) and has_key {
    version_key((if opts.delimiter == "" { left.trim().words() } else { left.split(opts.delimiter) }).get(key_field) ?? "") == version_key((if opts.delimiter == "" { right.trim().words() } else { right.split(opts.delimiter) }).get(key_field) ?? "")
  } else if is_version_sort(opts) {
    version_key(left) == version_key(right)
  } else if is_numeric_sort(opts) and has_key {
    if key_reversed(opts.key) {
      field_key(left, opts.delimiter, key_field, opts) == field_key(right, opts.delimiter, key_field, opts)
    } else {
      numeric_field_key(left, opts.delimiter, key_field, opts) == numeric_field_key(right, opts.delimiter, key_field, opts)
    }
  } else if has_key {
    field_key(left, opts.delimiter, key_field, opts) == field_key(right, opts.delimiter, key_field, opts)
  } else if is_numeric_sort(opts) {
    numeric_key(left) == numeric_key(right)
  } else if opts.blank {
    blank_text_key(left, opts) == blank_text_key(right, opts)
  } else if opts.fold_case or opts.dictionary or opts.ignore_nonprinting {
    character_order_key(left, opts.dictionary, opts.ignore_nonprinting, opts.fold_case) == character_order_key(right, opts.dictionary, opts.ignore_nonprinting, opts.fold_case)
  } else {
    left == right
  }
}

pure input_records(input: Str, zero_terminated: Bool) -> List[Str] {
  if zero_terminated {
    if input == "" { return [] }
    var records = input.split("\0")
    if ! records.is_empty() and records[-1] == "" {
      records = records[..records.len() - 1]
    }
    records
  } else {
    input.lines().collect()
  }
}

pure debug_primary_text(line: Str, opts: SortOptions, has_key: Bool, key_field: Int) -> Str {
  if has_key {
    let parts = if opts.delimiter == "" { line.trim().words() } else { line.split(opts.delimiter) }
    if is_human_numeric_sort(opts) {
      let text = parts.get(key_field) ?? ""
      let selected = text.split("") |> drop(key_character_offset(opts.key)).join("")
      human_numeric_prefix(selected)
    } else if is_numeric_sort(opts) {
      let text = parts.get(key_field) ?? ""
      text.split("") |> drop(key_character_offset(opts.key)).join("")
    } else {
      parts.get(key_field) ?? ""
    }
  } else if is_numeric_sort(opts) {
    numeric_prefix(line)
  } else if is_human_numeric_sort(opts) {
    human_numeric_prefix(line)
  } else if is_general_numeric_sort(opts) {
    general_numeric_prefix(line)
  } else if opts.blank {
    trim_leading_blanks(line)
  } else {
    line
  }
}

pure debug_visible_line(line: Str) -> Str {
  let input = bytes.from_text(line)
  var at = 0
  var visible = ""
  while at < line.byte_len() and line.byte_slice(at, length: 1) in [" ", "\t"] {
    visible += if input.byte_at(at) == 9 { ">" } else { " " }
    at += 1
  }
  visible + line.byte_slice(at)
}

pure leading_blank_count(line: Str) -> Int {
  var at = 0
  while at < line.byte_len() and line.byte_slice(at, length: 1) in [" ", "\t"] { at += 1 }
  at
}

pure debug_annotation(value: Str) -> Str {
  if value == "" { "^ no match for key" } else { text.padding(value.count_chars(), "_") }
}

pure debug_sort_text(lines: List[Str], opts: SortOptions, has_key: Bool, key_field: Int) -> Str {
  let has_last_resort = has_key or is_numeric_sort(opts) or is_human_numeric_sort(opts) or is_general_numeric_sort(opts) or is_version_sort(opts) or opts.blank or opts.fold_case or opts.dictionary or opts.ignore_nonprinting
  let annotation_count = if has_last_resort and ! opts.stable and ! opts.unique { 2 } else { 1 }
  var output = ""
  for line in lines {
    output += debug_visible_line(line) + "\n"
    let primary = debug_primary_text(line, opts, has_key, key_field)
    let indentation = if is_general_numeric_sort(opts) or is_numeric_sort(opts) or is_human_numeric_sort(opts) { text.padding(leading_blank_count(line), " ") } else { "" }
    output += indentation + debug_annotation(primary) + "\n"
    if annotation_count > 1 {
      output += debug_annotation(line) + "\n"
    }
  }
  output
}

## A required output operand may start with a dash, so attach it before parsing options.
pure normalize_output_args(argv: List[Str]) -> List[Str] {
  var normalized: List[Str] = []
  var operands_only = false
  var at = 0
  while at < argv.len() {
    let arg = argv[at]
    if ! operands_only and arg == "--" {
      normalized += [arg]
      operands_only = true
      at += 1
    } else if ! operands_only and arg in ["-o", "--output"] and at + 1 < argv.len() and argv[at + 1].starts_with("--") {
      let value = argv[at + 1]
      normalized += [if arg == "-o" { f"-o{value}" } else { f"--output={value}" }]
      at += 2
    } else {
      normalized += [arg]
      at += 1
    }
  }
  normalized
}

## A final unterminated record in one operand ends before the next operand starts.
proc read_sort_input(paths: List[Str], separator: Str) [fs, io, error] -> Result[Str, Error] {
  var sources = paths
  if sources.is_empty() { sources = ["-"] }

  var input = ""
  for source in sources {
    let part = if source == "-" { io.stdin_text()? } else { fp"{source}".read_text()? }
    if input != "" and ! input.ends_with(separator) {
      input += separator
    }
    input += part
  }
  input
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: SortOptions = cli.applet(
    normalize_output_args(argv),
    {
      reverse: {
        form: "-r --reverse",
        default: false,
      },
      unique: {
        form: "-u --unique",
        default: false,
      },
      numeric: {
        form: "-n --numeric-sort",
        default: false,
      },
      human_numeric: {
        form: "-h --human-numeric-sort",
        default: false,
      },
      general_numeric: {
        form: "-g --general-numeric-sort",
        default: false,
      },
      version_sort: {
        form: "-V --version-sort",
        default: false,
      },
      sort_mode: {
        form: "--sort=MODE",
        repeated: true,
      },
      debug: {
        form: "--debug",
        default: false,
      },
      fold_case: {
        form: "-f --ignore-case",
        default: false,
      },
      dictionary: {
        form: "-d --dictionary-order",
        default: false,
      },
      ignore_nonprinting: {
        form: "-i --ignore-nonprinting",
        default: false,
      },
      blank: {
        form: "-b --ignore-leading-blanks",
        default: false,
      },
      stable: {
        form: "-s --stable",
        default: false,
      },
      key: {
        form: "-k KEY",
        default: "",
      },
      delimiter: {
        form: "-t DELIMITER",
        default: "",
      },
      output: {
        form: "-o FILE",
        repeated: true,
      },
      check: {
        form: "--check[=TYPE]",
        default: "",
        optional_default: "diagnose-first",
      },
      short_check: {
        form: "-c",
        default: false,
      },
      silent_check: {
        form: "-C",
        default: false,
      },
      zero_terminated: {
        form: "-z --zero-terminated",
        default: false,
      },
      version: {
        form: "--version",
        default: false,
        stop: true,
      },
      paths: {
        form: "...FILE",
      },
    },
  )?

  if opts.version {
    gnu.version("sort")
    return
  }

  let selected_mode = selected_sort_mode(opts)
  let general_numeric = is_general_numeric_sort(opts)
  let human_numeric = is_human_numeric_sort(opts)
  if selected_mode != "" and ! selected_mode.starts_with("v") and ! is_human_numeric_mode(selected_mode) and selected_mode not in ["g", "general-numeric", "n", "numeric"] {
    gnu.error(f"invalid argument {gnu.quote_maybe(selected_mode)} for '--sort'")
    exit 2
  }

  let numeric_requested = opts.numeric or is_numeric_sort(opts)
  let general_numeric_requested = opts.general_numeric or general_numeric
  let human_numeric_requested = opts.human_numeric or human_numeric
  if numeric_requested and general_numeric_requested {
    gnu.error("options '-gn' are incompatible")
    exit 2
  }
  if human_numeric_requested and general_numeric_requested {
    gnu.error("options '-gh' are incompatible")
    exit 2
  }
  if human_numeric_requested and numeric_requested {
    gnu.error("options '-hn' are incompatible")
    exit 2
  }

  if (is_numeric_sort(opts) or human_numeric) and (opts.dictionary or opts.ignore_nonprinting) {
    let conflict = if opts.dictionary { if human_numeric { "-dh" } else { "-dn" } } else { if human_numeric { "-hi" } else { "-in" } }
    gnu.error(f"options '{conflict}' are incompatible")
    exit 2
  }

  let has_key = opts.key != ""
  let key_field = if has_key { key_index(opts.key) } else { 0 }
  let output_path = opts.output.get(0) ?? ""
  let has_output = ! opts.output.is_empty()
  for candidate in opts.output[1..] {
    if candidate != output_path {
      gnu.error("multiple output files specified")
      exit 2
    }
  }
  let output = if has_output { fp"{output_path}" } else { p"" }
  let check_mode = if opts.check != "" { opts.check } else if opts.short_check { "diagnose-first" } else { "" }
  let check_enabled = check_mode != "" or opts.silent_check
  let silent_check = opts.silent_check or check_mode in ["silent", "quiet", "silen", "quie", "s", "q"]
  let {delimiter, paths, ..} = opts

  let check_is_silent = check_mode in ["silent", "quiet", "silen", "quie", "s", "q"]
  if opts.silent_check and (opts.short_check or (opts.check != "" and ! check_is_silent)) {
    gnu.error("options '-cC' are incompatible")
    exit 2
  }
  if check_enabled and check_mode != "" and check_mode not in ["diagnose-first", "diagnose", "d", "silent", "quiet", "silen", "quie", "s", "q"] {
    gnu.error(f"invalid argument {gnu.quote_maybe(check_mode)} for '--check'")
    exit 2
  }
  if check_enabled and has_output {
    gnu.error(if silent_check { "options '-Co' are incompatible" } else { "options '-co' are incompatible" })
    exit 2
  }

  for input_path in paths {
    if input_path != "-" {
      if let Err(failure) = fs.stat(fp"{input_path}", follow_symlinks: true) {
        gnu.error(f"cannot read: {gnu.quote_maybe(input_path)}: {gnu.strerror(failure)}")
        exit 2
      }
    }
  }

  let input_separator = if opts.zero_terminated { "\0" } else { "\n" }
  let input = read_sort_input(paths, input_separator)?
  let input_lines = input_records(input, opts.zero_terminated)
  if check_enabled {
    if input_lines.len() > 1 {
      for index in range(1, input_lines.len()) {
        let previous = input_lines[index - 1]
        let current = input_lines[index]
        let unordered = ! pair_is_ordered(previous, current, opts, has_key, key_field)
        let duplicate = opts.unique and same_sort_key(previous, current, opts, has_key, key_field)

        if unordered or duplicate {
          if ! silent_check {
            let name = paths.get(0) ?? "-"
            if opts.zero_terminated {
              io.write_stderr(f"{gnu.prog()}: {gnu.quote_maybe(name)}:{index + 1}: disorder: {current}\0")?
            } else {
              gnu.error(f"{gnu.quote_maybe(name)}:{index + 1}: disorder: {current}")
            }
          }
          exit 1
        }
      }
    }
    return
  }

  let sorted = if human_numeric and has_key {
    if opts.reverse {
      input_lines |> sort-by(desc: true) human_numeric_field_sort_key(., delimiter, key_field, opts, opts.stable or opts.unique)
    } else {
      input_lines |> sort-by human_numeric_field_sort_key(., delimiter, key_field, opts, opts.stable or opts.unique)
    }
  } else if human_numeric {
    if opts.reverse {
      input_lines |> sort-by(desc: true) human_numeric_sort_key(., opts.stable or opts.unique)
    } else {
      input_lines |> sort-by human_numeric_sort_key(., opts.stable or opts.unique)
    }
  } else if general_numeric {
    if opts.reverse {
      input_lines |> sort-by(desc: true) general_numeric_sort_key(., opts.stable or opts.unique)
    } else {
      input_lines |> sort-by general_numeric_sort_key(., opts.stable or opts.unique)
    }
  } else if is_version_sort(opts) and has_key {
    if opts.reverse {
      input_lines |> sort-by(desc: true) version_field_sort_key(., delimiter, key_field, opts)
    } else {
      input_lines |> sort-by version_field_sort_key(., delimiter, key_field, opts)
    }
  } else if is_version_sort(opts) {
    if opts.reverse {
      input_lines |> sort-by(desc: true) version_sort_key(., opts.stable)
    } else {
      input_lines |> sort-by version_sort_key(., opts.stable)
    }
  } else if is_numeric_sort(opts) and has_key and key_reversed(opts.key) {
    if key_reversed(opts.key) != opts.reverse { input_lines |> sort-by(desc: true) field_sort_key(., delimiter, key_field, opts) } else { input_lines |> sort-by field_sort_key(., delimiter, key_field, opts) }
  } else if is_numeric_sort(opts) and has_key {
    if opts.reverse {
      input_lines |> sort-by(desc: true) numeric_field_sort_key(., delimiter, key_field, opts)
    } else {
      input_lines |> sort-by numeric_field_sort_key(., delimiter, key_field, opts)
    }
  } else if has_key {
    if opts.reverse {
      input_lines |> sort-by(desc: true) field_sort_key(., delimiter, key_field, opts)
    } else {
      input_lines |> sort-by field_sort_key(., delimiter, key_field, opts)
    }
  } else if is_numeric_sort(opts) {
    ## Keep the first spelling for each number before a reverse sort can reorder equal keys.
    let numeric_lines = if opts.unique { input_lines |> unique-by numeric_sort_key(., true) } else { input_lines }
    if opts.reverse {
      numeric_lines |> sort-by(desc: true) numeric_sort_key(., opts.stable or opts.unique)
    } else {
      numeric_lines |> sort-by numeric_sort_key(., opts.stable or opts.unique)
    }
  } else if opts.blank {
    blank_sorted(input_lines, opts.reverse, opts)
  } else if opts.fold_case or opts.dictionary or opts.ignore_nonprinting {
    if opts.reverse {
      input_lines |> sort-by(desc: true) { |line| {key: character_order_key(line, opts.dictionary, opts.ignore_nonprinting, opts.fold_case), raw: if opts.stable { "" } else { line }} }
    } else {
      input_lines |> sort-by { |line| {key: character_order_key(line, opts.dictionary, opts.ignore_nonprinting, opts.fold_case), raw: if opts.stable { "" } else { line }} }
    }
  } else if opts.reverse {
    input_lines |> sort-by(desc: true) .
  } else {
    input_lines |> sort
  }

  let lines = if opts.unique and human_numeric and has_key {
    sorted |> unique-by human_numeric_field_sort_key(., delimiter, key_field, opts, true)
  } else if opts.unique and human_numeric {
    sorted |> unique-by human_numeric_sort_key(., true)
  } else if opts.unique and general_numeric {
    sorted |> unique-by general_numeric_sort_key(., true)
  } else if opts.unique and is_numeric_sort(opts) {
    sorted |> unique-by numeric_sort_key(., true)
  } else if opts.unique {
    sorted |> unique-by .
  } else {
    sorted
  }

  let line_ending = if opts.zero_terminated { "\0" } else { "\n" }
  let text = if opts.debug {
    debug_sort_text(lines, opts, has_key, key_field)
  } else if lines.is_empty() {
    ""
  } else {
    f"{lines.join(line_ending)}{line_ending}"
  }

  if has_output {
    if let Err(failure) = output.write(text) {
      let action = if let Ok(_) = fs.stat(output, follow_symlinks: true) { "write failed" } else { "open failed" }
      gnu.error(f"{action}: {gnu.quote_maybe(output_path)}: {gnu.strerror(failure)}")
      exit 2
    }
  } else if opts.zero_terminated or opts.debug {
    gnu.write_text(text)
  } else {
    for line in lines {
      print $line
    }
  }
}
