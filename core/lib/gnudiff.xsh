##! Line comparison that picks the same edit script GNU diff prints: line
##! equivalence classes, the discard heuristics for lines that match too many or
##! no lines of the other file, the divide-and-conquer shortest-edit search, and
##! the boundary shifts that choose among equally short scripts.

## A file as comparison sees it: lines without their terminator, and whether the
## last line lacked one. A carriage return before the newline stays in the line
## unless it was stripped on request.
export type Source = {lines: List[Bytes], open: Bool}

## How lines are compared and which lines the search may drop first. `utf8`
## says the locale is UTF-8, so case folding and tab columns follow characters;
## `tabsize` is the tab stop `expand_tabs` (`-E`) compares by.
export type Rules = {utf8: Bool, tabsize: Int, ignore_case: Bool, all_space: Bool, space_change: Bool, trailing_space: Bool, expand_tabs: Bool, minimal: Bool, speed_large_files: Bool, horizon: Int}

## One run of changed lines: `deleted` lines of the first file starting at
## `line0` become `inserted` lines of the second starting at `line1`. Indexes are
## zero-based positions in the whole file.
export type Change = {line0: Int, line1: Int, deleted: Int, inserted: Int}

# A run of matching lines at least this long ends a forward or backward step
# and, with `--speed-large-files`, lets the search settle early.
const SNAKE_LIMIT = 20
const OFFSET_MAX = 1152921504606846976

## Whether a byte is white space for the comparison options (C locale).
export pure is_space(value: Int) -> Bool {
  value == 32 or (value >= 9 and value <= 13)
}

## Split a file into lines. `strip_cr` drops the carriage return of a CRLF
## terminator, as `--strip-trailing-cr` does; a carriage return at the very end
## of a file without a newline is always kept.
export pure split(data: Bytes, strip_cr: Bool) -> Source {
  let size = data.len()
  if size == 0 { return {lines: [], open: false} }
  let open = data.byte_at(size - 1) != 10
  let parts = data.lines()
  if !open and !(b"\r" in data) { return {lines: parts, open: false} }
  var lines: List[Bytes] = []
  var at = 0
  let count = parts.len()
  var index = 0
  while index < count {
    let part = parts[index]
    if open and index == count - 1 {
      lines += [data.slice(at)]
      break
    }
    let end = at + part.len()
    if data.byte_at(end) == 13 {
      lines += [if strip_cr { part } else { bytes.concat([part, b"\r"]) }]
      at = end + 2
    } else {
      lines += [part]
      at = end + 1
    }
    index += 1
  }
  {lines: lines, open: open}
}

## One character: its length in bytes and its width in columns.
export type Glyph = {bytes: Int, columns: Int}

# Ranges of code points that take no column (combining marks, joiners) and of
# East Asian wide characters that take two.
const NARROW = [[768, 879], [1155, 1161], [1425, 1469], [1471, 1471], [1473, 1474], [1476, 1477], [1479, 1479], [1552, 1562], [1611, 1631], [1648, 1648], [1750, 1756], [1759, 1764], [1767, 1768], [1770, 1773], [3633, 3633], [3636, 3642], [3655, 3662], [6832, 6911], [7616, 7679], [8203, 8207], [8400, 8447], [65024, 65039], [65056, 65071]]
const WIDE = [[4352, 4447], [11904, 12350], [12353, 13311], [13312, 19903], [19968, 40959], [40960, 42191], [44032, 55203], [63744, 64255], [65072, 65135], [65280, 65376], [65504, 65510], [127744, 128591], [129280, 129535], [131072, 262141]]

pure in_ranges(value: Int, ranges: List[List[Int]]) -> Bool {
  for span in ranges {
    if value >= span[0] and value <= span[1] { return true }
  }
  false
}

