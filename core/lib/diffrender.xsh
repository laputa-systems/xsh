##! GNU diff output formats over an edit script: normal, unified, context, ed,
##! RCS and side by side, with the hunk grouping, function headings, tab and
##! blank handling that make the bytes match.

use gnudiff

## How a script is printed. `style` is one of normal, unified, context, ed, rcs
## or sdiff. `function_patterns` are the regular expressions of `-p`/`-F`; a
## line matching any of them is a function heading, and none prints no heading. The ignore fields decide which changes are
## trivial: `ignore_blank` is `-B`, `blank_skips_space` says a blank line may
## hold white space (only with `-b` or `-w`), and `patterns` are the `-I`
## expressions. `palette` holds the colour sequences for reset, header, added,
## deleted and line-number text, in that order, or is empty for plain output.
export type Format = {utf8: Bool, style: Str, context: Int, initial_tab: Bool, expand_tabs: Bool, tabsize: Int, suppress_blank_empty: Bool, width: Int, left_column: Bool, suppress_common: Bool, function_patterns: List[Str], ignore_blank: Bool, blank_skips_space: Bool, patterns: List[Str], palette: List[Str]}

# The kinds of change a hunk holds, as GNU numbers them.
const OLD = 1
const NEW = 2
const CHANGED = 3

# A hunk's extent in both files and what it changes. `changes` is zero when
# every changed line is trivial, which suppresses the hunk.
type Hunk = {first0: Int, last0: Int, first1: Int, last1: Int, changes: Int}

# Heading state kept across hunks: where the last search started and the last
# heading found.
type Heading = {search_from: Int, line: Int}

# The heading text of a hunk and the state to carry to the next one.
type Title = {text: Bytes, state: Heading}

# Output bytes and the column they end at.
type Piece = {text: Bytes, column: Int}

# The room of one side-by-side half and where the second half starts.
type Layout = {half: Int, second: Int}

# Whether one line is trivially ignorable: blank under `-B`, or matching an
# `-I` expression.
pure trivial_line(line: Bytes, fmt: Format) -> Bool {
  if fmt.ignore_blank {
    if line.is_empty() { return true }
    if fmt.blank_skips_space {
      var blank = true
      for index in range(line.len()) {
        if !gnudiff.is_space(line.byte_at(index) ?? 0) { blank = false; break }
      }
      if blank { return true }
    }
  }
  for pattern in fmt.patterns {
    let found = regex.find_bytes(pattern, line)
    if found is Ok(_) and (found ?? []).len() > 0 { return true }
  }
  false
}

pure analyze(chain: List[gnudiff.Change], first: gnudiff.Source, second: gnudiff.Source, fmt: Format) -> Hunk {
  let check = fmt.ignore_blank or !fmt.patterns.is_empty()
  var trivial = check
  var from = 0
  var to = 0
  var l0 = 0
  var l1 = 0
  for item in chain {
    l0 = item.line0 + item.deleted - 1
    l1 = item.line1 + item.inserted - 1
    from += item.deleted
    to += item.inserted
    if trivial {
      for index in range(item.line0, l0 + 1) {
        if !trivial_line(first.lines[index], fmt) { trivial = false; break }
      }
    }
    if trivial {
      for index in range(item.line1, l1 + 1) {
        if !trivial_line(second.lines[index], fmt) { trivial = false; break }
      }
    }
  }
  let kind = if trivial { 0 } else { (if from > 0 { OLD } else { 0 }) + (if to > 0 { NEW } else { 0 }) }
  {first0: chain[0].line0, last0: l0, first1: chain[0].line1, last1: l1, changes: kind}
}

## Whether any change in the script is worth reporting (not blank or matching
## an ignore expression), which decides the exit status.
export pure significant(first: gnudiff.Source, second: gnudiff.Source, script: List[gnudiff.Change], fmt: Format) -> Bool {
  for item in script {
    if analyze([item], first, second, fmt).changes != 0 { return true }
  }
  false
}

# Whether line `index` of `source` carries a newline terminator.
pure terminated(source: gnudiff.Source, index: Int) -> Bool {
  !(source.open and index == source.lines.len() - 1)
}

