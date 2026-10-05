##! Byte records and tab stops for the text applets.
use gnu

## Successful input and whether any operand failed.
export type Input = {data: Bytes, failed: Bool}
## Absolute stops and the continuation interval after the last stop.
export type Tabs = {stops: List[Int], interval: Int, relative: Bool}

# Combining and wide character ranges used by the byte-preserving renderers.
const ZERO = rx"[\x{300}-\x{36f}\x{483}-\x{489}\x{591}-\x{5bd}\x{5bf}\x{5c1}\x{5c2}\x{5c4}\x{5c5}\x{5c7}\x{610}-\x{61a}\x{64b}-\x{65f}\x{670}\x{6d6}-\x{6dc}\x{6df}-\x{6e4}\x{6e7}\x{6e8}\x{6ea}-\x{6ed}\x{711}\x{730}-\x{74a}\x{7a6}-\x{7b0}\x{900}-\x{902}\x{93a}\x{93c}\x{941}-\x{948}\x{94d}\x{951}-\x{957}\x{962}\x{963}\x{e31}\x{e34}-\x{e3a}\x{e47}-\x{e4e}\x{200b}-\x{200f}\x{2060}-\x{2064}\x{20d0}-\x{20f0}\x{fe00}-\x{fe0f}\x{fe20}-\x{fe2f}\x{feff}\x{1ab0}-\x{1aff}\x{1dc0}-\x{1dff}]"
const WIDE = rx"[\x{1100}-\x{115f}\x{231a}\x{231b}\x{2329}\x{232a}\x{23e9}-\x{23ec}\x{23f0}\x{23f3}\x{25fd}\x{25fe}\x{2614}\x{2615}\x{2648}-\x{2653}\x{267f}\x{2693}\x{26a1}\x{26aa}\x{26ab}\x{26bd}\x{26be}\x{26c4}\x{26c5}\x{26ce}\x{26d4}\x{26ea}\x{26f2}\x{26f3}\x{26f5}\x{26fa}\x{26fd}\x{2705}\x{270a}\x{270b}\x{2728}\x{274c}\x{274e}\x{2753}-\x{2755}\x{2757}\x{2795}-\x{2797}\x{27b0}\x{27bf}\x{2b1b}\x{2b1c}\x{2b50}\x{2b55}\x{2e80}-\x{303e}\x{3041}-\x{33ff}\x{3400}-\x{4dbf}\x{4e00}-\x{9fff}\x{a000}-\x{a4cf}\x{a960}-\x{a97f}\x{ac00}-\x{d7a3}\x{f900}-\x{faff}\x{fe10}-\x{fe19}\x{fe30}-\x{fe6f}\x{ff00}-\x{ff60}\x{ffe0}-\x{ffe6}\x{1f300}-\x{1f64f}\x{1f900}-\x{1f9ff}\x{20000}-\x{3fffd}]"

## One UTF-8 character, or one undecodable byte, and its terminal width.
export type Character = {size: Int, width: Int}

## Decode one character without replacing invalid input bytes.
export pure character(data: Bytes, at: Int) -> Character {
  let lead = data.byte_at(at) ?? 0
  if lead < 128 { return {size: 1, width: if lead < 32 or lead == 127 { 0 } else { 1 }} }
  let size = if lead >= 194 and lead <= 223 { 2 } else if lead >= 224 and lead <= 239 { 3 } else if lead >= 240 and lead <= 244 { 4 } else { 1 }
  if size == 1 or at + size > data.len() { return {size: 1, width: 1} }
  guard let decoded = data[at..at + size].utf8() else { |_| return {size: 1, width: 1} }
  let width = if ZERO.matches(decoded) or rx"[\x7f-\x9f]".matches(decoded) { 0 } else if WIDE.matches(decoded) { 2 } else { 1 }
  {size: size, width: width}
}