## The character of `raw` at `at`. Outside a UTF-8 locale, or for bytes that
## are not valid UTF-8, every byte is one character of one column.
export pure unit_at(raw: Bytes, at: Int, utf8: Bool) -> Glyph {
  let lead = raw.byte_at(at) ?? 0
  if !utf8 or lead < 194 or lead > 244 { return {bytes: 1, columns: 1} }
  let need = if lead >= 240 { 4 } else if lead >= 224 { 3 } else { 2 }
  if at + need > raw.len() { return {bytes: 1, columns: 1} }
  var value = if need == 2 { lead - 192 } else if need == 3 { lead - 224 } else { lead - 240 }
  for step in range(1, need) {
    let next = raw.byte_at(at + step) ?? 0
    if next < 128 or next > 191 { return {bytes: 1, columns: 1} }
    value = value * 64 + (next - 128)
  }
  if in_ranges(value, NARROW) { return {bytes: need, columns: 0} }
  if in_ranges(value, WIDE) { return {bytes: need, columns: 2} }
  {bytes: need, columns: 1}
}

# The comparison key of one line under the white-space and case options.
# Expanding tabs comes first so that the other rules see its spaces.
pure line_key(line: Bytes, rules: Rules) -> Bytes {
  var text = line
  if rules.expand_tabs {
    var chunks: List[Bytes] = []
    var column = 0
    var start = 0
    var index = 0
    while index < text.len() {
      if text.byte_at(index) == 9 {
        chunks += [text[start..index]]
        let pad = rules.tabsize - column % rules.tabsize
        for _ in range(pad) { chunks += [b" "] }
        column += pad
        start = index + 1
        index += 1
      } else {
        let unit = unit_at(text, index, rules.utf8)
        column += unit.columns
        index += unit.bytes
      }
    }
    if start > 0 { chunks += [text[start..text.len()]]; text = bytes.concat(chunks) }
  }
  if rules.all_space or rules.space_change or rules.trailing_space {
    var chunks: List[Bytes] = []
    var start = 0
    var index = 0
    let size = text.len()
    while index < size {
      if is_space(text.byte_at(index) ?? 0) {
        var end = index
        while end < size and is_space(text.byte_at(end) ?? 0) { end += 1 }
        if rules.all_space {
          chunks += [text[start..index]]
        } else if rules.space_change {
          chunks += [text[start..index]]
          if end < size { chunks += [b" "] }
        } else if end < size {
          chunks += [text[start..end]]
        } else {
          chunks += [text[start..index]]
        }
        start = end
        index = end
      } else {
        index += 1
      }
    }
    if start < size { chunks += [text[start..size]] }
    text = bytes.concat(chunks)
  }
  if rules.ignore_case {
    if rules.utf8 and text.utf8() is Ok(_) {
      text = bytes.from_text((text.utf8() ?? "").lower())
    } else {
      text = text.lower()
    }
  }
  text
}

# Whether a missing final newline is invisible to the comparison: only the
# options that drop trailing white space make `b` equal `b` plus a newline.
pure newline_is_whitespace(rules: Rules) -> Bool {
  rules.all_space or rules.space_change or rules.trailing_space
}

# The class numbers of one file's lines and the tables that assigned them.
type Classes = {classes: List[Int], ids: Map[Bytes, Int], open_ids: Map[Bytes, Int], next: Int}

# Lines of `source` from `from` up to `to` as class numbers. A final line
# without a newline gets its own classes unless trailing white space is ignored.
pure classify(source: Source, from: Int, to: Int, rules: Rules, plain: Bool, ids: Map[Bytes, Int], open_ids: Map[Bytes, Int], next: Int) -> Classes {
  var table = ids
  var open_table = open_ids
  var counter = next
  var classes: List[Int] = []
  let last = source.lines.len() - 1
  for index in range(from, to) {
    let line = if plain { source.lines[index] } else { line_key(source.lines[index], rules) }
    let distinct = source.open and index == last and !newline_is_whitespace(rules)
    var found = (if distinct { open_table.get(line) } else { table.get(line) }) ?? 0
    if found == 0 {
      counter += 1
      found = counter
      if distinct { open_table[line] = counter } else { table[line] = counter }
    }
    classes += [found]
  }
  {classes: classes, ids: table, open_ids: open_table, next: counter}
}