# Tab expansion of one output line: tabs become spaces to the next stop, a
# carriage return restarts the column and, when a flag is shown, repeats it.
pure expand(line: Bytes, tabsize: Int, repeat_flag: Bytes?, utf8: Bool) -> Bytes {
  var chunks: List[Bytes] = []
  var column = 0
  var start = 0
  let size = line.len()
  var index = 0
  while index < size {
    let value = line.byte_at(index) ?? 0
    if value == 9 {
      chunks += [line[start..index]]
      let pad = tabsize - column % tabsize
      for _ in range(pad) { chunks += [b" "] }
      column += pad
      start = index + 1
      index += 1
    } else if value == 13 {
      chunks += [line[start..index + 1]]
      start = index + 1
      column = 0
      if let flag = repeat_flag {
        if index + 1 < size { chunks += [flag] }
      }
      index += 1
    } else if value == 8 {
      if column == 0 {
        chunks += [line[start..index]]
        start = index + 1
      } else {
        column -= 1
      }
      index += 1
    } else {
      let unit = gnudiff.unit_at(line, index, utf8)
      column += unit.columns
      index += unit.bytes
    }
  }
  chunks += [line[start..size]]
  bytes.concat(chunks)
}

## Wrap text in the colour of palette slot `slot` (1 header, 2 added, 3
# deleted, 4 line numbers); plain when colour is off or the slot is 0.
export pure paint(fmt: Format, slot: Int, text: Bytes) -> Bytes {
  if fmt.palette.is_empty() or slot == 0 { return text }
  bytes.concat([b"\x1b[", bytes.from_text(fmt.palette[slot]), b"m", text, b"\x1b[", bytes.from_text(fmt.palette[0]), b"m"])
}

# One output line with its flag: `flag` is "" for none. A flag is followed by a
# space (nothing in unified output), or a tab with `-T`; an empty line with
# `--suppress-blank-empty` keeps only the flag, and nothing for a blank flag.
# `slot` is the palette slot the line text is painted with (0 for none).
pure render_line(flag: Str, line: Bytes, open: Bool, fmt: Format, slot: Int) -> Bytes {
  var prefix = b""
  var repeat: Bytes? = null
  if flag != "" {
    let sep = if fmt.initial_tab { "\t" } else if fmt.style == "unified" { "" } else { " " }
    if fmt.suppress_blank_empty and line.is_empty() {
      prefix = bytes.from_text(flag.trim())
    } else {
      prefix = bytes.from_text(flag + sep)
      repeat = prefix
    }
  }
  let body = if fmt.expand_tabs { expand(line, fmt.tabsize, repeat, fmt.utf8) } else { line }
  let tail = if fmt.style == "rcs" and open { b"" } else if open { b"\n\\ No newline at end of file\n" } else { b"\n" }
  bytes.concat([paint(fmt, slot, bytes.concat([prefix, body])), tail])
}

# `translate_range` for a first/last pair of zero-based line indexes: one-based
# numbers, with the empty range naming the line before it.
pure range_pair(first: Int, last: Int) -> Str {
  let a = first + 1
  let b = last + 1
  if b > a { f"{a},{b}" } else { f"{b}" }
}

pure unified_range(first: Int, last: Int) -> Str {
  let a = first + 1
  let b = last + 1
  if b <= a {
    if b == a { f"{b}" } else { f"{b},0" }
  } else {
    f"{a},{b - a + 1}"
  }
}

# The nearest line before `from` that matches a heading pattern, searching back
# only as far as where the previous search began; when none is found the
# previous heading is reused.
pure function_heading(first: gnudiff.Source, from: Int, floor: Int, last_match: Int, patterns: List[Str]) -> Heading {
  var index = from - 1
  while index >= floor {
    for pattern in patterns {
      let found = regex.find_bytes(pattern, first.lines[index])
      if found is Ok(_) and (found ?? []).len() > 0 {
        return {line: index, search_from: from}
      }
    }
    index -= 1
  }
  {line: last_match, search_from: from}
}

# The hunk groups of a script when context joins nearby changes. A change
# joins the one before it when fewer than `2 * context + 1` lines separate
# them, or fewer than `context` when the later change is ignorable.
pure hunk_groups(script: List[gnudiff.Change], flags: List[Bool], context: Int) -> List[List[Int]] {
  var groups: List[List[Int]] = []
  var index = 0
  let count = script.len()
  while index < count {
    var members: List[Int] = [index]
    var prev = index
    while true {
      let next = prev + 1
      if next >= count { break }
      let top0 = script[prev].line0 + script[prev].deleted
      let threshold = if flags[next] { context } else { 2 * context + 1 }
      if script[next].line0 - top0 < threshold {
        members += [next]
        prev = next
      } else {
        break
      }
    }
    groups += [members]
    index = prev + 1
  }
  groups
}

# Flags saying which changes of the script are individually ignorable.
pure ignorable(first: gnudiff.Source, second: gnudiff.Source, script: List[gnudiff.Change], fmt: Format) -> List[Bool] {
  var flags: List[Bool] = []
  for item in script { flags += [analyze([item], first, second, fmt).changes == 0] }
  flags
}

