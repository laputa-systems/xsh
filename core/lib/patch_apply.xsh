##! Hunk placement for the patch applet.
##!
##! Finds where a hunk applies using GNU patch's search: the expected line is
##! tried first, then increasing distances (later lines before earlier ones at
##! the same distance), after dropping up to `fuzz` outer context lines.
##! Whitespace-insensitive matching compares canonical keys instead of raw
##! lines.

use patch_parse

## Canonical comparison forms of the input file lines, one per line.
export pure keys(lines: List[Bytes], loose_space: Bool) -> List[Bytes] {
  var output: List[Bytes] = []
  for line in lines { output += [line_key(line, loose_space)] }
  output
}

pure is_blank(byte: Int) -> Bool {
  byte == 32 or byte == 9 or byte == 11 or byte == 12 or byte == 13
}

## The comparison key of one line. With `loose_space`, runs of blanks collapse
## to one space and blanks at the end of the line are ignored.
export pure line_key(line: Bytes, loose_space: Bool) -> Bytes {
  var text = line
  if !loose_space { return text }
  let terminated = text.ends_with(b"\n")
  let content = if terminated { text[0..text.len() - 1] } else { text }
  var pieces: List[Int] = []
  var in_blanks = false
  for byte in content {
    if is_blank(byte) {
      in_blanks = true
    } else {
      if in_blanks { pieces += [32] }
      in_blanks = false
      pieces += [byte]
    }
  }
  let squeezed = bytes.from_ints(pieces) ?? b""
  if terminated { bytes.concat([squeezed, b"\n"]) } else { squeezed }
}

## The old-side lines of a hunk: its context and removed lines.
export pure pattern(hunk: patch_parse.Hunk) -> List[Bytes] {
  var output: List[Bytes] = []
  for index in range(hunk.kinds.len()) {
    if hunk.kinds[index] != 2 { output += [hunk.texts[index]] }
  }
  output
}

## The hunk applied in the other direction: removed and added lines swap, and
## so do the old and new line numbers and counts.
export pure reversed(hunk: patch_parse.Hunk) -> patch_parse.Hunk {
  var out = hunk
  # Within a change group removed lines come first, as in a hunk read
  # straight from a diff.
  var kinds: List[Int] = []
  var texts: List[Bytes] = []
  var at = 0
  let total = hunk.kinds.len()
  while at < total {
    if hunk.kinds[at] == 0 {
      kinds += [0]
      texts += [hunk.texts[at]]
      at += 1
      continue
    }
    var end = at
    while end < total and hunk.kinds[end] != 0 { end += 1 }
    for index in range(at, end) {
      if hunk.kinds[index] == 2 { kinds += [1]; texts += [hunk.texts[index]] }
    }
    for index in range(at, end) {
      if hunk.kinds[index] == 1 { kinds += [2]; texts += [hunk.texts[index]] }
    }
    at = end
  }
  out.kinds = kinds
  out.texts = texts
  out.old_first = hunk.new_first
  out.old_count = hunk.new_count
  out.new_first = hunk.old_first
  out.new_count = hunk.old_count
  out.old_start = hunk.new_start
  out.new_start = hunk.old_start
  out
}

pure matches_at(input: List[Bytes], pattern: List[Bytes], base: Int, prefix_fuzz: Int, suffix_fuzz: Int) -> Bool {
  let lines = pattern.len() - suffix_fuzz
  var p = prefix_fuzz
  var i = base + prefix_fuzz
  let total = input.len()
  while p < lines {
    if i < 1 or i > total { return false }
    if input[i - 1] != pattern[p] { return false }
    p += 1
    i += 1
  }
  true
}

## Where `hunk` applies at the given fuzz, or 0. `input` and `pattern` are
## canonical keys; `in_offset` is the offset of the previous placed hunk and
## `last_frozen` the last input line already consumed.
export pure locate(input: List[Bytes], pattern: List[Bytes], hunk: patch_parse.Hunk, fuzz: Int, in_offset: Int, last_frozen: Int) -> Int {
  let first_guess = hunk.old_first + in_offset
  let pat_lines = pattern.len()
  if pat_lines == 0 { return first_guess }
  let context = if hunk.prefix < hunk.suffix { hunk.suffix } else { hunk.prefix }
  let prefix_fuzz = fuzz + hunk.prefix - context
  let suffix_fuzz = fuzz + hunk.suffix - context
  let input_lines = input.len()
  # Negative fuzz only anchors the hunk to an end of the file; the window of
  # lines it may occupy is computed as if nothing were skipped.
  let prefix_skip = if prefix_fuzz < 0 { 0 } else { prefix_fuzz }
  let suffix_skip = if suffix_fuzz < 0 { 0 } else { suffix_fuzz }
  let max_where = input_lines - (pat_lines - suffix_skip) + 1
  let min_where = last_frozen + 1 - (hunk.prefix - prefix_skip)
  let max_pos_offset = max_where - first_guess
  var max_neg_offset = first_guess - min_where
  let max_offset = if max_pos_offset < max_neg_offset { max_neg_offset } else { max_pos_offset }
  if first_guess <= max_neg_offset { max_neg_offset = first_guess - 1 }
  if prefix_fuzz < 0 {
    # Fewer leading than trailing context: the hunk starts the file.
    let offset = 1 - first_guess
    if offset > max_pos_offset or -offset > max_neg_offset { return 0 }
    if matches_at(input, pattern, first_guess + offset, 0, suffix_fuzz) { return first_guess + offset }
    return 0
  }
  if suffix_fuzz < 0 {
    # Fewer trailing than leading context: the hunk ends the file.
    let offset = input_lines - pat_lines + 1 - first_guess
    if offset > max_pos_offset or -offset > max_neg_offset { return 0 }
    if matches_at(input, pattern, first_guess + offset, prefix_fuzz, 0) { return first_guess + offset }
    return 0
  }
  var offset = 0
  while offset <= max_offset {
    if offset <= max_pos_offset and matches_at(input, pattern, first_guess + offset, prefix_fuzz, suffix_fuzz) { return first_guess + offset }
    if offset > 0 and offset <= max_neg_offset and matches_at(input, pattern, first_guess - offset, prefix_fuzz, suffix_fuzz) { return first_guess - offset }
    offset += 1
  }
  0
}