# Decide which lines are dropped before the search: lines with no counterpart
# are discarded outright, lines with very many counterparts only when they sit
# in long enough runs of discardable lines. Returns 0 (keep) or 1 (discard).
pure discards(classes: List[Int], other_counts: List[Int]) -> List[Int] {
  let end = classes.len()
  var many = 5
  var tem = end / 64
  while true {
    tem = tem / 4
    if tem <= 0 { break }
    many *= 2
  }
  var marks: List[Int] = []
  for value in classes {
    let matches = other_counts[value]
    marks += [if matches == 0 { 1 } else if matches > many { 2 } else { 0 }]
  }
  var i = 0
  while i < end {
    if marks[i] == 2 {
      marks[i] = 0
    } else if marks[i] != 0 {
      var j = i
      var provisional = 0
      while j < end {
        if marks[j] == 0 { break }
        if marks[j] == 2 { provisional += 1 }
        j += 1
      }
      while j > i and marks[j - 1] == 2 {
        j -= 1
        marks[j] = 0
        provisional -= 1
      }
      let length = j - i
      if provisional * 4 > length {
        while j > i {
          j -= 1
          if marks[j] == 2 { marks[j] = 0 }
        }
      } else {
        var minimum = 1
        var quarter = length / 4
        while true {
          quarter = quarter / 4
          if quarter <= 0 { break }
          minimum *= 2
        }
        minimum += 1
        var k = 0
        var consec = 0
        while k < length {
          if marks[i + k] != 2 {
            consec = 0
          } else {
            consec += 1
            if consec == minimum {
              k -= consec
            } else if minimum < consec {
              marks[i + k] = 0
            }
          }
          k += 1
        }
        k = 0
        consec = 0
        while k < length {
          if k >= 8 and marks[i + k] == 1 { break }
          if marks[i + k] == 2 {
            consec = 0
            marks[i + k] = 0
          } else if marks[i + k] == 0 {
            consec = 0
          } else {
            consec += 1
          }
          if consec == 3 { break }
          k += 1
        }
        i += length - 1
        k = 0
        consec = 0
        while k < length {
          if k >= 8 and marks[i - k] == 1 { break }
          if marks[i - k] == 2 {
            consec = 0
            marks[i - k] = 0
          } else if marks[i - k] == 0 {
            consec = 0
          } else {
            consec += 1
          }
          if consec == 3 { break }
          k += 1
        }
      }
    }
    i += 1
  }
  marks
}

type Flags = {first: List[Int], second: List[Int]}

# One subproblem of the search: both ranges and whether it must be minimal.
type Segment = {xoff: Int, xlim: Int, yoff: Int, ylim: Int, minimal: Bool}