# The heading text of a hunk starting at line `at`: the matched line cut to 40
# bytes without trailing white space.
pure heading_text(first: gnudiff.Source, at: Int, state: Heading, fmt: Format) -> Title {
  var text = b""
  var next = state
  if !fmt.function_patterns.is_empty() {
    let found = function_heading(first, at, state.search_from, state.line, fmt.function_patterns)
    next = {search_from: found.search_from, line: found.line}
    if found.line >= 0 {
      var raw = first.lines[found.line]
      var size = raw.len()
      if size > 40 { size = 40 }
      raw = raw[0..size]
      while size > 0 and gnudiff.is_space(raw.byte_at(size - 1) ?? 0) { size -= 1 }
      text = raw[0..size]
    }
  }
  {text: text, state: next}
}

# Print a script as unified or context hunks. `header` is written before the
# first hunk that is actually printed.
pure render_context(first: gnudiff.Source, second: gnudiff.Source, script: List[gnudiff.Change], fmt: Format, header: Bytes, unified: Bool) -> Bytes {
  var out: List[Bytes] = []
  var wrote_header = false
  let flags = ignorable(first, second, script, fmt)
  var state: Heading = {search_from: 0, line: -1}
  let n0 = first.lines.len()
  let n1 = second.lines.len()
  for members in hunk_groups(script, flags, fmt.context) {
    var chain: List[gnudiff.Change] = []
    for member in members { chain += [script[member]] }
    let hunk = analyze(chain, first, second, fmt)
    if hunk.changes == 0 { continue }
    var first0 = hunk.first0 - fmt.context
    if first0 < 0 { first0 = 0 }
    var first1 = hunk.first1 - fmt.context
    if first1 < 0 { first1 = 0 }
    let last0 = if hunk.last0 < n0 - fmt.context { hunk.last0 + fmt.context } else { n0 - 1 }
    let last1 = if hunk.last1 < n1 - fmt.context { hunk.last1 + fmt.context } else { n1 - 1 }
    let title = heading_text(first, first0, state, fmt)
    state = title.state
    if !wrote_header {
      out += [header]
      wrote_header = true
    }
    if unified {
      let line = f"@@ -{unified_range(first0, last0)} +{unified_range(first1, last1)} @@"
      out += [paint(fmt, 4, bytes.from_text(line))]
      if !title.text.is_empty() { out += [b" ", title.text] }
      out += [b"\n"]
      var cursor = 0
      var i = first0
      var j = first1
      while i <= last0 or j <= last1 {
        if cursor >= chain.len() or i < chain[cursor].line0 {
          let blank = fmt.suppress_blank_empty and first.lines[i].is_empty()
          if !blank { out += [if fmt.initial_tab { b"\t" } else { b" " }] }
          out += [render_line("", first.lines[i], !terminated(first, i), fmt, 0)]
          i += 1
          j += 1
        } else {
          let item = chain[cursor]
          for _ in range(item.deleted) {
            out += [render_line("-", first.lines[i], !terminated(first, i), fmt, 3)]
            i += 1
          }
          for _ in range(item.inserted) {
            out += [render_line("+", second.lines[j], !terminated(second, j), fmt, 2)]
            j += 1
          }
          cursor += 1
        }
      }
    } else {
      out += [b"***************"]
      if !title.text.is_empty() { out += [b" ", title.text] }
      out += [b"\n", paint(fmt, 4, bytes.from_text(f"*** {range_pair(first0, last0)} ****")), b"\n"]
      if hunk.changes.bit_and(OLD) != 0 {
        var cursor = 0
        for i in range(first0, last0 + 1) {
          while cursor < chain.len() and chain[cursor].line0 + chain[cursor].deleted <= i { cursor += 1 }
          var flag = " "
          if cursor < chain.len() and chain[cursor].line0 <= i {
            flag = if chain[cursor].inserted > 0 { "!" } else { "-" }
          }
          out += [render_line(flag, first.lines[i], !terminated(first, i), fmt, 3)]
        }
      }
      out += [paint(fmt, 4, bytes.from_text(f"--- {range_pair(first1, last1)} ----")), b"\n"]
      if hunk.changes.bit_and(NEW) != 0 {
        var cursor = 0
        for i in range(first1, last1 + 1) {
          while cursor < chain.len() and chain[cursor].line1 + chain[cursor].inserted <= i { cursor += 1 }
          var flag = " "
          if cursor < chain.len() and chain[cursor].line1 <= i {
            flag = if chain[cursor].deleted > 0 { "!" } else { "+" }
          }
          out += [render_line(flag, second.lines[i], !terminated(second, i), fmt, 2)]
        }
      }
    }
  }
  bytes.concat(out)
}

