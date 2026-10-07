#!/bin/xsh
use lib.gnu
use lib.text_a2 as text

type SortOptions = {
  reverse: Bool,
  unique: Bool,
  numeric: Bool,
  human_numeric: Bool,
  general_numeric: Bool,
  month: Bool,
  fold_case: Bool,
  dictionary: Bool,
  ignore_nonprinting: Bool,
  blank: Bool,
  stable: Bool,
  merge: Bool,
  version_sort: Bool,
  sort_mode: List[Str],
  buffer_size: Str?,
  batch_size: Str?,
  temp_directory: Str?,
  debug: Bool,
  key: List[Str],
  delimiter: Str,
  output: List[Str],
  check: Str,
  short_check: Bool,
  silent_check: Bool,
  zero_terminated: Bool,
  files0_from: Str?,
  version: Bool,
  paths: List[Str],
}

type NumericSortKey = {number: Str, raw: Str}
type HumanNumericSortKey = {key: Str, raw: Str}
type GeneralNumericSortKey = {key: Str, raw: Str}
type TextSortKey = {key: Str, raw: Str}
type SortInput = {name: Bytes, path: Path, stdin: Bool}
type SortMergeReader = {fd: Int, name: Bytes, pending: Bytes, eof: Bool, current: Str?}

## External sorts unwind their temp-root defer before returning shell status 130.
on SIGINT [] {
  exit 130
}

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

## In the C locale, GNU sort -M orders English abbreviations after leading blanks; unknown prefixes come first.
pure month_order(line: Str) -> Int {
  let value = trim_leading_blanks(line).upper()
  if value.byte_len() < 3 { return 0 }
  let abbreviation = value.byte_slice(0, length: 3)
  if abbreviation == "JAN" { return 1 }
  if abbreviation == "FEB" { return 2 }
  if abbreviation == "MAR" { return 3 }
  if abbreviation == "APR" { return 4 }
  if abbreviation == "MAY" { return 5 }
  if abbreviation == "JUN" { return 6 }
  if abbreviation == "JUL" { return 7 }
  if abbreviation == "AUG" { return 8 }
  if abbreviation == "SEP" { return 9 }
  if abbreviation == "OCT" { return 10 }
  if abbreviation == "NOV" { return 11 }
  if abbreviation == "DEC" { return 12 }
  0
}

pure month_prefix(line: Str) -> Str {
  let value = trim_leading_blanks(line)
  if month_order(line) == 0 { return "" }
  value.byte_slice(0, length: 3)
}

pure month_sort_key(line: Str, stable: Bool) -> TextSortKey {
  {key: padded_decimal(month_order(line), 2), raw: if stable { "" } else { line }}
}

pure month_field_sort_key(line: Str, delimiter: Str, field: Int, opts: SortOptions, stable: Bool) -> TextSortKey {
  let parts = if delimiter == "" { line.trim().words() } else { line.split(delimiter) }
  let text = parts.get(field) ?? ""
  let selected = text.split("") |> drop(key_character_offset(primary_key_spec(opts))).join("")
  month_sort_key(selected, stable)
}