# Mark the lines of the two sequences that are not part of a longest common
# subsequence, by position in the undiscarded sequences. This is the
# bidirectional search of the greedy shortest-edit-script algorithm, applied
# recursively to the middle of each remaining problem. `x` and `y` hold class
# numbers; the result flags are indexed like them.
pure search(x: List[Int], y: List[Int], minimal: Bool, speed: Bool) -> Flags {
  let nx = x.len()
  let ny = y.len()
  var deleted: List[Int] = [0 for _ in range(nx)]
  var inserted: List[Int] = [0 for _ in range(ny)]
  var expensive = 1
  var diags = nx + ny + 3
  while diags != 0 {
    expensive *= 2
    diags = diags / 4
  }
  if expensive < 4096 { expensive = 4096 }
  let shift = ny + 1
  var fd: List[Int] = [0 for _ in range(nx + ny + 3)]
  var bd: List[Int] = [0 for _ in range(nx + ny + 3)]
  var work: List[Segment] = [{xoff: 0, xlim: nx, yoff: 0, ylim: ny, minimal: minimal}]
  while !work.is_empty() {
    let item = work[work.len() - 1]
    work = work[0..work.len() - 1]
    var xoff = item.xoff
    var xlim = item.xlim
    var yoff = item.yoff
    var ylim = item.ylim
    while xoff < xlim and yoff < ylim and x[xoff] == y[yoff] {
      xoff += 1
      yoff += 1
    }
    while xoff < xlim and yoff < ylim and x[xlim - 1] == y[ylim - 1] {
      xlim -= 1
      ylim -= 1
    }
    if xoff == xlim {
      while yoff < ylim {
        inserted[yoff] = 1
        yoff += 1
      }
      continue
    }
    if yoff == ylim {
      while xoff < xlim {
        deleted[xoff] = 1
        xoff += 1
      }
      continue
    }
    let dmin = xoff - ylim
    let dmax = xlim - yoff
    let fmid = xoff - yoff
    let bmid = xlim - ylim
    var fmin = fmid
    var fmax = fmid
    var bmin = bmid
    var bmax = bmid
    let odd = (fmid - bmid) % 2 != 0
    fd[fmid + shift] = xoff
    bd[bmid + shift] = xlim
    var xmid = 0
    var ymid = 0
    var lo_minimal = false
    var hi_minimal = false
    var cost = 1
    var found = false
    while !found {
      var big_snake = false
      if fmin > dmin {
        fmin -= 1
        fd[fmin - 1 + shift] = -1
      } else {
        fmin += 1
      }
      if fmax < dmax {
        fmax += 1
        fd[fmax + 1 + shift] = -1
      } else {
        fmax -= 1
      }
      var d = fmax
      while d >= fmin {
        let tlo = fd[d - 1 + shift]
        let thi = fd[d + 1 + shift]
        let x0 = if tlo < thi { thi } else { tlo + 1 }
        var px = x0
        var py = x0 - d
        while px < xlim and py < ylim and x[px] == y[py] {
          px += 1
          py += 1
        }
        if px - x0 > SNAKE_LIMIT { big_snake = true }
        fd[d + shift] = px
        if odd and bmin <= d and d <= bmax and bd[d + shift] <= px {
          xmid = px
          ymid = py
          lo_minimal = true
          hi_minimal = true
          found = true
          break
        }
        d -= 2
      }
      if found { break }
      if bmin > dmin {
        bmin -= 1
        bd[bmin - 1 + shift] = OFFSET_MAX
      } else {
        bmin += 1
      }
      if bmax < dmax {
        bmax += 1
        bd[bmax + 1 + shift] = OFFSET_MAX
      } else {
        bmax -= 1
      }
      d = bmax
      while d >= bmin {
        let tlo = bd[d - 1 + shift]
        let thi = bd[d + 1 + shift]
        let x0 = if tlo < thi { tlo } else { thi - 1 }
        var px = x0
        var py = x0 - d
        while xoff < px and yoff < py and x[px - 1] == y[py - 1] {
          px -= 1
          py -= 1
        }
        if x0 - px > SNAKE_LIMIT { big_snake = true }
        bd[d + shift] = px
        if !odd and fmin <= d and d <= fmax and px <= fd[d + shift] {
          xmid = px
          ymid = py
          lo_minimal = true
          hi_minimal = true
          found = true
          break
        }
        d -= 2
      }
      if found { break }
      if item.minimal {
        cost += 1
        continue
      }
      if 200 < cost and big_snake and speed {
        var best = 0
        d = fmax
        while d >= fmin {
          let dd = d - fmid
          let px = fd[d + shift]
          let py = px - d
          let v = (px - xoff) * 2 - dd
          if v > 12 * (cost + (if dd < 0 { -dd } else { dd })) {
            if v > best and xoff + SNAKE_LIMIT <= px and px < xlim and yoff + SNAKE_LIMIT <= py and py < ylim {
              var k = 1
              while x[px - k] == y[py - k] {
                if k == SNAKE_LIMIT {
                  best = v
                  xmid = px
                  ymid = py
                  break
                }
                k += 1
              }
            }
          }
          d -= 2
        }
        if best > 0 {
          lo_minimal = true
          hi_minimal = false
          found = true
          break
        }
        best = 0
        d = bmax
        while d >= bmin {
          let dd = d - bmid
          let px = bd[d + shift]
          let py = px - d
          let v = (xlim - px) * 2 + dd
          if v > 12 * (cost + (if dd < 0 { -dd } else { dd })) {
            if v > best and xoff < px and px <= xlim - SNAKE_LIMIT and yoff < py and py <= ylim - SNAKE_LIMIT {
              var k = 0
              while x[px + k] == y[py + k] {
                if k == SNAKE_LIMIT - 1 {
                  best = v
                  xmid = px
                  ymid = py
                  break
                }
                k += 1
              }
            }
          }
          d -= 2
        }
        if best > 0 {
          lo_minimal = false
          hi_minimal = true
          found = true
          break
        }
      }
      if cost >= expensive {
        var fxybest = -1
        var fxbest = 0
        d = fmax
        while d >= fmin {
          var px = fd[d + shift]
          if px > xlim { px = xlim }
          var py = px - d
          if ylim < py {
            px = ylim + d
            py = ylim
          }
          if fxybest < px + py {
            fxybest = px + py
            fxbest = px
          }
          d -= 2
        }
        var bxybest = OFFSET_MAX
        var bxbest = 0
        d = bmax
        while d >= bmin {
          var px = bd[d + shift]
          if px < xoff { px = xoff }
          var py = px - d
          if py < yoff {
            px = yoff + d
            py = yoff
          }
          if px + py < bxybest {
            bxybest = px + py
            bxbest = px
          }
          d -= 2
        }
        if xlim + ylim - bxybest < fxybest - (xoff + yoff) {
          xmid = fxbest
          ymid = fxybest - fxbest
          lo_minimal = true
          hi_minimal = false
        } else {
          xmid = bxbest
          ymid = bxybest - bxbest
          lo_minimal = false
          hi_minimal = true
        }
        found = true
        break
      }
      cost += 1
    }
    work += [{xoff: xoff, xlim: xmid, yoff: yoff, ylim: ymid, minimal: lo_minimal}]
    work += [{xoff: xmid, xlim: xlim, yoff: ymid, ylim: ylim, minimal: hi_minimal}]
  }
  {first: deleted, second: inserted}
}