## Read operands in order and retain successful input after a read error.
export proc read(paths: List[Str]) -> Input {
  var chunks: List[Bytes] = []
  var failed = false
  for name in if paths.is_empty() { ["-"] } else { paths } {
    match gnu.read_operand(name) {
      Ok(data) => { chunks += [data] }
      Err(failure) => { gnu.name_error(name, failure); failed = true }
    }
  }
  {data: bytes.concat(chunks), failed: failed}
}

## Split on a byte delimiter, keeping an unterminated last record.
export pure records(data: Bytes, mark = 10) -> List[Bytes] {
  var start = 0
  collect {
    for at in range(data.len()) {
      if data.byte_at(at) == mark {
        yield data[start..at]
        start = at + 1
      }
    }
    yield data[start..] when start < data.len()
  }
}

## Repeat a character only when output requires it.
export pure padding(count: Int, character = " ") -> Str {
  return "" when count <= 0
  [character for _ in range(count)].join("")
}

## Parse GNU tab lists, including final /N and +N continuation stops.
export proc tabs(values: List[Str]) -> Tabs {
  var stops: List[Int] = []
  var interval = 0
  var relative = false
  var last_marker = ""
  for value in values {
    for part in value.replace(",", with: " ").fields() {
      if interval > 0 { gnu.usage_error(f"{gnu.quote_value(last_marker)} specifier only allowed with the last value") }
      var prefix = 0
      var marker = ""
      while prefix < part.byte_len() and part.byte_slice(prefix, length: 1) in ["/", "+"] {
        marker = part.byte_slice(prefix, length: 1); prefix += 1
      }
      let extended = prefix > 0
      let digits = part.byte_slice(prefix)
      if digits == "" { continue }
      if ! rx"^[0-9]+$".matches(digits) {
        for offset in range(digits.byte_len()) {
          let mark = digits.byte_slice(offset, length: 1)
          if mark in ["/", "+"] { gnu.usage_error(f"{gnu.quote_value(mark)} specifier not at start of number: {gnu.quote_value(digits.byte_slice(offset))}") }
        }
        var first_invalid = 0
        while first_invalid < digits.byte_len() and rx"^[0-9]$".matches(digits.byte_slice(first_invalid, length: 1)) { first_invalid += 1 }
        gnu.usage_error(f"tab size contains invalid character(s): {gnu.quote_value(digits.byte_slice(first_invalid))}")
      }
      guard let number = digits.parse_int() else { |_| gnu.usage_error(f"tab stop is too large {gnu.quote_value(digits)}"); exit 1 }
      if number == 0 and extended and ! stops.is_empty() { continue }
      if number <= 0 { gnu.usage_error("tab size cannot be 0") }
      if extended {
        interval = number
        last_marker = marker
        relative = marker == "+"
      } else {
        if ! stops.is_empty() and number <= stops[-1] { gnu.usage_error("tab sizes must be ascending") }
        stops += [number]
      }
    }
  }
  if stops.is_empty() and interval == 0 { interval = 8 }
  if stops.len() == 1 and interval == 0 { interval = stops[0]; stops = [] }
  {stops: stops, interval: interval, relative: relative}
}

## Return the next column or -1 when a finite tab list is exhausted.
export pure next_stop(column: Int, spec: Tabs) -> Int {
  for stop in spec.stops { return stop when stop > column }
  if spec.interval == 0 { return -1 }
  let base = if spec.relative and ! spec.stops.is_empty() { spec.stops[-1] } else { 0 }
  let distance = spec.interval - (column - base) % spec.interval
  if column > 9223372036854775807 - distance { -1 } else { column + distance }
}

## Rewrite obsolete numeric options without changing operands after --.
export pure numeric_options(argv: List[Str], option: Str) -> List[Str] {
  var enabled = true
  var value = false
  let long = if option == "-w" { "--width" } else { "--tabs" }
  collect {
    for item in argv {
      if item == "--" { enabled = false }
      if value { yield item; value = false; continue }
      if enabled and rx"^-[0-9][0-9,]*$".matches(item) {
        yield @[option, item.byte_slice(1)]
      } else {
        yield item
        value = enabled and (item == long or (item.starts_with("-") and ! item.starts_with("--") and item.ends_with(option.byte_slice(1))))
      }
    }
  }
}