# Normal, ed and RCS formats: each change is printed on its own.
pure render_simple(first: gnudiff.Source, second: gnudiff.Source, script: List[gnudiff.Change], fmt: Format) -> Bytes {
  var out: List[Bytes] = []
  var order: List[Int] = []
  if fmt.style == "ed" {
    var at = script.len() - 1
    while at >= 0 {
      order += [at]
      at -= 1
    }
  } else {
    for index in range(script.len()) { order += [index] }
  }
  for index in order {
    let hunk = analyze([script[index]], first, second, fmt)
    if hunk.changes == 0 { continue }
    if fmt.style == "normal" {
      let letter = if hunk.changes == OLD { "d" } else if hunk.changes == NEW { "a" } else { "c" }
      out += [paint(fmt, 4, bytes.from_text(f"{range_pair(hunk.first0, hunk.last0)}{letter}{range_pair(hunk.first1, hunk.last1)}")), b"\n"]
      if hunk.changes.bit_and(OLD) != 0 {
        for i in range(hunk.first0, hunk.last0 + 1) { out += [render_line("<", first.lines[i], !terminated(first, i), fmt, 3)] }
      }
      if hunk.changes == CHANGED { out += [b"---\n"] }
      if hunk.changes.bit_and(NEW) != 0 {
        for i in range(hunk.first1, hunk.last1 + 1) { out += [render_line(">", second.lines[i], !terminated(second, i), fmt, 2)] }
      }
    } else if fmt.style == "ed" {
      let letter = if hunk.changes == OLD { "d" } else if hunk.changes == NEW { "a" } else { "c" }
      out += [bytes.from_text(f"{range_pair(hunk.first0, hunk.last0)}{letter}\n")]
      if hunk.changes != OLD {
        var insert_mode = true
        for i in range(hunk.first1, hunk.last1 + 1) {
          if !insert_mode {
            out += [b"a\n"]
            insert_mode = true
          }
          if second.lines[i] == b"." and terminated(second, i) {
            out += [b"..\n.\ns/.//\n"]
            insert_mode = false
          } else {
            out += [render_line("", second.lines[i], !terminated(second, i), fmt, 0)]
          }
        }
        if insert_mode { out += [b".\n"] }
      }
    } else {
      let from0 = hunk.first0 + 1
      let to0 = hunk.last0 + 1
      if hunk.changes.bit_and(OLD) != 0 {
        out += [bytes.from_text(f"d{from0} {to0 - from0 + 1}\n")]
      }
      if hunk.changes.bit_and(NEW) != 0 {
        out += [bytes.from_text(f"a{to0} {hunk.last1 - hunk.first1 + 1}\n")]
        for i in range(hunk.first1, hunk.last1 + 1) { out += [render_line("", second.lines[i], !terminated(second, i), fmt, 0)] }
      }
    }
  }
  bytes.concat(out)
}

# Pad from column `from` to column `to` with tabs (unless tabs are expanded)
# and then spaces. Returns the target column even when `from` is past it.
pure tab_from_to(from: Int, to: Int, fmt: Format) -> Piece {
  var chunks: List[Bytes] = []
  var at = from
  if !fmt.expand_tabs {
    var stop = from + fmt.tabsize - from % fmt.tabsize
    while stop <= to {
      chunks += [b"\t"]
      at = stop
      stop += fmt.tabsize
    }
  }
  while at < to {
    chunks += [b" "]
    at += 1
  }
  {text: bytes.concat(chunks), column: to}
}

# One half of a side-by-side line, cut to `bound` columns. Returns the bytes and
# the column the output ends at.
pure half_line(line: Bytes, indent: Int, bound: Int, fmt: Format) -> Piece {
  var chunks: List[Bytes] = []
  var in_position = 0
  var out_position = 0
  var index = 0
  let size = line.len()
  while index < size {
    let value = line.byte_at(index) ?? 0
    if value == 9 {
      let spaces = fmt.tabsize - in_position % fmt.tabsize
      if in_position == out_position {
        var stop = out_position + spaces
        if fmt.expand_tabs {
          if bound < stop { stop = bound }
          while out_position < stop {
            chunks += [b" "]
            out_position += 1
          }
        } else if stop < bound {
          out_position = stop
          chunks += [b"\t"]
        }
      }
      in_position += spaces
      index += 1
    } else if value == 13 {
      chunks += [b"\r"]
      let pad = tab_from_to(0, indent, fmt)
      chunks += [pad.text]
      in_position = 0
      out_position = 0
      index += 1
    } else if value == 8 {
      if in_position != 0 {
        let matched = in_position == out_position
        in_position -= 1
        if matched {
          out_position -= 1
          chunks += [b"\x08"]
        }
      }
      index += 1
    } else {
      let unit = gnudiff.unit_at(line, index, fmt.utf8)
      if in_position + unit.columns <= bound {
        chunks += [line[index..index + unit.bytes]]
        out_position = in_position + unit.columns
      }
      in_position += unit.columns
      index += unit.bytes
    }
  }
  {text: bytes.concat(chunks), column: out_position}
}