# Slide each run of changed lines up or down while the line that moves across it
# is the same, so that adjacent runs merge and a run that cannot merge ends as
# late as possible, or lines up with a change in the other file. `changed` and
# `other` carry a zero sentinel before and after the lines.
pure shift_boundaries(classes: List[Int], changed: List[Int], other: List[Int]) -> List[Int] {
  var flags = changed
  var shadow = other
  let end = classes.len()
  var i = 0
  var j = 0
  while true {
    while i < end and flags[i + 1] == 0 {
      var more = true
      while more {
        more = shadow[j + 1] != 0
        j += 1
      }
      i += 1
    }
    if i == end { break }
    var start = i
    i += 1
    while flags[i + 1] != 0 { i += 1 }
    while shadow[j + 1] != 0 { j += 1 }
    var runlength = 0
    var corresponding = 0
    while true {
      runlength = i - start
      while start > 0 and classes[start - 1] == classes[i - 1] {
        start -= 1
        flags[start + 1] = 1
        i -= 1
        flags[i + 1] = 0
        while flags[start] != 0 { start -= 1 }
        j -= 1
        while shadow[j + 1] != 0 { j -= 1 }
      }
      corresponding = if shadow[j] != 0 { i } else { end }
      while i != end and classes[start] == classes[i] {
        flags[start + 1] = 0
        start += 1
        flags[i + 1] = 1
        i += 1
        while flags[i + 1] != 0 { i += 1 }
        j += 1
        while shadow[j + 1] != 0 {
          j += 1
          corresponding = i
        }
      }
      if runlength == i - start { break }
    }
    while corresponding < i {
      start -= 1
      flags[start + 1] = 1
      i -= 1
      flags[i + 1] = 0
      j -= 1
      while shadow[j + 1] != 0 { j -= 1 }
    }
  }
  flags
}