pure human_numeric_field_sort_key(line: Str, delimiter: Str, field: Int, opts: SortOptions, stable: Bool) -> HumanNumericSortKey {
  let parts = if delimiter == "" { line.trim().words() } else { line.split(delimiter) }
  let text = parts.get(field) ?? ""
  let selected = text.split("") |> drop(key_character_offset(primary_key_spec(opts))).join("")
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
## GNU places failed numeric conversions before NaN and all valid numbers.
pure general_numeric_sort_key(line: Str, stable: Bool) -> GeneralNumericSortKey {
  if general_numeric_prefix(line) == "" {
    return {key: "0", raw: if stable { "" } else { line }}
  }
  let number = general_numeric_value(line)
  let special = number.format()
  if special == "NaN" { return {key: "1", raw: if stable { "" } else { line }} }
  if special == "-Infinity" { return {key: "2", raw: if stable { "" } else { line }} }
  if special == "Infinity" { return {key: "6", raw: if stable { "" } else { line }} }
  let display = if let Ok(exact) = number.format_number("g", 17) { exact } else { special }
  if display in ["0", "-0"] { return {key: "4", raw: if stable { "" } else { line }} }

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
  {key: f"{if negative { "3" } else { "5" }}{padded_decimal(ordered_exponent, 3)}{significance}", raw: if stable { "" } else { line }}
}

pure numeric_field_sort_key(line: Str, delimiter: Str, field: Int, opts: SortOptions) -> NumericSortKey {
  {number: numeric_field_key(line, delimiter, field, opts), raw: if opts.stable or opts.unique { "" } else { line }}
}

pure key_index(spec: Str) -> Int {
  (((spec.split(",").get(0) ?? "1").split(".").get(0) ?? "1").parse_int() ?? 1) - 1
}

pure primary_key_spec(opts: SortOptions) -> Str {
  opts.key.get(0) ?? ""
}

pure sort_delimiter(opts: SortOptions) -> Str {
  if opts.delimiter == "\\0" { "\0" } else { opts.delimiter }
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

pure key_end_field_index(spec: Str) -> Int? {
  if spec.split(",").len() < 2 { return null }
  key_index(spec.split(",").get(1) ?? "")
}

pure key_end_character_count(spec: Str) -> Int {
  let end = (spec.split(",").get(1) ?? "").split(".").get(1) ?? ""
  let input = bytes.from_text(end)
  var digits = 0
  while digits < input.len() and is_ascii_digit(input.byte_at(digits) ?? 0) { digits += 1 }
  if digits == 0 { return 0 }
  let position = end.byte_slice(0, length: digits).parse_int() ?? 0
  if position > 0 { position } else { 0 }
}

pure key_reversed(spec: Str) -> Bool {
  "r" in key_flags(spec)
}

pure key_flags(spec: Str) -> Str {
  var flags = ""
  for segment in spec.split(",") {
    let input = bytes.from_text(segment)
    var at = 0
    while at < input.len() and is_ascii_digit(input.byte_at(at) ?? 0) { at += 1 }
    if at < input.len() and input.byte_at(at) == 46 {
      at += 1
      while at < input.len() and is_ascii_digit(input.byte_at(at) ?? 0) { at += 1 }
    }
    flags += segment.byte_slice(at)
  }
  flags
}

pure key_options_are_explicit(spec: Str) -> Bool {
  key_flags(spec) != ""
}

pure key_effective_options(spec: Str, opts: SortOptions) -> SortOptions {
  if ! key_options_are_explicit(spec) { return opts }
  let flags = key_flags(spec)
  {
    ...opts,
    numeric: "n" in flags,
    human_numeric: "h" in flags,
    month: "M" in flags,
    general_numeric: "g" in flags,
    version_sort: "V" in flags,
    sort_mode: [],
    fold_case: "f" in flags,
    dictionary: "d" in flags,
    ignore_nonprinting: "i" in flags,
    blank: "b" in flags,
  }
}

## Encode bytes with a low end marker so descending keys keep equal-key order.
pure sort_direction_bytes(input: Bytes, reverse: Bool) -> Str {
  var key = ""
  for index in range(input.len()) {
    let byte = input.byte_at(index) ?? 0
    let ordered = if reverse { 255 - byte } else { byte + 1 }
    key += padded_decimal(ordered, 3)
  }
  key + (if reverse { "256" } else { "000" })
}

pure sort_direction_key(value: Str, reverse: Bool) -> Str {
  sort_direction_bytes(bytes.from_text(value), reverse)
}

pure character_order_sort_key(input: Bytes, dictionary: Bool, ignore_nonprinting: Bool, fold_case: Bool, reverse: Bool) -> Str {
  var key = ""
  for index in range(input.len()) {
    let byte = input.byte_at(index) ?? 0
    let blank = byte == 9 or byte == 32
    let alphanumeric = (byte >= 48 and byte <= 57) or (byte >= 65 and byte <= 90) or (byte >= 97 and byte <= 122)
    let in_dictionary = ! dictionary or blank or alphanumeric
    let printable = byte >= 32 and byte <= 126
    let in_printable = ! ignore_nonprinting or printable
    if in_dictionary and in_printable {
      let folded = if fold_case and byte >= 97 and byte <= 122 { byte - 32 } else { byte }
      let ordered = if reverse { 255 - folded } else { folded + 1 }
      key += padded_decimal(ordered, 3)
    }
  }
  key + (if reverse { "256" } else { "000" })
}

pure key_field_value(line: Str, delimiter: Str, spec: Str, opts: SortOptions) -> Str {
  let parts = if delimiter == "" { line.trim().words() } else { line.split(delimiter) }
  let start_field = key_index(spec)
  if start_field >= parts.len() { return "" }
  var start_prefix = ""
  if delimiter == "" and start_field > 0 and ! opts.blank and ! ("b" in spec) {
    let start = key_field_start_byte(line, delimiter, start_field)
    let token = key_field_token_start_byte(line, delimiter, start_field)
    start_prefix = line.byte_slice(start, length: token - start)
  }
  var start_value = start_prefix + parts[start_field]
  if opts.blank or "b" in spec { start_value = trim_leading_blanks(start_value) }
  var selected = start_value.split("") |> drop(key_character_offset(spec)).join("")
  if let end_field = key_end_field_index(spec) {
    if end_field < start_field or end_field >= parts.len() { return "" }
    let end_value = parts[end_field]
    let end_count = key_end_character_count(spec)
    if end_field == start_field {
      if end_count > 0 {
        let length = if end_count > key_character_offset(spec) { end_count - key_character_offset(spec) } else { 0 }
        return selected.split("") |> take(length).join("")
      }
      return selected
    }
    let joiner = if delimiter == "" { " " } else { delimiter }
    var middle = ""
    for field in range(start_field + 1, end_field) {
      middle += f"{joiner}{parts[field]}"
    }
    let end = if end_count > 0 { end_value.split("") |> take(end_count).join("") } else { end_value }
    selected + middle + joiner + end
  } else {
    let joiner = if delimiter == "" { " " } else { delimiter }
    for field in range(start_field + 1, parts.len()) {
      selected += f"{joiner}{parts[field]}"
    }
    selected
  }
}

pure key_range_end_byte(line: Str, delimiter: Str, spec: Str) -> Int? {
  let start_field = key_index(spec)
  if let end_field = key_end_field_index(spec) {
    if end_field < start_field { return null }
    if let field_end = key_field_end_byte(line, delimiter, end_field) {
      let end_count = key_end_character_count(spec)
      if end_count > 0 {
        let end = key_field_start_byte(line, delimiter, end_field) + end_count
        return if end < field_end { end } else { field_end }
      }
      return field_end
    }
    return null
  }
  line.byte_len()
}

pure key_range_bytes(line: Str, delimiter: Str, spec: Str, opts: SortOptions) -> Bytes {
  let input = bytes.from_text(line)
  let start = key_start_byte(line, delimiter, spec, opts)
  if let end = key_range_end_byte(line, delimiter, spec) {
    if end > start { input[start..end] } else { b"" }
  } else {
    b""
  }
}

pure key_field_start_byte(line: Str, delimiter: Str, field: Int) -> Int {
  if delimiter != "" {
    let parts = line.split(delimiter)
    if field >= parts.len() { return line.byte_len() }
    var position = 0
    for index in range(field) { position += parts[index].byte_len() + delimiter.byte_len() }
    return position
  }

  let input = bytes.from_text(line)
  var position = 0
  var current_field = 0
  while position < input.len() and current_field < field {
    while position < input.len() and (input.byte_at(position) ?? 0) not in [32, 9] { position += 1 }
    current_field += 1
    if current_field < field {
      while position < input.len() and (input.byte_at(position) ?? 0) in [32, 9] { position += 1 }
    }
  }
  position
}

pure key_field_token_start_byte(line: Str, delimiter: Str, field: Int) -> Int {
  var position = key_field_start_byte(line, delimiter, field)
  if delimiter != "" { return position }
  let input = bytes.from_text(line)
  while position < input.len() and (input.byte_at(position) ?? 0) in [32, 9] { position += 1 }
  position
}

pure key_field_end_byte(line: Str, delimiter: Str, field: Int) -> Int? {
  if delimiter != "" {
    let parts = line.split(delimiter)
    if field >= parts.len() { return null }
    return key_field_start_byte(line, delimiter, field) + parts[field].byte_len()
  }

  let input = bytes.from_text(line)
  if field >= line.trim().words().len() { return null }
  var position = key_field_start_byte(line, delimiter, field)
  while position < input.len() and (input.byte_at(position) ?? 0) in [32, 9] { position += 1 }
  while position < input.len() and (input.byte_at(position) ?? 0) not in [32, 9] { position += 1 }
  position
}

pure key_start_byte(line: Str, delimiter: Str, spec: Str, opts: SortOptions) -> Int {
  if key_index(spec) >= (if delimiter == "" { line.trim().words().len() } else { line.split(delimiter).len() }) { return line.byte_len() }
  let input = bytes.from_text(line)
  var position = key_field_start_byte(line, delimiter, key_index(spec))
  if opts.blank or "b" in spec or is_numeric_sort(opts) or is_human_numeric_sort(opts) or
    is_general_numeric_sort(opts) or is_month_sort(opts) {
    while position < input.len() and (input.byte_at(position) ?? 0) in [32, 9] { position += 1 }
  }
  position += key_character_offset(spec)
  if position > input.len() { input.len() } else { position }
}

pure key_debug_width(line: Str, delimiter: Str, spec: Str, opts: SortOptions) -> Int {
  let start = key_start_byte(line, delimiter, spec, opts)
  key_range_bytes(line, delimiter, spec, opts).len()
}

pure key_order_value(line: Str, delimiter: Str, spec: Str, opts: SortOptions) -> Str {
  let key_opts = key_effective_options(spec, opts)
  let reverse = opts.reverse != key_reversed(spec)
  if ! is_month_sort(key_opts) and ! is_human_numeric_sort(key_opts) and ! is_general_numeric_sort(key_opts) and
    ! is_version_sort(key_opts) and ! is_numeric_sort(key_opts) {
    return character_order_sort_key(
      key_range_bytes(line, delimiter, spec, key_opts),
      key_opts.dictionary,
      key_opts.ignore_nonprinting,
      key_opts.fold_case,
      reverse,
    )
  }
  let value = key_field_value(line, delimiter, spec, key_opts)
  let key = if is_month_sort(key_opts) {
    padded_decimal(month_order(value), 2)
  } else if is_human_numeric_sort(key_opts) {
    human_numeric_sort_key(value, true).key
  } else if is_general_numeric_sort(key_opts) {
    general_numeric_sort_key(value, true).key
  } else if is_version_sort(key_opts) {
    version_key(value)
  } else if is_numeric_sort(key_opts) {
    numeric_key(value)
  } else if key_opts.blank {
    character_order_key(trim_leading_blanks(value), key_opts.dictionary, key_opts.ignore_nonprinting, key_opts.fold_case)
  } else {
    character_order_key(value, key_opts.dictionary, key_opts.ignore_nonprinting, key_opts.fold_case)
  }
  sort_direction_key(key, reverse)
}

## GNU compares keys in order, then uses the raw line unless stable or unique was requested.
pure multiple_key_sort(lines: List[Str], opts: SortOptions) -> List[Str] {
  var sorted = lines
  if ! opts.stable and ! opts.unique {
    sorted = if opts.reverse {
      lines |> sort-by { |line| sort_direction_key(line, true) }
    } else {
      lines |> sort
    }
  }
  for position in range(opts.key.len()) {
    let spec = opts.key[opts.key.len() - position - 1]
    sorted = sorted |> sort-by { |line| key_order_value(line, sort_delimiter(opts), spec, opts) }
  }
  sorted
}

pure same_multiple_keys(left: Str, right: Str, opts: SortOptions) -> Bool {
  for spec in opts.key {
    if key_order_value(left, sort_delimiter(opts), spec, opts) != key_order_value(right, sort_delimiter(opts), spec, opts) {
      return false
    }
  }
  true
}

pure unique_key_lines(lines: List[Str], opts: SortOptions) -> List[Str] {
  var unique: List[Str] = []
  for line in lines {
    if unique.is_empty() or ! same_multiple_keys(unique[unique.len() - 1], line, opts) {
      unique += [line]
    }
  }
  unique
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

pure is_month_mode(mode: Str) -> Bool {
  mode != "" and "month".starts_with(mode)
}

pure is_month_sort(opts: SortOptions) -> Bool {
  let mode = selected_sort_mode(opts)
  if mode == "" { opts.month } else { is_month_mode(mode) }
}

pure is_general_numeric_sort(opts: SortOptions) -> Bool {
  let mode = selected_sort_mode(opts)
  if mode == "" { opts.general_numeric } else { is_general_numeric_mode(mode) }
}

pure is_general_numeric_mode(mode: Str) -> Bool {
  mode != "" and "general-numeric".starts_with(mode)
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
  let key = text.split("") |> drop(key_character_offset(primary_key_spec(opts))).join("")
  numeric_key(key)
}

pure trim_leading_blanks(line: Str) -> Str {
  let characters = line.split("")
  var at = 0
  while at < characters.len() and characters[at] in [" ", "\t"] { at += 1 }
  characters |> drop(at) |> join("")
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
  if has_key {
    return multiple_key_sort([left, right], opts)[0] == left
  }

  let pair = [left, right]
  let ordered = if is_month_sort(opts) and has_key {
    if opts.reverse { pair |> sort-by(desc: true) month_field_sort_key(., sort_delimiter(opts), key_field, opts, opts.stable or opts.unique) } else { pair |> sort-by month_field_sort_key(., sort_delimiter(opts), key_field, opts, opts.stable or opts.unique) }
  } else if is_month_sort(opts) {
    if opts.reverse { pair |> sort-by(desc: true) month_sort_key(., opts.stable or opts.unique) } else { pair |> sort-by month_sort_key(., opts.stable or opts.unique) }
  } else if is_human_numeric_sort(opts) and has_key {
    if opts.reverse { pair |> sort-by(desc: true) human_numeric_field_sort_key(., sort_delimiter(opts), key_field, opts, opts.stable or opts.unique) } else { pair |> sort-by human_numeric_field_sort_key(., sort_delimiter(opts), key_field, opts, opts.stable or opts.unique) }
  } else if is_human_numeric_sort(opts) {
    if opts.reverse { pair |> sort-by(desc: true) human_numeric_sort_key(., opts.stable or opts.unique) } else { pair |> sort-by human_numeric_sort_key(., opts.stable or opts.unique) }
  } else if is_general_numeric_sort(opts) {
    if opts.reverse { pair |> sort-by(desc: true) general_numeric_sort_key(., opts.stable or opts.unique) } else { pair |> sort-by general_numeric_sort_key(., opts.stable or opts.unique) }
  } else if is_version_sort(opts) and has_key {
    if opts.reverse { pair |> sort-by(desc: true) version_field_sort_key(., sort_delimiter(opts), key_field, opts) } else { pair |> sort-by version_field_sort_key(., sort_delimiter(opts), key_field, opts) }
  } else if is_version_sort(opts) {
    if opts.reverse { pair |> sort-by(desc: true) version_sort_key(., opts.stable) } else { pair |> sort-by version_sort_key(., opts.stable) }
  } else if is_numeric_sort(opts) and has_key and key_reversed(primary_key_spec(opts)) {
    if key_reversed(primary_key_spec(opts)) != opts.reverse { pair |> sort-by(desc: true) field_sort_key(., sort_delimiter(opts), key_field, opts) } else { pair |> sort-by field_sort_key(., sort_delimiter(opts), key_field, opts) }
  } else if is_numeric_sort(opts) and has_key {
    if opts.reverse { pair |> sort-by(desc: true) numeric_field_sort_key(., sort_delimiter(opts), key_field, opts) } else { pair |> sort-by numeric_field_sort_key(., sort_delimiter(opts), key_field, opts) }
  } else if has_key {
    if opts.reverse { pair |> sort-by(desc: true) field_sort_key(., sort_delimiter(opts), key_field, opts) } else { pair |> sort-by field_sort_key(., sort_delimiter(opts), key_field, opts) }
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
  if has_key {
    same_multiple_keys(left, right, opts)
  } else if is_month_sort(opts) and has_key {
    month_field_sort_key(left, sort_delimiter(opts), key_field, opts, true).key == month_field_sort_key(right, sort_delimiter(opts), key_field, opts, true).key
  } else if is_month_sort(opts) {
    month_order(left) == month_order(right)
  } else if is_human_numeric_sort(opts) and has_key {
    let left_key = human_numeric_field_sort_key(left, sort_delimiter(opts), key_field, opts, true)
    let right_key = human_numeric_field_sort_key(right, sort_delimiter(opts), key_field, opts, true)
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
    version_key((if sort_delimiter(opts) == "" { left.trim().words() } else { left.split(sort_delimiter(opts)) }).get(key_field) ?? "") == version_key((if sort_delimiter(opts) == "" { right.trim().words() } else { right.split(sort_delimiter(opts)) }).get(key_field) ?? "")
  } else if is_version_sort(opts) {
    version_key(left) == version_key(right)
  } else if is_numeric_sort(opts) and has_key {
    if key_reversed(primary_key_spec(opts)) {
      field_key(left, sort_delimiter(opts), key_field, opts) == field_key(right, sort_delimiter(opts), key_field, opts)
    } else {
      numeric_field_key(left, sort_delimiter(opts), key_field, opts) == numeric_field_key(right, sort_delimiter(opts), key_field, opts)
    }
  } else if has_key {
    field_key(left, sort_delimiter(opts), key_field, opts) == field_key(right, sort_delimiter(opts), key_field, opts)
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

## Split before decoding so POSIX filenames with invalid UTF-8 remain usable paths.
pure files0_names(data: Bytes) -> List[Bytes] {
  var names: List[Bytes] = []
  var start = 0
  for at in range(data.len()) {
    if data.byte_at(at) == 0 {
      names += [data[start..at]]
      start = at + 1
    }
  }
  if start < data.len() { names += [data[start..]] }
  names
}

pure debug_primary_text(line: Str, opts: SortOptions, has_key: Bool, key_field: Int) -> Str {
  if has_key {
    let selected = key_field_value(line, sort_delimiter(opts), primary_key_spec(opts), opts)
    if is_month_sort(opts) {
      month_prefix(selected)
    } else if is_human_numeric_sort(opts) {
      human_numeric_prefix(selected)
    } else if is_numeric_sort(opts) {
      numeric_prefix(selected)
    } else {
      selected
    }
  } else if is_month_sort(opts) {
    month_prefix(line)
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

pure debug_key_text(line: Str, opts: SortOptions, spec: Str) -> Str {
  let key_opts = key_effective_options(spec, opts)
  let selected = key_field_value(line, sort_delimiter(opts), spec, key_opts)
  if is_month_sort(key_opts) {
    month_prefix(selected)
  } else if is_human_numeric_sort(key_opts) {
    human_numeric_prefix(selected)
  } else if is_numeric_sort(key_opts) {
    numeric_prefix(selected)
  } else {
    selected
  }
}

pure debug_visible_line(line: Str) -> Str {
  let input = bytes.from_text(line)
  var at = 0
  var visible = ""
  while at < input.len() and (input.byte_at(at) == 32 or input.byte_at(at) == 9) {
    visible += if input.byte_at(at) == 9 { ">" } else { " " }
    at += 1
  }
  visible + line.byte_slice(at)
}

pure leading_blank_count(line: Str) -> Int {
  let input = bytes.from_text(line)
  var at = 0
  while at < input.len() and (input.byte_at(at) == 32 or input.byte_at(at) == 9) { at += 1 }
  at
}

pure debug_annotation(value: Str) -> Str {
  if value == "" { "^ no match for key" } else { text.padding(value.byte_len(), "_") }
}

pure debug_key_annotation(line: Str, opts: SortOptions, spec: Str) -> Str {
  let key_opts = key_effective_options(spec, opts)
  if is_month_sort(key_opts) or is_human_numeric_sort(key_opts) or is_numeric_sort(key_opts) or is_general_numeric_sort(key_opts) {
    debug_annotation(debug_key_text(line, opts, spec))
  } else {
    let width = key_debug_width(line, sort_delimiter(opts), spec, key_opts)
    if width == 0 { "^ no match for key" } else { text.padding(width, "_") }
  }
}

pure debug_sort_text(lines: List[Str], opts: SortOptions, has_key: Bool, key_field: Int) -> Str {
  let has_last_resort = has_key or is_month_sort(opts) or is_numeric_sort(opts) or is_human_numeric_sort(opts) or is_general_numeric_sort(opts) or is_version_sort(opts) or opts.blank or opts.fold_case or opts.dictionary or opts.ignore_nonprinting
  var output = ""
  for line in lines {
    output += debug_visible_line(line) + "\n"
    if has_key {
      for spec in opts.key {
        let key_opts = key_effective_options(spec, opts)
        let indentation = text.padding(key_start_byte(line, sort_delimiter(opts), spec, key_opts), " ")
        output += indentation + debug_key_annotation(line, opts, spec) + "\n"
      }
      if ! opts.stable and ! opts.unique { output += debug_annotation(line) + "\n" }
    } else {
      let primary = debug_primary_text(line, opts, false, key_field)
      let indentation = if is_month_sort(opts) or is_general_numeric_sort(opts) or is_numeric_sort(opts) or is_human_numeric_sort(opts) { text.padding(leading_blank_count(line), " ") } else { "" }
      output += indentation + debug_annotation(primary) + "\n"
      if has_last_resort and ! opts.stable and ! opts.unique { output += debug_annotation(line) + "\n" }
    }
  }
  output
}

pure sort_input_lines(input_lines: List[Str], opts: SortOptions, has_key: Bool, key_field: Int) -> List[Str] {
  let human_numeric = is_human_numeric_sort(opts)
  let general_numeric = is_general_numeric_sort(opts)
  ## Removing identical records before sorting preserves all results and avoids costly duplicate work.
  let unique_input = if opts.unique { input_lines |> unique-by . } else { input_lines }
  if opts.merge {
    unique_input
  } else if has_key {
    multiple_key_sort(unique_input, opts)
  } else if is_month_sort(opts) {
    if opts.reverse {
      unique_input |> sort-by(desc: true) month_sort_key(., opts.stable or opts.unique)
    } else {
      unique_input |> sort-by month_sort_key(., opts.stable or opts.unique)
    }
  } else if human_numeric {
    if opts.reverse {
      unique_input |> sort-by(desc: true) human_numeric_sort_key(., opts.stable or opts.unique)
    } else {
      unique_input |> sort-by human_numeric_sort_key(., opts.stable or opts.unique)
    }
  } else if general_numeric {
    if opts.reverse {
      unique_input |> sort-by(desc: true) general_numeric_sort_key(., opts.stable or opts.unique)
    } else {
      unique_input |> sort-by general_numeric_sort_key(., opts.stable or opts.unique)
    }
  } else if is_version_sort(opts) {
    if opts.reverse {
      unique_input |> sort-by(desc: true) version_sort_key(., opts.stable)
    } else {
      unique_input |> sort-by version_sort_key(., opts.stable)
    }
  } else if is_numeric_sort(opts) {
    ## Keep the first spelling for each number before a reverse sort can reorder equal keys.
    let numeric_lines = if opts.unique { unique_input |> unique-by numeric_sort_key(., true) } else { unique_input }
    if opts.reverse {
      numeric_lines |> sort-by(desc: true) numeric_sort_key(., opts.stable or opts.unique)
    } else {
      numeric_lines |> sort-by numeric_sort_key(., opts.stable or opts.unique)
    }
  } else if opts.blank {
    blank_sorted(unique_input, opts.reverse, opts)
  } else if opts.fold_case or opts.dictionary or opts.ignore_nonprinting {
    if opts.reverse {
      unique_input |> sort-by(desc: true) { |line| {key: character_order_key(line, opts.dictionary, opts.ignore_nonprinting, opts.fold_case), raw: if opts.stable { "" } else { line }} }
    } else {
      unique_input |> sort-by { |line| {key: character_order_key(line, opts.dictionary, opts.ignore_nonprinting, opts.fold_case), raw: if opts.stable { "" } else { line }} }
    }
  } else if opts.reverse {
    unique_input |> sort-by(desc: true) .
  } else {
    unique_input |> sort
  }
}

pure unique_sorted_lines(sorted: List[Str], opts: SortOptions, has_key: Bool) -> List[Str] {
  if opts.unique and has_key {
    unique_key_lines(sorted, opts)
  } else if opts.unique and is_month_sort(opts) {
    sorted |> unique-by month_sort_key(., true)
  } else if opts.unique and is_human_numeric_sort(opts) {
    sorted |> unique-by human_numeric_sort_key(., true)
  } else if opts.unique and is_general_numeric_sort(opts) {
    sorted |> unique-by general_numeric_sort_key(., true)
  } else if opts.unique and is_numeric_sort(opts) {
    sorted |> unique-by numeric_sort_key(., true)
  } else if opts.unique {
    sorted |> unique-by .
  } else {
    sorted
  }
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
proc read_sort_records(sources: List[SortInput], zero_terminated: Bool) [fs, io, process, error] -> List[List[Str]] {
  var records: List[List[Str]] = []
  for source in sources {
    let part = if source.stdin {
      io.stdin_text()?
    } else {
      match source.path.read_text() {
        Ok(text) => text
        Err(failure) => {
          gnu.error(f"cannot read: {gnu.quote_bytes(source.name, always: false)}: {gnu.strerror(failure)}")
          exit 2
        }
      }
    }
    records += [input_records(part, zero_terminated)]
  }
  records
}

## Merge one current record from each input; ties keep the earlier input first.
pure merge_sort_records(sources: List[List[Str]], opts: SortOptions, has_key: Bool, key_field: Int) -> List[Str] {
  let plain_text_order = ! has_key and ! is_month_sort(opts) and ! is_human_numeric_sort(opts) and
    ! is_general_numeric_sort(opts) and ! is_version_sort(opts) and ! is_numeric_sort(opts) and
    ! opts.blank and ! opts.fold_case and ! opts.dictionary and ! opts.ignore_nonprinting
  var positions: List[Int] = []
  for _ in sources { positions += [0] }
  var merged: List[Str] = []

  loop {
    var selected: Int? = null
    for index in range(sources.len()) {
      let position = positions[index]
      if position < sources[index].len() {
        if let current_index = selected {
          let candidate = sources[index][position]
          let current = sources[current_index][positions[current_index]]
          let candidate_precedes = if plain_text_order {
            if opts.reverse { candidate > current } else { candidate < current }
          } else {
            pair_is_ordered(candidate, current, opts, has_key, key_field) and
              ! pair_is_ordered(current, candidate, opts, has_key, key_field)
          }
          if candidate_precedes {
            selected = index
          }
        } else {
          selected = index
        }
      }
    }

    if let index = selected {
      merged += [sources[index][positions[index]]]
      positions[index] += 1
    } else {
      break
    }
  }

  merged
}

## Read one newline or NUL-delimited record while retaining only the unfinished suffix.
proc advance_sort_merge_reader(reader: SortMergeReader, zero_terminated: Bool) [io, error] -> Result[SortMergeReader, Error] {
  var pending = reader.pending
  var eof = reader.eof
  let separator = if zero_terminated { 0 } else { 10 }

  loop {
    for at in range(pending.len()) {
      if pending.byte_at(at) == separator {
        let record = pending[0..at].utf8()?
        return Ok({...reader, pending: pending[at + 1..], eof: eof, current: record})
      }
    }

    if eof {
      if pending.is_empty() {
        return Ok({...reader, pending: b"", current: null, eof: true})
      }
      let record = pending.utf8()?
      return Ok({...reader, pending: b"", current: record, eof: true})
    }

    let chunk = unix.read_fd(reader.fd, 65536)?
    if chunk.is_empty() {
      eof = true
    } else {
      pending = bytes.concat([pending, chunk])
    }
  }
  fail "sort merge reader ended unexpectedly"
}

## Open all source streams only when their descriptor count stays within the process and batch limits.
proc can_stream_sort_merge(inputs: List[SortInput], opts: SortOptions, check_enabled: Bool, inputs_are_regular: Bool) [process, error] -> Bool {
  if ! opts.merge or check_enabled or opts.debug or ! opts.output.is_empty() or ! inputs_are_regular { return false }

  let limit = process.rlimit("nofile")?
  let open_limit = if let soft = limit.soft { soft - 3 } else { inputs.len() }
  let batch_limit = if let size = opts.batch_size { size.parse_int()? } else { open_limit }
  let maximum = if batch_limit < open_limit { batch_limit } else { open_limit }
  inputs.len() <= maximum
}

## Merge sorted file streams directly to stdout so a failed writer stops further reads.
proc stream_sort_merge(inputs: List[SortInput], opts: SortOptions, has_key: Bool, key_field: Int) [fs, io, env, process, error] {
  var readers: List[SortMergeReader] = []
  for input in inputs {
    match unix.open_fd(input.path) {
      Ok(fd) => readers += [{fd: fd, name: input.name, pending: b"", eof: false, current: null}]
      Err(failure) => {
        gnu.error(f"cannot read: {gnu.quote_bytes(input.name, always: false)}: {gnu.strerror(failure)}")
        exit 2
      }
    }
  }

  for index in range(readers.len()) {
    match advance_sort_merge_reader(readers[index], opts.zero_terminated) {
      Ok(reader) => readers[index] = reader
      Err(failure) => {
        gnu.error(f"cannot read: {gnu.quote_bytes(readers[index].name, always: false)}: {gnu.strerror(failure)}")
        exit 2
      }
    }
  }

  let line_ending = if opts.zero_terminated { "\0" } else { "\n" }
  var output = ""
  var previous: Str? = null
  loop {
    var selected: Int? = null
    for index in range(readers.len()) {
      if let candidate = readers[index].current {
        if let current_index = selected {
          let current = readers[current_index].current ?? ""
          let plain_text_order = ! has_key and ! is_month_sort(opts) and ! is_human_numeric_sort(opts) and
            ! is_general_numeric_sort(opts) and ! is_version_sort(opts) and ! is_numeric_sort(opts) and
            ! opts.blank and ! opts.fold_case and ! opts.dictionary and ! opts.ignore_nonprinting
          let candidate_precedes = if plain_text_order {
            if opts.reverse { candidate > current } else { candidate < current }
          } else {
            pair_is_ordered(candidate, current, opts, has_key, key_field) and
              ! pair_is_ordered(current, candidate, opts, has_key, key_field)
          }
          if candidate_precedes { selected = index }
        } else {
          selected = index
        }
      }
    }

    if let index = selected {
      let line = readers[index].current ?? ""
      let duplicate = if opts.unique {
        if let prior = previous { same_sort_key(prior, line, opts, has_key, key_field) } else { false }
      } else { false }
      if ! duplicate {
        output += f"{line}{line_ending}"
        if output.byte_len() >= 65536 {
          write_sort_stdout(output)
          output = ""
        }
      }
      previous = line
      match advance_sort_merge_reader(readers[index], opts.zero_terminated) {
        Ok(reader) => readers[index] = reader
        Err(failure) => {
          gnu.error(f"cannot read: {gnu.quote_bytes(readers[index].name, always: false)}: {gnu.strerror(failure)}")
          exit 2
        }
      }
    } else {
      break
    }
  }

  if output != "" { finish_sort_stdout(output) }
  for reader in readers { unix.close_fd(reader.fd)? }
}

## GNU sort reserves three descriptors for standard streams and signal handling.
proc validate_batch_size(value: Str) [process, env, error] -> Unit {
  let raw = bytes.from_text(value)
  var valid = raw.len() > 0
  for at in range(raw.len()) {
    if ! is_ascii_digit(raw.byte_at(at) ?? 0) { valid = false }
  }
  if ! valid {
    gnu.error(f"invalid --batch-size argument {gnu.quote(value)}")
    exit 2
  }

  let limit = process.rlimit("nofile")?
  let maximum = if let soft = limit.soft { soft - 3 } else { 9223372036854775807 }
  match value.parse_int() {
    Ok(size) => {
      if size < 2 {
        gnu.error(f"invalid --batch-size argument {gnu.quote(value)}")
        gnu.error("minimum --batch-size argument is '2'")
        exit 2
      }
      if size > maximum {
        gnu.error(f"--batch-size argument {gnu.quote(value)} too large")
        gnu.error(f"maximum --batch-size argument with current rlimit is {maximum}")
        exit 2
      }
    }
    Err(_) => {
      gnu.error(f"--batch-size argument {gnu.quote(value)} too large")
      gnu.error(f"maximum --batch-size argument with current rlimit is {maximum}")
      exit 2
    }
  }
}

proc parse_buffer_size(value: Str) [fs, env, process, error] -> Int {
  let raw = bytes.from_text(value)
  var digits = 0
  while digits < raw.len() and is_ascii_digit(raw.byte_at(digits) ?? 0) { digits += 1 }
  if digits == 0 {
    gnu.error(f"invalid --buffer-size argument {gnu.quote(value)}")
    exit 2
  }

  let number_text = value.byte_slice(0, length: digits)
  let number = match number_text.parse_int() {
    Ok(number) => number
    Err(_) => {
      gnu.error(f"--buffer-size argument {gnu.quote(value)} too large")
      exit 2
    }
  }
  let suffix = value.byte_slice(digits)
  if suffix != "%" and suffix.find("%") != null {
    gnu.error(f"invalid --buffer-size argument {gnu.quote(value)}")
    exit 2
  }
  if suffix == "%" {
    let memory = linux.meminfo()?
    if number > 100 {
      gnu.error(f"--buffer-size argument {gnu.quote(value)} too large")
      exit 2
    }
    return memory.total / 100 * number + memory.total % 100 * number / 100
  }

  let power = if suffix in ["", "b"] { 0 } else if suffix in ["k", "K"] { 1 } else if suffix == "M" { 2 } else if suffix == "G" { 3 } else if suffix == "T" { 4 } else if suffix == "P" { 5 } else if suffix == "E" { 6 } else if suffix == "Z" { 7 } else if suffix == "Y" { 8 } else {
    gnu.error(f"invalid suffix in --buffer-size argument {gnu.quote(value)}")
    exit 2
  }

  let maximum = 9223372036854775807
  var multiplier = 1
  for _ in range(power) {
    if multiplier > maximum / 1024 {
      gnu.error(f"--buffer-size argument {gnu.quote(value)} too large")
      exit 2
    }
    multiplier *= 1024
  }
  if number > maximum / multiplier {
    gnu.error(f"--buffer-size argument {gnu.quote(value)} too large")
    exit 2
  }
  number * multiplier
}

## Flush an intermediate merge chunk so failed output stops further input reads.
proc write_sort_stdout(text: Str) [io, env, process] -> Unit {
  gnu.write_text(text)
}

## Final merge output uses sort's named stdout diagnostic.
proc finish_sort_stdout(text: Str) [io, env, process] -> Unit {
  if let Err(failure) = io.write_stdout(text) {
    gnu.error(f"write failed: 'standard output': {gnu.strerror(failure)}")
    exit 2
  }
  if let Err(failure) = io.flush_stdout() {
    gnu.error(f"write failed: 'standard output': {gnu.strerror(failure)}")
    exit 2
  }
}

proc write_sort_fd(fd: Int, text: Str, output_name: Str) [io, env, process, error] -> Unit {
  let data = bytes.from_text(text)
  var offset = 0
  while offset < data.len() {
    match unix.write_fd(fd, data[offset..]) {
      Ok(written) => {
        if written == 0 {
          gnu.error(f"write failed: {gnu.quote(output_name)}: descriptor write made no progress")
          exit 2
        }
        offset += written
      }
      Err(failure) => {
        gnu.error(f"write failed: {gnu.quote(output_name)}: {gnu.strerror(failure)}")
        exit 2
      }
    }
  }
}

proc create_sort_temp_root(directory: Str?) [fs, io, process, env, error] -> FsRoot {
  let created = if let temp_dir = directory { fs.tempdir_in(fp"{temp_dir}") } else { fs.tempdir() }
  match created {
    Ok(root) => root
    Err(failure) => {
      gnu.error(f"cannot create temporary file: {gnu.strerror(failure)}")
      exit 2
    }
  }
}

proc sort_temp_path(root: FsRoot, name: Str) [fs, error] -> Path {
  let relative = fp"{name}"
  root.write(relative, b"")?
  root.chmod(relative, 0o600)?
  fp"{root.host_path()?}/{name}"
}

proc write_sort_run(root: FsRoot, name: Str, records: List[Str], opts: SortOptions, has_key: Bool, key_field: Int) [fs, error] -> Path {
  let sorted = sort_input_lines(records, opts, has_key, key_field)
  let ending = if opts.zero_terminated { "\0" } else { "\n" }
  let contents = if sorted.is_empty() { "" } else { f"{sorted.join(ending)}{ending}" }
  let relative = fp"{name}"
  root.write(relative, contents)?
  root.chmod(relative, 0o600)?
  fp"{root.host_path()?}/{name}"
}

proc sort_merge_batch_limit(opts: SortOptions) [process, error] -> Int {
  let limit = process.rlimit("nofile")?
  let available = if let soft = limit.soft { if soft > 6 { soft - 6 } else { 2 } } else { 32 }
  let bounded = if available > 64 { 64 } else { available }
  if let requested = opts.batch_size {
    let size = requested.parse_int()?
    if size < bounded { size } else { bounded }
  } else {
    bounded
  }
}

proc merge_sort_stream(inputs: List[SortInput], opts: SortOptions, has_key: Bool, key_field: Int, output_fd: Int?, deduplicate: Bool) [fs, io, env, process, error] {
  var readers: List[SortMergeReader] = []
  defer {
    for reader in readers {
      if reader.fd != 0 { let _ = unix.close_fd(reader.fd) }
    }
  }

  for input in inputs {
    let fd = if input.stdin {
      0
    } else {
      match unix.open_fd(input.path) {
        Ok(fd) => fd
        Err(failure) => {
          gnu.error(f"cannot read: {gnu.quote_bytes(input.name, always: false)}: {gnu.strerror(failure)}")
          exit 2
        }
      }
    }
    readers += [{fd: fd, name: input.name, pending: b"", eof: false, current: null}]
  }

  for index in range(readers.len()) {
    match advance_sort_merge_reader(readers[index], opts.zero_terminated) {
      Ok(reader) => readers[index] = reader
      Err(failure) => {
        gnu.error(f"cannot read: {gnu.quote_bytes(readers[index].name, always: false)}: {gnu.strerror(failure)}")
        exit 2
      }
    }
  }

  let line_ending = if opts.zero_terminated { "\0" } else { "\n" }
  var output = ""
  var previous: Str? = null
  loop {
    var selected: Int? = null
    for index in range(readers.len()) {
      if let candidate = readers[index].current {
        if let current_index = selected {
          let current = readers[current_index].current ?? ""
          let plain_text_order = ! has_key and ! is_month_sort(opts) and ! is_human_numeric_sort(opts) and
            ! is_general_numeric_sort(opts) and ! is_version_sort(opts) and ! is_numeric_sort(opts) and
            ! opts.blank and ! opts.fold_case and ! opts.dictionary and ! opts.ignore_nonprinting
          let candidate_precedes = if plain_text_order {
            if opts.reverse { candidate > current } else { candidate < current }
          } else {
            pair_is_ordered(candidate, current, opts, has_key, key_field) and
              ! pair_is_ordered(current, candidate, opts, has_key, key_field)
          }
          if candidate_precedes { selected = index }
        } else {
          selected = index
        }
      }
    }

    if let index = selected {
      let line = readers[index].current ?? ""
      let duplicate = if deduplicate {
        if let prior = previous { same_sort_key(prior, line, opts, has_key, key_field) } else { false }
      } else { false }
      if ! duplicate {
        output += f"{line}{line_ending}"
        if output.byte_len() >= 65536 {
          if let fd = output_fd { write_sort_fd(fd, output, "temporary file") } else { finish_sort_stdout(output) }
          output = ""
        }
      }
      previous = line
      match advance_sort_merge_reader(readers[index], opts.zero_terminated) {
        Ok(reader) => readers[index] = reader
        Err(failure) => {
          gnu.error(f"cannot read: {gnu.quote_bytes(readers[index].name, always: false)}: {gnu.strerror(failure)}")
          exit 2
        }
      }
    } else {
      break
    }
  }

  if output != "" {
    if let fd = output_fd { write_sort_fd(fd, output, "temporary file") } else { finish_sort_stdout(output) }
  }
}

proc merge_sort_batched(inputs: List[SortInput], root: FsRoot, opts: SortOptions, has_key: Bool, key_field: Int, batch_limit: Int, output_fd: Int?, deduplicate: Bool) [fs, io, env, process, error] {
  var runs = inputs
  var pass = 0
  while runs.len() > batch_limit {
    var next: List[SortInput] = []
    var start = 0
    var group_number = 0
    while start < runs.len() {
      let stop = if start + batch_limit < runs.len() { start + batch_limit } else { runs.len() }
      var input_group: List[SortInput] = []
      for index in range(start, stop) { input_group += [runs[index]] }
      if input_group.len() == 1 {
        next += input_group
      } else {
        let name = f"merge-{pass}-{group_number}"
        let run_file = sort_temp_path(root, name)
        let fd = unix.open_fd(run_file, write: true)?
        merge_sort_stream(input_group, opts, has_key, key_field, fd, false)
        unix.close_fd(fd)?
        next += [{name: bytes.from_text(run_file.display()), path: run_file, stdin: false}]
      }
      start = stop
      group_number += 1
    }
    runs = next
    pass += 1
  }
  merge_sort_stream(runs, opts, has_key, key_field, output_fd, deduplicate)
}

proc buffered_sort(inputs: List[SortInput], opts: SortOptions, buffer_size: Int, has_key: Bool, key_field: Int, output: Path, has_output: Bool) [fs, io, env, process, error] {
  var temp_root: FsRoot? = null
  defer { if let root = temp_root { root.close()? } }

  let requested_limit = if buffer_size == 0 { 9223372036854775807 } else { buffer_size }
  ## A minimum run size prevents tiny -S values from creating a temporary file for every short record.
  let chunk_limit = if requested_limit < 65536 { 65536 } else { requested_limit }
  let separator_bytes = if opts.zero_terminated { 1 } else { 1 }
  var chunk: List[Str] = []
  var chunk_bytes = 0
  var runs: List[Path] = []
  var run_number = 0

  for input in inputs {
    let fd = if input.stdin {
      0
    } else {
      match unix.open_fd(input.path) {
        Ok(fd) => fd
        Err(failure) => {
          gnu.error(f"cannot read: {gnu.quote_bytes(input.name, always: false)}: {gnu.strerror(failure)}")
          exit 2
        }
      }
    }
    var reader: SortMergeReader = {fd: fd, name: input.name, pending: b"", eof: false, current: null}
    loop {
      match advance_sort_merge_reader(reader, opts.zero_terminated) {
        Ok(next) => {
          reader = next
          if let line = reader.current {
            chunk += [line]
            chunk_bytes += line.byte_len() + separator_bytes
            if chunk_bytes >= chunk_limit {
              if temp_root == null { temp_root = create_sort_temp_root(opts.temp_directory) }
              if let root = temp_root {
                runs += [write_sort_run(root, f"run-{run_number}", chunk, opts, has_key, key_field)]
              } else {
                fail "sort temporary directory was not created"
              }
              run_number += 1
              chunk = []
              chunk_bytes = 0
            }
          } else {
            break
          }
        }
        Err(failure) => {
          gnu.error(f"cannot read: {gnu.quote_bytes(input.name, always: false)}: {gnu.strerror(failure)}")
          exit 2
        }
      }
    }
    if ! input.stdin { unix.close_fd(fd)? }
  }

  if runs.is_empty() {
    let lines = unique_sorted_lines(sort_input_lines(chunk, opts, has_key, key_field), opts, has_key)
    let line_ending = if opts.zero_terminated { "\0" } else { "\n" }
    let output_text = if lines.is_empty() { "" } else { f"{lines.join(line_ending)}{line_ending}" }
    if has_output {
      if let Err(failure) = output.write(output_text) {
        let action = if let Ok(_) = fs.stat(output, follow_symlinks: true) { "write failed" } else { "open failed" }
        gnu.error(f"{action}: {gnu.quote_maybe(output.display())}: {gnu.strerror(failure)}")
        exit 2
      }
    } else if opts.zero_terminated {
      write_sort_stdout(output_text)
    } else {
      for line in lines { print $line }
    }
    return
  }

  if ! chunk.is_empty() {
    if let root = temp_root {
      runs += [write_sort_run(root, f"run-{run_number}", chunk, opts, has_key, key_field)]
    } else {
      fail "sort temporary directory was not created"
    }
  }
  if let root = temp_root {
    var run_inputs: List[SortInput] = []
    for run_file in runs { run_inputs += [{name: bytes.from_text(run_file.display()), path: run_file, stdin: false}] }
    let batch_limit = sort_merge_batch_limit(opts)
    if has_output {
      if let Err(failure) = output.write("") {
        let action = if let Ok(_) = fs.stat(output, follow_symlinks: true) { "write failed" } else { "open failed" }
        gnu.error(f"{action}: {gnu.quote_maybe(output.display())}: {gnu.strerror(failure)}")
        exit 2
      }
      let fd = unix.open_fd(output, write: true)?
      merge_sort_batched(run_inputs, root, opts, has_key, key_field, batch_limit, fd, opts.unique)
      unix.close_fd(fd)?
    } else {
      merge_sort_batched(run_inputs, root, opts, has_key, key_field, batch_limit, null, opts.unique)
    }
  } else {
    fail "sort temporary directory was not created"
  }
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
      month: {
        form: "-M --month-sort",
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
      buffer_size: {
        form: "-S --buffer-size SIZE",
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
      merge: {
        form: "-m --merge",
        default: false,
      },
      batch_size: {
        form: "--batch-size SIZE",
      },
      temp_directory: {
        form: "-T --temporary-directory DIR",
      },
      key: {
        form: "-k KEY",
        repeated: true,
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
      files0_from: {
        form: "--files0-from FILE",
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

  let buffer_size: Int? = if let value = opts.buffer_size { parse_buffer_size(value) } else { null }
  if let batch_size = opts.batch_size { validate_batch_size(batch_size) }

  let selected_mode = selected_sort_mode(opts)
  let general_numeric = is_general_numeric_sort(opts)
  let human_numeric = is_human_numeric_sort(opts)
  if selected_mode != "" and ! selected_mode.starts_with("v") and ! is_human_numeric_mode(selected_mode) and ! is_month_mode(selected_mode) and ! is_general_numeric_mode(selected_mode) and selected_mode not in ["n", "numeric"] {
    gnu.error(f"invalid argument {gnu.quote_maybe(selected_mode)} for '--sort'")
    exit 2
  }

  let numeric_requested = opts.numeric or is_numeric_sort(opts)
  let general_numeric_requested = opts.general_numeric or general_numeric
  let human_numeric_requested = opts.human_numeric or human_numeric
  let month_requested = opts.month or is_month_sort(opts)
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
  if month_requested and general_numeric_requested {
    gnu.error("options '-gM' are incompatible")
    exit 2
  }
  if month_requested and human_numeric_requested {
    gnu.error("options '-hM' are incompatible")
    exit 2
  }
  if month_requested and numeric_requested {
    gnu.error("options '-Mn' are incompatible")
    exit 2
  }

  if (is_numeric_sort(opts) or human_numeric) and (opts.dictionary or opts.ignore_nonprinting) {
    let conflict = if opts.dictionary { if human_numeric { "-dh" } else { "-dn" } } else { if human_numeric { "-hi" } else { "-in" } }
    gnu.error(f"options '{conflict}' are incompatible")
    exit 2
  }

  let has_key = ! opts.key.is_empty()
  let key_field = if has_key { key_index(primary_key_spec(opts)) } else { 0 }
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
  let delimiter = sort_delimiter(opts)
  let paths = opts.paths
  if delimiter != "" and delimiter.count_chars() != 1 {
    gnu.error(f"separator must be exactly one character long: {gnu.quote(delimiter)}")
    exit 2
  }

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

  var inputs: List[SortInput] = []
  if let list = opts.files0_from {
    if ! paths.is_empty() {
      gnu.error(f"extra operand {gnu.quote(paths[0])}")
      eprint "file operands cannot be combined with --files0-from"
      exit 2
    }

    let data = if list == "-" {
      io.stdin_bytes()?
    } else {
      match fp"{list}".read_bytes() {
        Ok(data) => data
        Err(failure) => {
          let action = if gnu.errno(failure) == 21 { "cannot read" } else { "open failed" }
          gnu.error(f"{action}: {gnu.quote_maybe(list)}: {gnu.strerror(failure)}")
          exit 2
        }
      }
    }
    let names = files0_names(data)
    if names.is_empty() {
      gnu.error(f"no input from {gnu.quote(list)}")
      exit 2
    }
    for index in range(names.len()) {
      let name = names[index]
      if name.is_empty() {
        gnu.error(f"{gnu.quote_maybe(list)}:{index + 1}: invalid zero-length file name")
        exit 2
      }
      if list == "-" and name == b"-" {
        gnu.error("when reading file names from standard input, no file name of '-' allowed")
        exit 2
      }
      let input_path = Path.parse_bytes(name)?
      inputs += [{name: name, path: input_path, stdin: name == b"-"}]
    }
  } else if paths.is_empty() {
    inputs = [{name: b"-", path: p"-", stdin: true}]
  } else {
    for path_arg in paths {
      inputs += [{name: bytes.from_text(path_arg), path: fp"{path_arg}", stdin: path_arg == "-"}]
    }
  }

  var inputs_are_regular = true
  for input in inputs {
    if input.stdin {
      inputs_are_regular = false
    } else {
      match fs.stat(input.path, follow_symlinks: true) {
        Ok(metadata) => if metadata.mode / 4096 % 16 != 8 { inputs_are_regular = false }
        Err(failure) => {
          gnu.error(f"cannot read: {gnu.quote_bytes(input.name, always: false)}: {gnu.strerror(failure)}")
          exit 2
        }
      }
    }
  }

  if opts.merge and ! check_enabled and ! opts.debug and ! has_output and inputs_are_regular {
    let batch_limit = sort_merge_batch_limit(opts)
    if inputs.len() > batch_limit {
      let scratch = create_sort_temp_root(opts.temp_directory)
      defer scratch.close()?
      merge_sort_batched(inputs, scratch, opts, has_key, key_field, batch_limit, null, opts.unique)
      return
    }
  }

  if let size = buffer_size {
    if ! opts.merge and ! check_enabled and ! opts.debug {
      buffered_sort(inputs, opts, size, has_key, key_field, output, has_output)
      return
    }
  }

  if can_stream_sort_merge(inputs, opts, check_enabled, inputs_are_regular) {
    stream_sort_merge(inputs, opts, has_key, key_field)
    return
  }
  let source_records = read_sort_records(inputs, opts.zero_terminated)
  var input_lines: List[Str] = []
  if opts.merge {
    input_lines = merge_sort_records(source_records, opts, has_key, key_field)
  } else {
    for records in source_records { input_lines += records }
  }
  if check_enabled {
    if input_lines.len() > 1 {
      for index in range(1, input_lines.len()) {
        let previous = input_lines[index - 1]
        let current = input_lines[index]
        let unordered = ! pair_is_ordered(previous, current, opts, has_key, key_field)
        let duplicate = opts.unique and same_sort_key(previous, current, opts, has_key, key_field)

        if unordered or duplicate {
          if ! silent_check {
            let name = inputs[0].name
            if opts.zero_terminated {
              io.write_stderr(f"{gnu.prog()}: {gnu.quote_bytes(name, always: false)}:{index + 1}: disorder: {current}\0")?
            } else {
              gnu.error(f"{gnu.quote_bytes(name, always: false)}:{index + 1}: disorder: {current}")
            }
          }
          exit 1
        }
      }
    }
    return
  }

  let sorted = sort_input_lines(input_lines, opts, has_key, key_field)
  let lines = unique_sorted_lines(sorted, opts, has_key)

  let line_ending = if opts.zero_terminated { "\0" } else { "\n" }
  let text = if opts.debug {
    debug_sort_text(lines, opts, has_key, key_field)
  } else if opts.merge or lines.is_empty() {
    ""
  } else {
    f"{lines.join(line_ending)}{line_ending}"
  }

  if has_output {
    let output_text = if opts.merge and ! opts.debug and ! lines.is_empty() {
      f"{lines.join(line_ending)}{line_ending}"
    } else { text }
    if let Err(failure) = output.write(output_text) {
      let action = if let Ok(_) = fs.stat(output, follow_symlinks: true) { "write failed" } else { "open failed" }
      gnu.error(f"{action}: {gnu.quote_maybe(output_path)}: {gnu.strerror(failure)}")
      exit 2
    }
  } else if opts.merge and ! opts.debug {
    for line in lines {
      finish_sort_stdout(f"{line}{line_ending}")
    }
  } else if opts.zero_terminated or opts.debug {
    write_sort_stdout(text)
  } else {
    for line in lines {
      print $line
    }
  }
}
