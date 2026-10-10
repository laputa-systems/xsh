##! Byte input and number parsing for the byte-level `cmp` comparison.

## A parsed size: its value and the text left after the number and suffix.
export type Size = {value: Int, rest: Str}

# The largest offset cmp accepts; larger values saturate to it.
const LARGEST = 9223372036854775807

pure digit_value(byte: Int) -> Int {
  if byte >= 48 and byte <= 57 { return byte - 48 }
  if byte >= 97 and byte <= 102 { return byte - 87 }
  if byte >= 65 and byte <= 70 { return byte - 55 }
  -1
}

# Multiply with saturation at `LARGEST`.
pure scale(value: Int, factor: Int) -> Int {
  if value == 0 or factor == 0 { return 0 }
  if value > LARGEST / factor { return LARGEST }
  value * factor
}

# How many powers of the base a multiplier letter stands for; zero when the
# letter is not a multiplier.
pure multiplier_steps(letter: Str) -> Int {
  match letter {
    "k" => 1
    "K" => 1
    "M" => 2
    "G" => 3
    "T" => 4
    "P" => 5
    "E" => 6
    "Z" => 7
    "Y" => 8
    _ => 0
  }
}

## Parse an unsigned number the way `xstrtoumax` does for cmp: optional leading
## white space and `+`, a decimal, octal (`0` prefix) or hexadecimal (`0x`)
## value, and an optional multiplier (`k`, `K`, `M`, `G`, `T`, `P`, `E`, `Z`,
## `Y`: binary by default, decimal when followed by `B`, binary again for
## `iB`; a lone letter counts as one unit). Values past the range saturate.
## The result is null when no number starts the text.
export pure parse_size(text: Str) -> Size? {
  let raw = bytes.from_text(text)
  let size = raw.len()
  var at = 0
  while at < size and (raw.byte_at(at) == 32 or ((raw.byte_at(at) ?? 0) >= 9 and (raw.byte_at(at) ?? 0) <= 13)) { at += 1 }
  if at < size and raw.byte_at(at) == 43 { at += 1 }
  if at >= size { return null }
  let leading = raw[at..at + 1].utf8() ?? ""
  let bare_multiplier = multiplier_steps(leading) > 0
  if !bare_multiplier and (digit_value(raw.byte_at(at) ?? 0) < 0 or digit_value(raw.byte_at(at) ?? 0) >= 10) { return null }
  var base = 10
  var start = at
  if raw.byte_at(at) == 48 {
    let marker = raw.byte_at(at + 1) ?? 0
    let after = digit_value(raw.byte_at(at + 2) ?? 0)
    let next = digit_value(marker)
    if (marker == 120 or marker == 88) and after >= 0 and after < 16 {
      base = 16
      start = at + 2
    } else if next >= 0 and next < 8 {
      base = 8
      start = at + 1
    }
  }
  # A multiplier letter with no digits counts once, so `K` is 1024.
  var value = if bare_multiplier { 1 } else { 0 }
  var cursor = start
  while cursor < size and !bare_multiplier {
    let found = digit_value(raw.byte_at(cursor) ?? 0)
    if found < 0 or found >= base { break }
    value = if value > (LARGEST - found) / base { LARGEST } else { value * base + found }
    cursor += 1
  }
  let letter = if cursor < size { raw[cursor..cursor + 1].utf8() ?? "" } else { "" }
  let steps = multiplier_steps(letter)
  if steps > 0 {
    var factor = 1024
    var used = 1
    let following = if cursor + 1 < size { raw[cursor + 1..cursor + 2].utf8() ?? "" } else { "" }
    if following == "B" {
      factor = 1000
      used = 2
    } else if following == "i" and cursor + 2 < size and raw.byte_at(cursor + 2) == 66 {
      used = 3
    }
    var power = 1
    for _ in range(steps) { power = scale(power, factor) }
    value = scale(value, power)
    cursor += used
  }
  {value: value, rest: text.byte_slice(cursor)}
}

## Consume a bounded prefix of stdin before comparing the remaining bytes.
export proc skip_stdin(count: Int) [io, error] -> Result[Unit, Error] {
  var remaining = count
  while remaining > 0 {
    let chunk = io.stdin_read(if remaining < 65536 { remaining } else { 65536 })?
    if chunk.len() == 0 { break }
    remaining -= chunk.len()
  }
  Ok()
}

## Count LF bytes: the number of completed lines.
export pure newline_count(data: Bytes) -> Int {
  data.count_lines() - (if data.len() > 0 and !data.ends_with(b"\n") { 1 } else { 0 })
}

## Display bytes with caret control notation and a meta prefix for the high bit.
export pure display_byte(value: Int) -> Str {
  let lower = value % 128
  let prefix = if value >= 128 { "M-" } else { "" }
  if lower == 127 { return prefix + "^?" }
  let ascii = if lower < 32 { lower + 64 } else { lower }
  prefix + (if lower < 32 { "^" } else { "" }) + ((bytes.from_ints([ascii]) ?? b"?").utf8() ?? "?")
}

## Right-align text with spaces to the requested byte width.
export pure pad(text: Str, width: Int) -> Str {
  var result = text
  while result.byte_len() < width { result = " " + result }
  result
}

## Left-align text with spaces to the requested byte width.
export pure pad_right(text: Str, width: Int) -> Str {
  var result = text
  while result.byte_len() < width { result = result + " " }
  result
}

## Format a byte as octal, right-aligned to three columns.
export pure octal(value: Int) -> Str {
  var text = ""
  var rest = value
  if rest == 0 { text = "0" }
  while rest > 0 {
    text = f"{rest % 8}{text}"
    rest = rest / 8
  }
  pad(text, 3)
}