## Expand tabs with byte-preserving backspace and newline accounting.
export proc expand(data: Bytes, spec: Tabs, initial = false, tab_byte = 9) -> Bytes {
  var column = 0
  var leading = true
  var held = 0
  var out: List[Bytes] = []
  var at = 0
  while at < data.len() {
    let unit = character(data, at)
    let value = data.byte_at(at) ?? 0
    if value == tab_byte or value == 9 {
      let stop = if value == 9 and tab_byte != 9 { column + 8 - column % 8 } else { next_stop(column, spec) }
      let width = if stop < 0 { 1 } else { stop - column }
      if leading or ! initial {
        out += [data[held..at], bytes.from_text(padding(width))]
        held = at + 1
      }
      column += width
    } else {
      if value == 10 { column = 0; leading = true } else if value == 8 { if column > 0 { column -= 1 } } else { column += unit.width; if value != 32 { leading = false } }
    }
    at += unit.size
  }
  bytes.concat([@out, data[held..]])
}

pure blank_size(data: Bytes, at: Int) -> Int {
  let byte = data.byte_at(at) ?? 0
  if byte == 32 or byte == 9 { return 1 }
  let unit = character(data, at)
  if unit.size > 1 and rx"[\x{a0}\x{1680}\x{2000}-\x{200a}\x{202f}\x{205f}\x{3000}]".matches(data[at..at + unit.size].utf8() ?? "") { unit.size } else { 0 }
}

## Compress blank runs while keeping isolated noninitial spaces unchanged.
export proc unexpand(data: Bytes, spec: Tabs, all: Bool) -> Bytes {
  var out: List[Bytes] = []
  var held = 0
  var at = 0
  var column = 0
  var leading = true
  while at < data.len() {
    let value = data.byte_at(at) ?? 0
    if blank_size(data, at) > 0 and (leading or all) {
      out += [data[held..at]]
      let start = at
      let origin = column
      while at < data.len() and blank_size(data, at) > 0 {
        if data.byte_at(at) == 9 {
          let next = next_stop(column, spec)
          if next < 0 { break }
          column = next
        } else { column += character(data, at).width }
        at += blank_size(data, at)
      }
      if at == start { out += [data[at..at + 1]]; at += 1; held = at; column += 1; continue }
      var pos = origin
      var blanks: List[Bytes] = []
      var converted = false
      while pos < column {
        let next = next_stop(pos, spec)
        var boundary = origin
        var cursor = start
        var aligned = false
        while cursor < at and boundary < next {
          let unit = character(data, cursor)
          boundary = if data.byte_at(cursor) == 9 { next_stop(boundary, spec) } else { boundary + unit.width }
          cursor += unit.size
          if boundary == next { aligned = true }
        }
        if next > pos and next <= column and aligned and (leading or at - start > 1 or value == 9) {
          blanks += [b"\t"]; pos = next; converted = true
        } else {
          blanks += [bytes.from_text(padding(column - pos))]; pos = column
        }
      }
      out += if converted { blanks } else { [data[start..at]] }
      held = at
    } else {
      let unit = character(data, at)
      at += unit.size
      if value == 10 { column = 0; leading = true } else if value == 8 { if column > 0 { column -= 1 } } else if value == 9 { let next = next_stop(column, spec); column = if next < 0 { column + 1 } else { next } } else { column += unit.width; if value != 32 { leading = false } }
    }
  }
  bytes.concat([@out, data[held..]])
}

## Flush buffered output and choose the final command status.
export proc finish(failed: Bool) -> Int {
  if let Err(failure) = io.flush_stdout() {
    if gnu.errno(failure) == 32 { return 141 }
    gnu.error(f"write error: {gnu.strerror(failure)}")
    return 1
  }
  if failed { 1 } else { 0 }
}
