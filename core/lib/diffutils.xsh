##! Byte input and normal-format rendering for native comparison operations.

error InputError = Unsupported(message: Str)

type LineRange = {start: Int, count: Int}

## Parse a nonnegative byte count with conventional binary size suffixes.
export pure byte_count(text: Str) -> Int? {
  var number = text
  var multiplier = 1
  for suffix in [{text: "kB", size: 1000}, {text: "MB", size: 1000000}, {text: "GB", size: 1000000000}, {text: "K", size: 1024}, {text: "M", size: 1048576}, {text: "G", size: 1073741824}] {
    if number.ends_with(suffix.text) { number = number.byte_slice(0, length: number.byte_len() - suffix.text.byte_len()); multiplier = suffix.size; break }
  }
  let parsed = number.parse_int()
  if let Ok(value) = parsed {
    if value < 0 or value > 9223372036854775807 / multiplier { return null }
    return value * multiplier
  }
  null
}

## Read bounded file chunks or accumulate stdin chunks until the requested
## size or EOF. Short pipe reads do not indicate end of input.
export proc read_chunk(name: Str, offset: Int, count: Int) [fs, io, error] -> Result[Bytes, Error] {
  if name != "-" {
    let source = fp"{name}"
    let metadata = fs.stat(source, follow_symlinks: true)?
    if metadata.kind != "file" { return Err(InputError.Unsupported(message: "comparison requires a regular file or stdin")) }
    let available = if offset < metadata.size { metadata.size - offset } else { 0 }
    return bytes.read_at(source, offset, if available < count { available } else { count })
  }
  var data = b""
  while data.len() < count {
    let next = io.stdin_read(count - data.len())?
    if next.len() == 0 { break }
    data = bytes.concat([data, next])
  }
  Ok(data)
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

## Count LF bytes; an unterminated segment is not a completed line.
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

## Format a byte as three octal digits.
export pure octal(value: Int) -> Str {
  f"{value / 64}{value / 8 % 8}{value % 8}"
}

## Right-align text with spaces to the requested byte width.
export pure pad(text: Str, width: Int) -> Str {
  var result = text
  while result.byte_len() < width { result = " " + result }
  result
}

pure line_range(text: Str) -> LineRange {
  let fields = text.split(",")
  {start: (fields.get(0) ?? "0").parse_int() ?? 0, count: (fields.get(1) ?? "1").parse_int() ?? 1}
}

pure range_text(item: LineRange) -> Str {
  if item.count <= 1 { f"{item.start}" } else { f"{item.start},{item.start + item.count - 1}" }
}

pure normal_hunk(old: LineRange, new: LineRange, removed: Str, added: Str) -> Str {
  if old.count == 0 { return f"{old.start}a{range_text(new)}\n{added}" }
  if new.count == 0 { return f"{range_text(old)}d{new.start}\n{removed}" }
  f"{range_text(old)}c{range_text(new)}\n{removed}---\n{added}"
}

## Translate a native zero-context unified patch to conventional normal diff.
export pure normal(text: Str) -> Str {
  var old: LineRange = {start: 0, count: 0}
  var new: LineRange = {start: 0, count: 0}
  var active = false
  var removed = ""
  var added = ""
  var previous = ""
  var output = ""
  for line in text.lines() {
    if line.starts_with("@@ ") {
      if active { output += normal_hunk(old, new, removed, added) }
      let words = line.words()
      old = line_range(words[1].byte_slice(1))
      new = line_range(words[2].byte_slice(1))
      removed = ""
      added = ""
      active = true
    } else if active and line.starts_with("-") { removed += "< " + line.byte_slice(1) + "\n"; previous = "-" } else if active and line.starts_with("+") { added += "> " + line.byte_slice(1) + "\n"; previous = "+" } else if active and line.starts_with("\\ No newline") {
      if previous == "-" { removed += line + "\n" } else { added += line + "\n" }
    }
  }
  if active { output += normal_hunk(old, new, removed, added) }
  output
}
