##! Byte input and normal-format rendering for native comparison operations.

error InputError = Unsupported(message: Str)

type LineRange = {start: Int, count: Int}

# A run of changed lines: old lines [old_start, old_end) are replaced by new
# lines [new_start, new_end). Equal lines between blocks are not stored.
type ChangeBlock = {old_start: Int, old_end: Int, new_start: Int, new_end: Int}

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

## Comparison key for `-b`: runs of whitespace are one space and trailing
## whitespace is dropped. A line that starts with whitespace keeps one leading
## space, so indentation still differs from none.
export pure space_key(line: Str) -> Str {
  let words = line.words()
  if words.is_empty() { return "" }
  let lead = if line.starts_with(words[0]) { "" } else { " " }
  lead + words.join(" ")
}

## A line is blank when it holds only whitespace.
export pure is_blank(line: Str) -> Bool {
  line.words().is_empty()
}

pure compare_key(line: Str, space: Bool) -> Str {
  if space { space_key(line) } else { line }
}

pure key_text(keys: List[Str]) -> Str {
  if keys.is_empty() { return "" }
  keys.join("\n") + "\n"
}

pure all_blank(lines: List[Str], start: Int, end: Int) -> Bool {
  for index in range(start, end) {
    if !is_blank(lines[index]) { return false }
  }
  true
}

# A hunk range: a zero-length range names the line before it, as unified diffs do.
pure hunk_range(first: Int, count: Int) -> Str {
  if count == 0 { return f"{first},0" }
  if count == 1 { return f"{first + 1}" }
  f"{first + 1},{count}"
}

# Render one line with its marker. A final line without a newline gets the
# standard marker line so the patch records the missing terminator.
pure line_text(lines: List[Str], index: Int, marker: Str, open_end: Bool) -> Str {
  let text = marker + lines[index] + "\n"
  if open_end and index == lines.len() - 1 { text + "\\ No newline at end of file\n" } else { text }
}

# Parse a full-context unified patch of comparison keys into change blocks.
# The first two lines are the `---` and `+++` headers, which begin with the
# same characters as removed and added lines, so they are skipped by position.
pure change_blocks(patch_text: Str) -> List[ChangeBlock] {
  var blocks: List[ChangeBlock] = []
  var old_index = 0
  var new_index = 0
  var open = false
  var open_old = 0
  var open_new = 0
  var position = 0
  for line in patch_text.lines() {
    position += 1
    if position <= 2 or line.starts_with("@@") or line.starts_with("\\") { continue }
    let marker = line.byte_slice(0, length: 1)
    if marker == " " {
      if open {
        blocks += [{old_start: open_old, old_end: old_index, new_start: open_new, new_end: new_index}]
        open = false
      }
      old_index += 1
      new_index += 1
      continue
    }
    if !open {
      open = true
      open_old = old_index
      open_new = new_index
    }
    if marker == "-" { old_index += 1 } else { new_index += 1 }
  }
  if open {
    blocks += [{old_start: open_old, old_end: old_index, new_start: open_new, new_end: new_index}]
  }
  blocks
}

# Render change blocks as unified hunks. Context lines always come from the old
# text. With `blank`, a group of blocks is dropped when every changed line in it
# is blank; a group that also holds a visible change prints all of its lines.
pure render_hunks(old_text: Str, new_text: Str, blocks: List[ChangeBlock], context: Int, blank: Bool) -> Str {
  let old: List[Str] = old_text.lines().collect()
  let new: List[Str] = new_text.lines().collect()
  let old_open = old_text.byte_len() > 0 and !old_text.ends_with("\n")
  let new_open = new_text.byte_len() > 0 and !new_text.ends_with("\n")
  var output = ""
  var group_start = 0
  while group_start < blocks.len() {
    var group_end = group_start
    while group_end + 1 < blocks.len() and blocks[group_end + 1].old_start - blocks[group_end].old_end <= 2 * context {
      group_end += 1
    }

    var visible = false
    for index in range(group_start, group_end + 1) {
      let block = blocks[index]
      if !blank or !all_blank(old, block.old_start, block.old_end) or !all_blank(new, block.new_start, block.new_end) { visible = true }
    }

    if visible {
      let first = blocks[group_start]
      let last = blocks[group_end]
      let gap_before = if group_start == 0 { first.old_start } else { first.old_start - blocks[group_start - 1].old_end }
      let gap_after = if group_end + 1 == blocks.len() { old.len() - last.old_end } else { blocks[group_end + 1].old_start - last.old_end }
      let lead = if gap_before < context { gap_before } else { context }
      let trail = if gap_after < context { gap_after } else { context }
      let old_first = first.old_start - lead
      let old_last = last.old_end + trail
      let new_first = first.new_start - lead
      let new_last = last.new_end + trail
      output += f"@@ -{hunk_range(old_first, old_last - old_first)} +{hunk_range(new_first, new_last - new_first)} @@\n"
      var cursor = old_first
      for index in range(group_start, group_end + 1) {
        let block = blocks[index]
        while cursor < block.old_start {
          output += line_text(old, cursor, " ", old_open)
          cursor += 1
        }
        for line_index in range(block.old_start, block.old_end) { output += line_text(old, line_index, "-", old_open) }
        for line_index in range(block.new_start, block.new_end) { output += line_text(new, line_index, "+", new_open) }
        cursor = block.old_end
      }
      while cursor < old_last {
        output += line_text(old, cursor, " ", old_open)
        cursor += 1
      }
    }
    group_start = group_end + 1
  }
  output
}

## Compare two texts line by line with `-b` (`space`) or `-B` (`blank`) rules and
## return the unified patch, or "" when no change is reportable. Changes are
## found by diffing comparison keys with full context, so only the rendering
## differs from a plain unified diff.
export proc unified_ignoring(old: Str, new: Str, context: Int, space: Bool, blank: Bool) [fs, error] -> Result[Str, Error] {
  let old_lines: List[Str] = old.lines().collect()
  let new_lines: List[Str] = new.lines().collect()
  var old_keys: List[Str] = []
  for line in old_lines { old_keys += [compare_key(line, space)] }
  var new_keys: List[Str] = []
  for line in new_lines { new_keys += [compare_key(line, space)] }

  let scratch = fs.tempdir()?
  defer scratch.close()
  let root = scratch.host_path()?
  scratch.write(p"original", key_text(old_keys))
  scratch.write(p"modified", key_text(new_keys))
  let key_patch = diff.unified(fp"{root}/original", fp"{root}/modified", context: old_lines.len() + new_lines.len() + 1)?

  let body = render_hunks(old, new, change_blocks(key_patch.text), context, blank)
  if body == "" { return Ok("") }
  Ok("--- original\n+++ modified\n" + body)
}