# The widths of a side-by-side layout: the room of one half and the column the
# second half starts at. With tabs the second half is moved to a tab stop.
pure sdiff_layout(fmt: Format) -> Layout {
  let width = fmt.width
  let base = if width > 3 { (width - 3) / 2 } else { 0 }
  if base == 0 { return {half: 0, second: width} }
  if fmt.expand_tabs { return {half: base, second: width - base} }
  var offset = fmt.tabsize * ((width + 3 + fmt.tabsize) / (2 * fmt.tabsize))
  if offset > width { offset = width }
  var half = if offset - 3 < width - offset { offset - 3 } else { width - offset }
  if half < 0 { half = 0 }
  if half == 0 { offset = width }
  {half: half, second: offset}
}

pure sdiff_line(left: Bytes?, left_open: Bool, mark: Str, right: Bytes?, right_open: Bool, fmt: Format) -> Bytes {
  let layout = sdiff_layout(fmt)
  let half = layout.half
  let second = layout.second
  var chunks: List[Bytes] = []
  var column = 0
  var newline = false
  var sep = mark
  if let text = left {
    newline = newline or !left_open
    let piece = half_line(text, 0, half, fmt)
    chunks += [piece.text]
    column = piece.column
  }
  if sep != " " {
    let target = (half + second - 1) / 2
    let pad = tab_from_to(column, target, fmt)
    chunks += [pad.text]
    column = pad.column + 1
    if sep == "|" and newline == right_open {
      sep = if newline { "/" } else { "\\" }
    }
    chunks += [bytes.from_text(sep)]
  }
  if let text = right {
    newline = newline or !right_open
    if !text.is_empty() {
      let pad = tab_from_to(column, second, fmt)
      chunks += [pad.text]
      let piece = half_line(text, second, half, fmt)
      chunks += [piece.text]
    }
  }
  let slot = if mark == "<" { 3 } else if mark == ">" { 2 } else { 0 }
  if slot == 0 or fmt.palette.is_empty() {
    if newline { chunks += [b"\n"] }
    return bytes.concat(chunks)
  }
  # A coloured side-by-side line is reset only after its newline.
  let tail = if newline { b"\n" } else { b"" }
  bytes.concat([b"\x1b[", bytes.from_text(fmt.palette[slot]), b"m", bytes.concat(chunks), tail, b"\x1b[", bytes.from_text(fmt.palette[0]), b"m"])
}

# Side-by-side output: common lines, then each hunk's paired and unpaired lines.
pure render_sdiff(first: gnudiff.Source, second: gnudiff.Source, script: List[gnudiff.Change], fmt: Format) -> Bytes {
  var out: List[Bytes] = []
  var next0 = 0
  var next1 = 0
  let n0 = first.lines.len()
  let n1 = second.lines.len()
  var cursor = 0
  while cursor <= script.len() {
    var limit0 = n0
    var limit1 = n1
    var hunk: Hunk = {first0: 0, last0: -1, first1: 0, last1: -1, changes: 0}
    if cursor < script.len() {
      hunk = analyze([script[cursor]], first, second, fmt)
      if hunk.changes == 0 {
        cursor += 1
        continue
      }
      limit0 = hunk.first0
      limit1 = hunk.first1
    }
    if !fmt.suppress_common and (next0 != limit0 or next1 != limit1) {
      var i0 = next0
      var i1 = next1
      while i0 < limit0 and i1 < limit1 {
        if fmt.left_column {
          out += [sdiff_line(first.lines[i0], !terminated(first, i0), "(", null, false, fmt)]
        } else {
          out += [sdiff_line(first.lines[i0], !terminated(first, i0), " ", second.lines[i1], !terminated(second, i1), fmt)]
        }
        i0 += 1
        i1 += 1
      }
      while i1 < limit1 {
        out += [sdiff_line(null, false, ")", second.lines[i1], !terminated(second, i1), fmt)]
        i1 += 1
      }
      while i0 < limit0 {
        out += [sdiff_line(first.lines[i0], !terminated(first, i0), "(", null, false, fmt)]
        i0 += 1
      }
    }
    next0 = limit0
    next1 = limit1
    if cursor >= script.len() { break }
    var changes = hunk.changes
    var i = hunk.first0
    var j = hunk.first1
    if changes == CHANGED {
      while i <= hunk.last0 and j <= hunk.last1 {
        out += [sdiff_line(first.lines[i], !terminated(first, i), "|", second.lines[j], !terminated(second, j), fmt)]
        i += 1
        j += 1
      }
      changes = (if i <= hunk.last0 { OLD } else { 0 }) + (if j <= hunk.last1 { NEW } else { 0 })
      next0 = i
      next1 = j
    }
    if changes.bit_and(NEW) != 0 {
      while j <= hunk.last1 {
        out += [sdiff_line(null, false, ">", second.lines[j], !terminated(second, j), fmt)]
        j += 1
      }
      next1 = j
    }
    if changes.bit_and(OLD) != 0 {
      while i <= hunk.last0 {
        out += [sdiff_line(first.lines[i], !terminated(first, i), "<", null, false, fmt)]
        i += 1
      }
      next0 = i
    }
    cursor += 1
  }
  bytes.concat(out)
}

