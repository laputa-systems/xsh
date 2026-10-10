##! Reject-file text for the patch applet: failed hunks written back as
##! unified or context diff hunks, the way GNU patch writes them.

use patch_parse

# A unified range: the line count is omitted for one line, and an empty range
# names the line before it.
pure range_text(first: Int, count: Int) -> Str {
  if count == 1 { f"{first}" } else if count == 0 { f"{first - 1},0" } else { f"{first},{count}" }
}

# A body line as GNU patch writes it into a reject file: the line bytes
# exactly as read, so a line that had no terminator runs into the next one.
pure line_text(kind: Str, line: Bytes) -> Bytes {
  bytes.concat([bytes.from_text(kind), line])
}

## One failed hunk as unified-diff text.
export pure unified_reject(hunk: patch_parse.Hunk) -> Bytes {
  var parts: List[Bytes] = [bytes.from_text(f"@@ -{range_text(hunk.old_first, hunk.old_count)} +{range_text(hunk.new_first, hunk.new_count)} @@"), hunk.function, b"\n"]
  for index in range(hunk.kinds.len()) {
    let kind = hunk.kinds[index]
    parts += [line_text(if kind == 0 { " " } else if kind == 1 { "-" } else { "+" }, hunk.texts[index])]
  }
  bytes.concat(parts)
}

pure context_range(first: Int, count: Int) -> Str {
  if count == 0 { return f"{first - 1}" }
  if count == 1 { return f"{first}" }
  f"{first},{first + count - 1}"
}

## One failed hunk as context-diff text. A hunk read from a normal diff has no
## context lines and prints the bare ranges GNU patch prints for such hunks.
export pure context_reject(hunk: patch_parse.Hunk, from_normal: Bool) -> Bytes {
  var old_lines: List[Bytes] = []
  var new_lines: List[Bytes] = []
  let kinds = hunk.kinds
  # Within one change group, removed and added lines show as changed (`!`)
  # when the group has both, otherwise as `-` or `+`.
  var marks: List[Str] = []
  var index = 0
  while index < kinds.len() {
    if kinds[index] == 0 {
      marks += [" "]
      index += 1
      continue
    }
    var end = index
    var removed = 0
    var added = 0
    while end < kinds.len() and kinds[end] != 0 {
      if kinds[end] == 1 { removed += 1 } else { added += 1 }
      end += 1
    }
    for k in range(end - index) {
      if removed > 0 and added > 0 and !from_normal { marks += ["!"] } else if kinds[index + k] == 1 { marks += ["-"] } else { marks += ["+"] }
    }
    index = end
  }
  for at in range(kinds.len()) {
    let mark = marks[at]
    let text = hunk.texts[at]
    let note = b""
    if kinds[at] != 2 {
      old_lines += [bytes.concat([bytes.from_text(if mark == "!" { "! " } else if mark == "-" { "- " } else { "  " }), text, note])]
    }
    if kinds[at] != 1 {
      new_lines += [bytes.concat([bytes.from_text(if mark == "!" { "! " } else if mark == "+" { "+ " } else { "  " }), text, note])]
    }
  }
  var parts: List[Bytes] = [bytes.from_text("***************"), hunk.function, b"\n"]
  let old_range = if from_normal and hunk.old_count == 0 { "0" } else { context_range(hunk.old_first, hunk.old_count) }
  let new_range = if from_normal and hunk.new_count == 0 { "0" } else { context_range(hunk.new_first, hunk.new_count) }
  let old_tail = if from_normal { "" } else { " ****" }
  let new_tail = if from_normal { " -----" } else { " ----" }
  parts += [bytes.from_text(f"*** {old_range}{old_tail}\n")]
  parts += old_lines
  parts += [bytes.from_text(f"--- {new_range}{new_tail}\n")]
  parts += new_lines
  bytes.concat(parts)
}