pure same_line(first: Source, at: Int, second: Source, to: Int) -> Bool {
  first.lines[at] == second.lines[to] and (first.open and at == first.lines.len() - 1) == (second.open and to == second.lines.len() - 1)
}

## The edit script between two files, as runs of changed lines in order. The
## lines before the first and after the last difference are not searched, except
## for `horizon` lines of each, which stay in the problem as GNU keeps them.
export pure script(first: Source, second: Source, rules: Rules) -> List[Change] {
  let n0 = first.lines.len()
  let n1 = second.lines.len()
  var prefix = 0
  while prefix < n0 and prefix < n1 and same_line(first, prefix, second, prefix) { prefix += 1 }
  if prefix == n0 and prefix == n1 { return [] }
  let start = prefix - (if prefix < rules.horizon { prefix } else { rules.horizon })
  var suffix = 0
  while n0 - 1 - suffix >= start and n1 - 1 - suffix >= start and same_line(first, n0 - 1 - suffix, second, n1 - 1 - suffix) { suffix += 1 }
  let dropped = suffix - (if suffix < rules.horizon { suffix } else { rules.horizon })
  let end0 = n0 - dropped
  let end1 = n1 - dropped
  let plain = !(rules.ignore_case or rules.all_space or rules.space_change or rules.trailing_space or rules.expand_tabs)
  let left = classify(first, start, end0, rules, plain, {}, {}, 0)
  let right = classify(second, start, end1, rules, plain, left.ids, left.open_ids, left.next)
  let m0 = end0 - start
  let m1 = end1 - start
  var count0: List[Int] = [0 for _ in range(right.next + 1)]
  var count1: List[Int] = [0 for _ in range(right.next + 1)]
  for value in left.classes { count0[value] += 1 }
  for value in right.classes { count1[value] += 1 }
  let drop0 = discards(left.classes, count1)
  let drop1 = discards(right.classes, count0)
  var kept0: List[Int] = []
  var real0: List[Int] = []
  var changed0: List[Int] = [0 for _ in range(m0 + 2)]
  for index in range(m0) {
    if drop0[index] == 0 {
      kept0 += [left.classes[index]]
      real0 += [index]
    } else {
      changed0[index + 1] = 1
    }
  }
  var kept1: List[Int] = []
  var real1: List[Int] = []
  var changed1: List[Int] = [0 for _ in range(m1 + 2)]
  for index in range(m1) {
    if drop1[index] == 0 {
      kept1 += [right.classes[index]]
      real1 += [index]
    } else {
      changed1[index + 1] = 1
    }
  }
  let marks = search(kept0, kept1, rules.minimal, rules.speed_large_files)
  for index in range(kept0.len()) {
    if marks.first[index] != 0 { changed0[real0[index] + 1] = 1 }
  }
  for index in range(kept1.len()) {
    if marks.second[index] != 0 { changed1[real1[index] + 1] = 1 }
  }
  changed0 = shift_boundaries(left.classes, changed0, changed1)
  changed1 = shift_boundaries(right.classes, changed1, changed0)
  var changes: List[Change] = []
  var i0 = 0
  var i1 = 0
  while i0 < m0 or i1 < m1 {
    if (i0 < m0 and changed0[i0 + 1] != 0) or (i1 < m1 and changed1[i1 + 1] != 0) {
      let from0 = i0
      let from1 = i1
      while i0 < m0 and changed0[i0 + 1] != 0 { i0 += 1 }
      while i1 < m1 and changed1[i1 + 1] != 0 { i1 += 1 }
      changes += [{line0: start + from0, line1: start + from1, deleted: i0 - from0, inserted: i1 - from1}]
    } else {
      i0 += 1
      i1 += 1
    }
  }
  changes
}