## The templates of merged-file output: the four group formats (old, new,
## changed, unchanged) and the three line formats (old, new, unchanged).
export type Templates = {old_group: Str, new_group: Str, changed_group: Str, unchanged_group: Str, old_line: Str, new_line: Str, unchanged_line: Str}

# What a group format can refer to: the lines of the group in each file.
type Span = {first: gnudiff.Source, second: gnudiff.Source, beg0: Int, end0: Int, beg1: Int, end1: Int, templates: Templates}

# Output text and where scanning stopped.
type Scan = {text: Bytes, at: Int}

# A run of digits: its value, where it ended and whether any were present.
type Digits = {value: Int, at: Int, seen: Bool}

# A character literal of a format: its byte and where it ended.
type Literal = {value: Int, at: Int}

pure is_digit(value: Int) -> Bool {
  value >= 48 and value <= 57
}

pure pad_to(text: Str, width: Int, left: Bool, zero: Bool) -> Str {
  var out = text
  if out.byte_len() >= width { return out }
  let fill = width - out.byte_len()
  var padding = ""
  for _ in range(fill) { padding += if zero and !left { "0" } else { " " } }
  if left { out + padding } else if zero and out.starts_with("-") { "-" + padding + out.byte_slice(1) } else { padding + out }
}

# printf-style rendering of an integer with the conversion `d`, `o`, `x` or `X`.
pure integer_text(value: Int, conversion: Str, left: Bool, zero: Bool, width: Int, precision: Int) -> Str {
  var digits = ""
  var rest = if value < 0 { -value } else { value }
  let base = if conversion == "o" { 8 } else if conversion == "d" { 10 } else { 16 }
  let table = if conversion == "X" { "0123456789ABCDEF" } else { "0123456789abcdef" }
  if rest == 0 { digits = "0" }
  while rest > 0 {
    digits = table.byte_slice(rest % base, length: 1) + digits
    rest = rest / base
  }
  if precision >= 0 {
    while digits.byte_len() < precision { digits = "0" + digits }
    if precision == 0 and value == 0 { digits = "" }
  }
  let signed = if value < 0 and conversion == "d" { "-" + digits } else { digits }
  pad_to(signed, width, left, zero and precision < 0)
}

# The value of a group letter: lower case describes the first file's lines of
# the group, upper case the second file's.
pure group_value(letter: Int, span: Span) -> Int? {
  let upper = letter >= 65 and letter <= 90
  let kind = if upper { letter + 32 } else { letter }
  let beg = if upper { span.beg1 } else { span.beg0 }
  let end = if upper { span.end1 } else { span.end0 }
  if kind == 102 { return beg + 1 }
  if kind == 108 { return end }
  if kind == 110 { return end - beg }
  if kind == 101 { return beg }
  if kind == 109 { return end + 1 }
  null
}

# Parse an optional run of digits starting at `at`.
pure number_at(raw: Bytes, at: Int) -> Digits {
  var cursor = at
  var value = 0
  var seen = false
  while cursor < raw.len() and is_digit(raw.byte_at(cursor) ?? 0) {
    value = value * 10 + ((raw.byte_at(cursor) ?? 48) - 48)
    cursor += 1
    seen = true
  }
  {value: value, at: cursor, seen: seen}
}

# A `%c'C'` or `%c'\OOO'` character at `at` (just after the `c`), as the byte
# and the position after the closing quote; null when malformed.
pure char_spec(raw: Bytes, at: Int) -> Literal? {
  if raw.byte_at(at) != 39 { return null }
  let first = raw.byte_at(at + 1)
  if first == null { return null }
  if first == 92 {
    var cursor = at + 2
    var value = 0
    var count = 0
    while count < 3 and cursor < raw.len() {
      let digit = raw.byte_at(cursor) ?? 0
      if digit < 48 or digit > 55 { break }
      value = value * 8 + (digit - 48)
      cursor += 1
      count += 1
    }
    if count == 0 or raw.byte_at(cursor) != 39 { return null }
    return {value: value % 256, at: cursor + 1}
  }
  if raw.byte_at(at + 2) != 39 { return null }
  {value: first ?? 0, at: at + 3}
}

# Scan a group format from `at` until `stop` (a byte, or -1 for the end of the
# text), writing to the result only when `emit` is true. Unknown or malformed
# directives are copied through as they were written.
pure group_scan(raw: Bytes, start: Int, stop: Int, emit: Bool, span: Span) -> Scan {
  var out: List[Bytes] = []
  var at = start
  let size = raw.len()
  while at < size {
    let value = raw.byte_at(at) ?? 0
    if value == stop { break }
    if value != 37 {
      if emit { out += [raw[at..at + 1]] }
      at += 1
      continue
    }
    let after = raw.byte_at(at + 1)
    if after == null {
      if emit { out += [b"%"] }
      at += 1
      continue
    }
    let letter = after ?? 0
    if letter == 37 {
      if emit { out += [b"%"] }
      at += 2
    } else if letter == 60 or letter == 62 or letter == 61 {
      if emit {
        let source = if letter == 62 { span.second } else { span.first }
        let layout_text = if letter == 60 { span.templates.old_line } else if letter == 62 { span.templates.new_line } else { span.templates.unchanged_line }
        let from = if letter == 62 { span.beg1 } else { span.beg0 }
        let to = if letter == 62 { span.end1 } else { span.end0 }
        out += [group_lines(layout_text, source, from, to)]
      }
      at += 2
    } else if letter == 99 {
      let spec = char_spec(raw, at + 2)
      if spec == null {
        if emit { out += [b"%"] }
        at += 1
      } else {
        let found = spec ?? {value: 0, at: at}
        if emit { out += [bytes.from_ints([found.value]) ?? b""] }
        at = found.at
      }
    } else if letter == 40 {
      var cursor = at + 2
      var values: List[Int] = []
      var well_formed = true
      for side in range(2) {
        let number = number_at(raw, cursor)
        if number.seen {
          values += [number.value]
          cursor = number.at
        } else {
          let named = group_value(raw.byte_at(cursor) ?? 0, span)
          if named == null {
            well_formed = false
            break
          }
          values += [named ?? 0]
          cursor += 1
        }
        let expected = if side == 0 { 61 } else { 63 }
        if raw.byte_at(cursor) != expected {
          well_formed = false
          break
        }
        cursor += 1
      }
      if !well_formed {
        if emit { out += [b"%"] }
        at += 1
      } else {
        let chosen = values[0] == values[1]
        let then_part = group_scan(raw, cursor, 58, emit and chosen, span)
        out += [then_part.text]
        cursor = then_part.at
        if cursor < size {
          let else_part = group_scan(raw, cursor + 1, 41, emit and !chosen, span)
          out += [else_part.text]
          cursor = else_part.at
          if cursor < size { cursor += 1 }
        }
        at = cursor
      }
    } else {
      var cursor = at + 1
      var left = false
      var zero = false
      while cursor < size {
        let flag = raw.byte_at(cursor) ?? 0
        if flag == 45 {
          left = true
        } else if flag == 48 {
          zero = true
        } else {
          break
        }
        cursor += 1
      }
      let width = number_at(raw, cursor)
      cursor = width.at
      var precision = -1
      if raw.byte_at(cursor) == 46 {
        let digits = number_at(raw, cursor + 1)
        precision = digits.value
        cursor = digits.at
      }
      let conversion = raw.byte_at(cursor) ?? 0
      let named = group_value(raw.byte_at(cursor + 1) ?? 0, span)
      if (conversion == 100 or conversion == 111 or conversion == 120 or conversion == 88) and named != null {
        if emit {
          let kind = raw[cursor..cursor + 1].utf8() ?? "d"
          out += [bytes.from_text(integer_text(named ?? 0, kind, left, zero, width.value, precision))]
        }
        at = cursor + 2
      } else {
        if emit { out += [b"%"] }
        at += 1
      }
    }
  }
  {text: bytes.concat(out), at: at}
}

# One line through a line format: `%L` the line with its newline, `%l` without,
# `%n` (with printf flags) the line number, `%c'C'` and `%%`.
pure line_text(raw: Bytes, line: Bytes, terminated: Bool, number: Int) -> Bytes {
  var out: List[Bytes] = []
  var at = 0
  let size = raw.len()
  while at < size {
    let value = raw.byte_at(at) ?? 0
    if value != 37 {
      out += [raw[at..at + 1]]
      at += 1
      continue
    }
    let after = raw.byte_at(at + 1)
    if after == null {
      out += [b"%"]
      at += 1
      continue
    }
    let letter = after ?? 0
    if letter == 37 {
      out += [b"%"]
      at += 2
    } else if letter == 76 {
      out += [line, if terminated { b"\n" } else { b"" }]
      at += 2
    } else if letter == 108 {
      out += [line]
      at += 2
    } else if letter == 99 {
      let spec = char_spec(raw, at + 2)
      if spec == null {
        out += [b"%"]
        at += 1
      } else {
        let found = spec ?? {value: 0, at: at}
        out += [bytes.from_ints([found.value]) ?? b""]
        at = found.at
      }
    } else {
      var cursor = at + 1
      var left = false
      var zero = false
      while cursor < size {
        let flag = raw.byte_at(cursor) ?? 0
        if flag == 45 {
          left = true
        } else if flag == 48 {
          zero = true
        } else {
          break
        }
        cursor += 1
      }
      let width = number_at(raw, cursor)
      cursor = width.at
      var precision = -1
      if raw.byte_at(cursor) == 46 {
        let digits = number_at(raw, cursor + 1)
        precision = digits.value
        cursor = digits.at
      }
      let conversion = raw.byte_at(cursor) ?? 0
      if (conversion == 100 or conversion == 111 or conversion == 120 or conversion == 88) and raw.byte_at(cursor + 1) == 110 {
        let kind = raw[cursor..cursor + 1].utf8() ?? "d"
        out += [bytes.from_text(integer_text(number, kind, left, zero, width.value, precision))]
        at = cursor + 2
      } else {
        out += [b"%"]
        at += 1
      }
    }
  }
  bytes.concat(out)
}

# The lines `from` up to `to` of one file through a line format.
pure group_lines(layout_text: Str, source: gnudiff.Source, from: Int, to: Int) -> Bytes {
  var out: List[Bytes] = []
  let raw = bytes.from_text(layout_text)
  for index in range(from, to) {
    out += [line_text(raw, source.lines[index], terminated(source, index), index + 1)]
  }
  bytes.concat(out)
}

## Merged-file output: unchanged stretches and changed groups through the group
# formats, each changed group chosen by what it changes.
export pure render_ifdef(first: gnudiff.Source, second: gnudiff.Source, script: List[gnudiff.Change], fmt: Format, templates: Templates) -> Bytes {
  var out: List[Bytes] = []
  var next0 = 0
  var next1 = 0
  let n0 = first.lines.len()
  let n1 = second.lines.len()
  for item in script {
    let hunk = analyze([item], first, second, fmt)
    if hunk.changes == 0 { continue }
    if next0 < hunk.first0 {
      out += [group_scan(bytes.from_text(templates.unchanged_group), 0, -1, true, {first: first, second: second, beg0: next0, end0: hunk.first0, beg1: next1, end1: hunk.first1, templates: templates}).text]
    }
    next0 = hunk.last0 + 1
    next1 = hunk.last1 + 1
    let pattern = if hunk.changes == OLD { templates.old_group } else if hunk.changes == NEW { templates.new_group } else { templates.changed_group }
    out += [group_scan(bytes.from_text(pattern), 0, -1, true, {first: first, second: second, beg0: hunk.first0, end0: next0, beg1: hunk.first1, end1: next1, templates: templates}).text]
  }
  if next0 < n0 {
    out += [group_scan(bytes.from_text(templates.unchanged_group), 0, -1, true, {first: first, second: second, beg0: next0, end0: n0, beg1: next1, end1: n1, templates: templates}).text]
  }
  bytes.concat(out)
}

## Render a script in the format `fmt.style` names. `header` is the two label
## lines of the unified and context formats.
export pure render(first: gnudiff.Source, second: gnudiff.Source, script: List[gnudiff.Change], fmt: Format, header: Bytes) -> Bytes {
  if fmt.style == "unified" { return render_context(first, second, script, fmt, header, true) }
  if fmt.style == "context" { return render_context(first, second, script, fmt, header, false) }
  if fmt.style == "sdiff" { return render_sdiff(first, second, script, fmt) }
  render_simple(first, second, script, fmt)
}
