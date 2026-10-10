##! Patch input parsing for the patch applet.
##!
##! Splits patch text into lines that keep their terminators, finds the next
##! file patch (its format and file names), and reads hunks of unified,
##! context, and normal diffs into one hunk shape. Header parsing follows GNU
##! patch: names end at the first white space unless quoted, and `-p` counts
##! slash-separated components.

## One hunk body in diff-neutral form. `kinds` holds 0 for a context line, 1
## for a removed line, and 2 for an added line, in patch order; `texts` holds
## each line with its terminator unless the patch marked it as missing one.
## `old_first` is the 1-based input line of the first pattern line; for an
## empty pattern it is the line before which the text is inserted.
export type Hunk = {
  old_first: Int,
  old_count: Int,
  new_first: Int,
  new_count: Int,
  kinds: List[Int],
  texts: List[Bytes],
  prefix: Int,
  suffix: Int,
  function: Bytes,
  line: Int,
  old_start: Int,
  new_start: Int,
}

## Outcome of reading one hunk. `bad_line` is the 1-based patch line number of
## a malformed construct (0 when the hunk is well formed); `bad_text` is the
## text GNU patch echoes for it.
export type HunkRead = {hunk: Hunk?, next: Int, bad_line: Int, bad_text: Bytes, truncated: Bool, partial: Bool}

## A file name field of a patch header: `name` is the text before the
## timestamp, `stamp` the rest of the line, and `present` tells a missing
## name from an empty one.
export type HeaderName = {name: Str, stamp: Str, present: Bool}

## Git extended header facts for one file patch.
export type GitHeader = {
  present: Bool,
  old_path: Str,
  new_path: Str,
  rename_from: Str,
  rename_to: Str,
  copy_from: Str,
  copy_to: Str,
  old_mode: Str,
  new_mode: Str,
  new_file_mode: Str,
  deleted_file_mode: Str,
  binary: Bool,
}

## What the scan found: the format, the names, and where the first hunk starts.
## `kind` is "unified", "context", "normal", "ed", "git" (a Git header with no
## hunk lines), or "none". `start` is the first line of leading text for this
## patch; `body` is the line index of the first hunk line.
export type Scan = {
  kind: Str,
  start: Int,
  body: Int,
  old_name: HeaderName,
  new_name: HeaderName,
  index_name: Str,
  git: GitHeader,
  prereq: Str,
  cr_header: Bool,
}

const HUNK_RE = rx"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@(.*)$"
const CONTEXT_RANGE_RE = rx"^\*\*\* (\d+)(?:,(\d+))? \*\*\*\*"
const CONTEXT_NEW_RANGE_RE = rx"^--- (\d+)(?:,(\d+))? ----"
const NORMAL_RE = rx"^(\d+)(?:,(\d+))?([acd])(\d+)(?:,(\d+))?$"
const ED_RE = rx"^(\d+)(?:,(\d+))?([acdi])$"

## A header name that was not present.
export const NO_NAME: HeaderName = {name: "", stamp: "", present: false}

## A patch with no Git extended header.
export const NO_GIT: GitHeader = {
  present: false,
  old_path: "",
  new_path: "",
  rename_from: "",
  rename_to: "",
  copy_from: "",
  copy_to: "",
  old_mode: "",
  new_mode: "",
  new_file_mode: "",
  deleted_file_mode: "",
  binary: false,
}

## A hunk with no lines, the starting point for building one.
export const EMPTY_HUNK: Hunk = {
  old_first: 0,
  old_count: 0,
  new_first: 0,
  new_count: 0,
  kinds: [],
  texts: [],
  prefix: 0,
  suffix: 0,
  function: b"",
  line: 0,
  old_start: 0,
  new_start: 0,
}

## Split bytes into lines that keep their terminator. The last line has none
## when the data does not end in a line break. Lone carriage returns stay in
## the line.
export pure split_lines(data: Bytes) -> List[Bytes] {
  var output: List[Bytes] = []
  var at = 0
  let total = data.len()
  for chunk in data.lines() {
    var end = at + chunk.len()
    if end < total {
      if data.byte_at(end) == 10 { end += 1 } else if data.byte_at(end) == 13 {
        end += if end + 1 < total and data.byte_at(end + 1) == 10 { 2 } else { 1 }
      }
    }
    output += [data[at..end]]
    at = end
  }
  output
}

## The line without its terminator, as text; undecodable lines read as empty.
export pure body_text(line: Bytes) -> Str {
  let text = line.utf8() ?? ""
  if text.ends_with("\r\n") { return text.byte_slice(0, text.byte_len() - 2) }
  if text.ends_with("\n") { return text.byte_slice(0, text.byte_len() - 1) }
  text
}

## Remove a carriage return that precedes the line feed.
export pure strip_cr(line: Bytes) -> Bytes {
  let size = line.len()
  if size >= 2 and line.byte_at(size - 1) == 10 and line.byte_at(size - 2) == 13 {
    return bytes.concat([line[0..size - 2], b"\n"])
  }
  line
}

pure starts(line: Bytes, prefix: Str) -> Bool {
  line.starts_with(bytes.from_text(prefix))
}

pure number(text: Str) -> Int {
  text.parse_int() ?? 0
}

pure unquote(text: Str) -> HeaderName {
  var output = ""
  var index = 1
  let size = text.byte_len()
  while index < size {
    let char = text.byte_slice(index, length: 1)
    if char == "\"" { index += 1; break }
    if char == "\\" and index + 1 < size {
      let next = text.byte_slice(index + 1, length: 1)
      index += 2
      if next == "n" { output += "\n" } else if next == "t" { output += "\t" } else if next == "r" { output += "\r" } else if next == "a" { output += "\u{07}" } else if next == "b" { output += "\u{08}" } else if next == "f" { output += "\u{0c}" } else if next == "v" { output += "\u{0b}" } else if next >= "0" and next <= "7" {
        var value = number(next)
        var digits = 1
        while digits < 3 and index < size {
          let more = text.byte_slice(index, length: 1)
          if more >= "0" and more <= "7" { value = value * 8 + number(more); index += 1; digits += 1 } else { break }
        }
        output += (bytes.from_ints([value % 256]) ?? b"").utf8() ?? "?"
      } else { output += next }
      continue
    }
    output += char
    index += 1
  }
  {name: output, stamp: text.byte_slice(index).trim(), present: true}
}

## Split the text after a `--- `, `+++ `, or `*** ` marker into name and
## timestamp. A quoted name may contain white space; otherwise the name ends
## at the first white space character.
export pure fetch_name(rest: Str) -> HeaderName {
  let text = rest.trim()
  if text.starts_with("\"") { return unquote(text) }
  var end = 0
  let size = text.byte_len()
  # A tab ends the name, so a name may hold spaces; without one the first
  # space ends it.
  let tabbed = text.find("\t") != null
  while end < size {
    let char = text.byte_slice(end, length: 1)
    if char == "\t" or (char == " " and !tabbed) { break }
    end += 1
  }
  {name: text.byte_slice(0, end), stamp: text.byte_slice(end).trim(), present: true}
}

## Apply the `-p` component count to a header name. `count` of -1 keeps only
## the final component (GNU's default). Doubled slashes count as one
## separator; a name with fewer components than requested has no name left.
export pure strip_name(name: Str, count: Int) -> Str {
  if count < 0 {
    var last = name
    var index = 0
    let size = name.byte_len()
    var start = 0
    while index < size {
      if name.byte_slice(index, length: 1) == "/" {
        while index + 1 < size and name.byte_slice(index + 1, length: 1) == "/" { index += 1 }
        start = index + 1
      }
      index += 1
    }
    last = name.byte_slice(start)
    return last
  }
  var start = 0
  var remaining = count
  var index = 0
  let size = name.byte_len()
  while remaining > 0 and index < size {
    if name.byte_slice(index, length: 1) == "/" {
      while index + 1 < size and name.byte_slice(index + 1, length: 1) == "/" { index += 1 }
      start = index + 1
      remaining -= 1
    }
    index += 1
  }
  if remaining > 0 { return "" }
  name.byte_slice(start)
}

# Days from 1970-01-01 to a civil date.
pure days_from_civil(year: Int, month: Int, day: Int) -> Int {
  let y = if month <= 2 { year - 1 } else { year }
  let era = (if y >= 0 { y } else { y - 399 }) / 400
  let yoe = y - era * 400
  let mp = (month + 9) % 12
  let doy = (153 * mp + 2) / 5 + day - 1
  let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy
  era * 146097 + doe - 719468
}

const STAMP_RE = rx"^(\d{4})-(\d{2})-(\d{2})[ T](\d{2}):(\d{2}):(\d{2})(?:\.(\d+))?(?: ?([+-])(\d{2}):?(\d{2}))?"

## A parsed header timestamp: whole seconds since the epoch and nanoseconds.
export type StampTime = {valid: Bool, seconds: Int, nanos: Int}

## Parse the ISO-style timestamp that `diff` writes after a file name. A
## timestamp without a zone is read as UTC.
export pure parse_stamp(stamp: Str) -> StampTime {
  let c = STAMP_RE.captures(stamp)
  if c.is_empty() { return {valid: false, seconds: 0, nanos: 0} }
  let days = days_from_civil(number(c[1]), number(c[2]), number(c[3]))
  var seconds = days * 86400 + number(c[4]) * 3600 + number(c[5]) * 60 + number(c[6])
  if c[8] != "" {
    let offset = number(c[9]) * 3600 + number(c[10]) * 60
    seconds -= if c[8] == "-" { -offset } else { offset }
  }
  var fraction = c[7]
  while fraction.byte_len() < 9 { fraction += "0" }
  {valid: true, seconds: seconds, nanos: number(fraction.byte_slice(0, 9))}
}

## Whether a header timestamp is the Unix epoch, which diff uses for a file
## that does not exist.
export pure is_epoch(stamp: Str) -> Bool {
  let parsed = parse_stamp(stamp)
  parsed.valid and parsed.seconds == 0 and parsed.nanos == 0
}

pure first_byte(line: Bytes) -> Int {
  line.byte_at(0) ?? -1
}

type GitNames = {old: Str, new: Str}

pure git_names(text: Str) -> GitNames {
  let words = text.split(" ")
  if words.len() == 2 { return {old: words[0], new: words[1]} }
  let half = (text.byte_len() - 1) / 2
  if text.byte_len() % 2 == 1 and text.byte_slice(half, length: 1) == " " {
    return {old: text.byte_slice(0, half), new: text.byte_slice(half + 1)}
  }
  {old: words[0], new: words[words.len() - 1]}
}

# Whether a format is allowed. `allowed` is "any", "unified", "context",
# "normal", or "ed" as chosen by -u, -c, -n, -e.
pure format_allowed(allowed: Str, kind: Str) -> Bool {
  allowed == "any" or allowed == kind
}

## Find the next file patch at or after line `from`. `allowed` restricts the
## recognized format. The Git header is parsed whether or not a hunk follows.
export pure scan(lines: List[Bytes], from: Int, allowed: Str) -> Scan {
  var old_name = NO_NAME
  var new_name = NO_NAME
  var index_name = ""
  var prereq = ""
  var git = NO_GIT
  var cr_header = false
  var start = from
  var i = from
  let total = lines.len()
  var normal_candidate = -1
  var ed_candidate = -1
  var pending_stars = -1
  while i < total {
    let line = lines[i]
    let text = body_text(line)
    let crlf = line.ends_with(b"\r\n")
    if text.starts_with("Index: ") and index_name == "" {
      let fetched = fetch_name(text.byte_slice(7))
      index_name = fetched.name
      i += 1
      continue
    }
    if text.starts_with("Prereq: ") {
      prereq = text.byte_slice(8).trim()
      i += 1
      continue
    }
    if text.starts_with("diff --git ") and format_allowed(allowed, "unified") {
      var info = NO_GIT
      let names = git_names(text.byte_slice(11))
      info.present = true
      info.old_path = names.old
      info.new_path = names.new
      var j = i + 1
      while j < total {
        let extended = body_text(lines[j])
        if extended.starts_with("old mode ") { info.old_mode = extended.byte_slice(9).trim() } else if extended.starts_with("new mode ") { info.new_mode = extended.byte_slice(9).trim() } else if extended.starts_with("new file mode ") { info.new_file_mode = extended.byte_slice(14).trim() } else if extended.starts_with("deleted file mode ") { info.deleted_file_mode = extended.byte_slice(18).trim() } else if extended.starts_with("rename from ") { info.rename_from = extended.byte_slice(12) } else if extended.starts_with("rename to ") { info.rename_to = extended.byte_slice(10) } else if extended.starts_with("copy from ") { info.copy_from = extended.byte_slice(10) } else if extended.starts_with("copy to ") { info.copy_to = extended.byte_slice(8) } else if extended.starts_with("similarity index ") or extended.starts_with("dissimilarity index ") or extended.starts_with("index ") { } else if extended.starts_with("GIT binary patch") or extended.starts_with("Binary files ") { info.binary = true } else { break }
        j += 1
      }
      git = info
      old_name = NO_NAME
      new_name = NO_NAME
      start = i
      # A patch with hunks continues with its `---` and `+++` lines.
      if j + 1 < total and body_text(lines[j]).starts_with("--- ") and body_text(lines[j + 1]).starts_with("+++ ") {
        i = j
        continue
      }
      # A header that carries no hunks (rename, mode change, new empty file).
      return {kind: "git", start: start, body: j, old_name: NO_NAME, new_name: NO_NAME, index_name: index_name, git: git, prereq: prereq, cr_header: crlf}
    }
    if text.starts_with("--- ") and i + 1 < total and body_text(lines[i + 1]).starts_with("+++ ") and format_allowed(allowed, "unified") {
      # A unified header; the hunk start must follow directly.
      if i + 2 < total and body_text(lines[i + 2]).starts_with("@@ -") {
        old_name = fetch_name(text.byte_slice(4))
        new_name = fetch_name(body_text(lines[i + 1]).byte_slice(4))
        return {kind: "unified", start: start, body: i + 2, old_name: old_name, new_name: new_name, index_name: index_name, git: git, prereq: prereq, cr_header: crlf or lines[i + 1].ends_with(b"\r\n")}
      }
    }
    if text.starts_with("*** ") and i + 3 < total and body_text(lines[i + 1]).starts_with("--- ") and body_text(lines[i + 2]).starts_with("***************") and format_allowed(allowed, "context") {
      old_name = fetch_name(text.byte_slice(4))
      new_name = fetch_name(body_text(lines[i + 1]).byte_slice(4))
      return {kind: "context", start: start, body: i + 2, old_name: old_name, new_name: new_name, index_name: index_name, git: git, prereq: prereq, cr_header: crlf}
    }
    if text.starts_with("@@ -") and format_allowed(allowed, "unified") {
      return {kind: "unified", start: start, body: i, old_name: old_name, new_name: new_name, index_name: index_name, git: git, prereq: prereq, cr_header: crlf}
    }
    if text.starts_with("***************") and i + 1 < total and CONTEXT_RANGE_RE.matches(body_text(lines[i + 1])) and format_allowed(allowed, "context") {
      return {kind: "context", start: start, body: i, old_name: old_name, new_name: new_name, index_name: index_name, git: git, prereq: prereq, cr_header: crlf}
    }
    if NORMAL_RE.matches(text) and format_allowed(allowed, "normal") and i + 1 < total {
      let next = body_text(lines[i + 1])
      if next.starts_with("< ") or next.starts_with("> ") or next == "<" or next == ">" {
        return {kind: "normal", start: start, body: i, old_name: old_name, new_name: new_name, index_name: index_name, git: git, prereq: prereq, cr_header: crlf}
      }
    }
    if ED_RE.matches(text) and format_allowed(allowed, "ed") and ed_candidate < 0 {
      ed_candidate = i
    }
    i += 1
  }
  if ed_candidate >= 0 {
    return {kind: "ed", start: from, body: ed_candidate, old_name: old_name, new_name: new_name, index_name: index_name, git: git, prereq: prereq, cr_header: false}
  }
  {kind: "none", start: from, body: total, old_name: NO_NAME, new_name: NO_NAME, index_name: "", git: NO_GIT, prereq: "", cr_header: false}
}

pure finish(hunk: Hunk) -> Hunk {
  var out = hunk
  # Within a change group removed lines come before added ones, whatever
  # order the patch wrote them in.
  var kinds: List[Int] = []
  var texts: List[Bytes] = []
  var at = 0
  let count = hunk.kinds.len()
  while at < count {
    if hunk.kinds[at] == 0 {
      kinds += [0]
      texts += [hunk.texts[at]]
      at += 1
      continue
    }
    var end = at
    while end < count and hunk.kinds[end] != 0 { end += 1 }
    for index in range(at, end) { if hunk.kinds[index] == 1 { kinds += [1]; texts += [hunk.texts[index]] } }
    for index in range(at, end) { if hunk.kinds[index] == 2 { kinds += [2]; texts += [hunk.texts[index]] } }
    at = end
  }
  out.kinds = kinds
  out.texts = texts
  var prefix = 0
  var counting = true
  for kind in kinds {
    if kind == 0 and counting { prefix += 1 } else { counting = false }
  }
  var suffix = 0
  var index = kinds.len() - 1
  while index >= 0 and kinds[index] == 0 {
    suffix += 1
    index -= 1
  }
  # A hunk of context only has no changes to anchor; keep one count.
  if prefix == kinds.len() { suffix = 0 }
  out.prefix = prefix
  out.suffix = suffix
  out
}

pure without_terminator(text: Bytes) -> Bytes {
  let size = text.len()
  if size >= 2 and text.byte_at(size - 1) == 10 and text.byte_at(size - 2) == 13 { return text[0..size - 2] }
  if size >= 1 and text.byte_at(size - 1) == 10 { return text[0..size - 1] }
  text
}

pure failed_read(line: Int, text: Bytes) -> HunkRead {
  {hunk: null, next: 0, bad_line: line, bad_text: text, truncated: false, partial: false}
}

# The patch ended inside a body line, which GNU patch reports separately
# before calling the patch malformed.
pure partial_read(line: Int) -> HunkRead {
  {hunk: null, next: 0, bad_line: line, bad_text: b" \n", truncated: false, partial: true}
}

## Read one unified hunk whose `@@` line is `lines[at]`. With `strip` a
## carriage return before the line feed is removed from each body line.
export pure read_unified(lines: List[Bytes], at: Int, strip: Bool) -> HunkRead {
  let header = HUNK_RE.captures(body_text(lines[at]))
  if header.is_empty() { return failed_read(at + 1, lines[at]) }
  let old_first = number(header[1])
  let old_count = if header[2] == "" { 1 } else { number(header[2]) }
  let new_first = number(header[3])
  let new_count = if header[4] == "" { 1 } else { number(header[4]) }
  var kinds: List[Int] = []
  var texts: List[Bytes] = []
  var old_seen = 0
  var new_seen = 0
  var i = at + 1
  var truncated = false
  while old_seen < old_count or new_seen < new_count {
    if i >= lines.len() {
      if old_count - old_seen == new_count - new_seen { truncated = true; break }
      return failed_read(lines.len(), b" \n")
    }
    var line = lines[i]
    if strip { line = strip_cr(line) }
    let lead = first_byte(line)
    if i == lines.len() - 1 and !lines[i].ends_with(b"\n") and (lead == 32 or lead == 45 or lead == 43) { return partial_read(i) }
    if lead == 32 { kinds += [0]; texts += [line[1..]]; old_seen += 1; new_seen += 1 } else if lead == 45 { kinds += [1]; texts += [line[1..]]; old_seen += 1 } else if lead == 43 { kinds += [2]; texts += [line[1..]]; new_seen += 1 } else if lead == 10 { kinds += [0]; texts += [line]; old_seen += 1; new_seen += 1 } else if lead == 92 {
      if !texts.is_empty() { texts[texts.len() - 1] = without_terminator(texts[texts.len() - 1]) }
    } else { return failed_read(i + 1, lines[i]) }
    i += 1
  }
  while i < lines.len() and first_byte(lines[i]) == 92 {
    if !texts.is_empty() { texts[texts.len() - 1] = without_terminator(texts[texts.len() - 1]) }
    i += 1
  }
  var hunk = EMPTY_HUNK
  hunk.old_first = if old_count == 0 { old_first + 1 } else { old_first }
  hunk.old_count = old_seen
  hunk.new_first = if new_count == 0 { new_first + 1 } else { new_first }
  hunk.new_count = new_seen
  hunk.kinds = kinds
  hunk.texts = texts
  hunk.function = if header[5] == "" { b"" } else { bytes.from_text(header[5]) }
  hunk.line = at + 1
  hunk.old_start = old_first
  hunk.new_start = new_first
  var finished = finish(hunk)
  # A hunk cut short still expects the lines its header counted. They are
  # placeholders no line can equal, so only a fuzz that skips trailing
  # context can place it, and the output never uses them.
  var missing = old_count - old_seen
  while missing > 0 {
    finished.kinds += [0]
    finished.texts += [b"\xff\x00\xff patch: missing context line"]
    missing -= 1
  }
  finished.old_count = old_count
  finished.new_count = new_count
  {hunk: finished, next: i, bad_line: 0, bad_text: b"", truncated: truncated, partial: false}
}

# One line of a context-diff half: the marker character and the text after it.
type HalfLine = {mark: Int, text: Bytes}

pure range_count(low: Str, high: Str) -> Int {
  if high == "" { return 1 }
  let span = number(high) - number(low) + 1
  if span < 0 { 0 } else { span }
}

## Read one context hunk whose `***************` line is `lines[at]`.
export pure read_context(lines: List[Bytes], at: Int, strip: Bool) -> HunkRead {
  let total = lines.len()
  let function = body_text(lines[at]).byte_slice(15)
  if at + 1 >= total { return failed_read(-1, b"") }
  let old_header = CONTEXT_RANGE_RE.captures(body_text(lines[at + 1]))
  if old_header.is_empty() { return failed_read(at + 2, lines[at + 1]) }
  var old_lines: List[HalfLine] = []
  var new_lines: List[HalfLine] = []
  var i = at + 2
  var new_header: List[Str] = []
  var old_listed = false
  while i < total {
    let text = body_text(lines[i])
    let matched = CONTEXT_NEW_RANGE_RE.captures(text)
    if !matched.is_empty() { new_header = matched; break }
    var line = lines[i]
    if strip { line = strip_cr(line) }
    let lead = first_byte(line)
    let second = line.byte_at(1) ?? 32
    if lead == 92 {
      if !old_lines.is_empty() { old_lines[old_lines.len() - 1].text = without_terminator(old_lines[old_lines.len() - 1].text) }
    } else if line.len() >= 2 and (lead == 32 or lead == 45 or lead == 33) and second == 32 {
      old_lines += [{mark: lead, text: line[2..]}]
      old_listed = true
    } else if lead == 10 {
      old_lines += [{mark: 32, text: line}]
    } else if line.len() == 1 and (lead == 32 or lead == 45 or lead == 33) {
      old_lines += [{mark: lead, text: b"\n"}]
    } else { return failed_read(i + 1, lines[i]) }
    i += 1
  }
  if new_header.is_empty() { return failed_read(-1, b"") }
  i += 1
  let expected_new = range_count(new_header[1], new_header[2])
  var new_seen = 0
  var new_listed = false
  while i < total and new_seen < expected_new {
    var line = lines[i]
    if strip { line = strip_cr(line) }
    let lead = first_byte(line)
    let second = line.byte_at(1) ?? 32
    if line.len() >= 2 and (lead == 32 or lead == 43 or lead == 33) and second == 32 {
      new_lines += [{mark: lead, text: line[2..]}]
      new_seen += 1
      new_listed = true
    } else if lead == 10 {
      new_lines += [{mark: 32, text: line}]
      new_seen += 1
    } else if line.len() == 1 and (lead == 32 or lead == 43 or lead == 33) {
      new_lines += [{mark: lead, text: b"\n"}]
      new_seen += 1
    } else { break }
    i += 1
  }
  while i < total and first_byte(lines[i]) == 92 {
    if !new_lines.is_empty() { new_lines[new_lines.len() - 1].text = without_terminator(new_lines[new_lines.len() - 1].text) }
    i += 1
  }
  # A half that diff left out holds only context, which the other half shows.
  if !old_listed and !new_lines.is_empty() {
    for entry in new_lines { if entry.mark == 32 { old_lines += [entry] } }
  }
  if !new_listed and !old_lines.is_empty() {
    for entry in old_lines { if entry.mark == 32 { new_lines += [entry] } }
  }
  var kinds: List[Int] = []
  var texts: List[Bytes] = []
  var oi = 0
  var ni = 0
  while oi < old_lines.len() or ni < new_lines.len() {
    while oi < old_lines.len() and old_lines[oi].mark != 32 {
      kinds += [1]
      texts += [old_lines[oi].text]
      oi += 1
    }
    while ni < new_lines.len() and new_lines[ni].mark != 32 {
      kinds += [2]
      texts += [new_lines[ni].text]
      ni += 1
    }
    if oi < old_lines.len() {
      kinds += [0]
      texts += [old_lines[oi].text]
      oi += 1
      if ni < new_lines.len() { ni += 1 }
    } else if ni < new_lines.len() {
      kinds += [0]
      texts += [new_lines[ni].text]
      ni += 1
    }
  }
  var old_count = 0
  var new_count = 0
  for kind in kinds {
    if kind != 2 { old_count += 1 }
    if kind != 1 { new_count += 1 }
  }
  let old_start = number(old_header[1])
  let new_start = number(new_header[1])
  var hunk = EMPTY_HUNK
  hunk.old_first = if old_count == 0 { old_start + 1 } else { old_start }
  hunk.old_count = old_count
  hunk.new_first = if new_count == 0 { new_start + 1 } else { new_start }
  hunk.new_count = new_count
  hunk.kinds = kinds
  hunk.texts = texts
  hunk.function = bytes.from_text(function)
  hunk.line = at + 1
  hunk.old_start = old_start
  hunk.new_start = new_start
  {hunk: finish(hunk), next: i, bad_line: 0, bad_text: b"", truncated: false, partial: false}
}

## Read one normal-format hunk whose command line is `lines[at]`.
export pure read_normal(lines: List[Bytes], at: Int, strip: Bool) -> HunkRead {
  let total = lines.len()
  let command = NORMAL_RE.captures(body_text(lines[at]))
  if command.is_empty() { return failed_read(at + 1, lines[at]) }
  let action = command[3]
  let old_start = number(command[1])
  let old_count = if action == "a" { 0 } else { range_count(command[1], command[2]) }
  let new_start = number(command[4])
  let new_count = if action == "d" { 0 } else { range_count(command[4], command[5]) }
  var kinds: List[Int] = []
  var texts: List[Bytes] = []
  var i = at + 1
  var removed = 0
  while removed < old_count {
    if i >= total { return failed_read(total, b" \n") }
    var line = lines[i]
    if strip { line = strip_cr(line) }
    if line.starts_with(b"< ") { kinds += [1]; texts += [line[2..]]; removed += 1 } else if line.starts_with(b"<\n") { kinds += [1]; texts += [line[1..]]; removed += 1 } else if first_byte(line) == 92 {
      if !texts.is_empty() { texts[texts.len() - 1] = without_terminator(texts[texts.len() - 1]) }
    } else { return failed_read(i + 1, lines[i]) }
    i += 1
  }
  while i < total and first_byte(lines[i]) == 92 {
    if !texts.is_empty() { texts[texts.len() - 1] = without_terminator(texts[texts.len() - 1]) }
    i += 1
  }
  if action == "c" {
    if i >= total or body_text(lines[i]) != "---" { return failed_read(if i >= total { total } else { i + 1 }, if i >= total { b" \n" } else { lines[i] }) }
    i += 1
  }
  var added = 0
  while added < new_count {
    if i >= total { return failed_read(total, b" \n") }
    var line = lines[i]
    if strip { line = strip_cr(line) }
    if line.starts_with(b"> ") { kinds += [2]; texts += [line[2..]]; added += 1 } else if line.starts_with(b">\n") { kinds += [2]; texts += [line[1..]]; added += 1 } else if first_byte(line) == 92 {
      if !texts.is_empty() { texts[texts.len() - 1] = without_terminator(texts[texts.len() - 1]) }
    } else { return failed_read(i + 1, lines[i]) }
    i += 1
  }
  while i < total and first_byte(lines[i]) == 92 {
    if !texts.is_empty() { texts[texts.len() - 1] = without_terminator(texts[texts.len() - 1]) }
    i += 1
  }
  var hunk = EMPTY_HUNK
  hunk.old_first = if action == "a" { old_start + 1 } else { old_start }
  hunk.old_count = old_count
  hunk.new_first = if action == "d" { new_start + 1 } else { new_start }
  hunk.new_count = new_count
  hunk.kinds = kinds
  hunk.texts = texts
  hunk.function = b""
  hunk.line = at + 1
  hunk.old_start = old_start
  hunk.new_start = new_start
  {hunk: hunk, next: i, bad_line: 0, bad_text: b"", truncated: false, partial: false}
}

## Result of running an ed script: the new content, or the text of the first
## command that could not be carried out.
export type EdResult = {output: List[Bytes], failed: Str, next: Int}

const ED_ADDRESS_RE = rx"^(\.|\$|\d+)?(?:,(\.|\$|\d+))?\s*([a-zA-Z=]?)(.*)$"

pure ed_address(text: Str, current: Int, last: Int) -> Int {
  if text == "." { return current }
  if text == "$" { return last }
  text.parse_int() ?? -1
}

pure with_newline(text: Bytes) -> Bytes {
  if text.ends_with(b"\n") { text } else { bytes.concat([text, b"\n"]) }
}

## Apply an ed script that starts at `lines[at]` to `input` the way `ed -`
## does: commands run in order against the whole buffer, and text for `a`,
## `i`, and `c` ends at a line holding a single dot.
export pure run_ed(lines: List[Bytes], at: Int, input: List[Bytes]) -> EdResult {
  var buffer = input
  var current = buffer.len()
  var i = at
  while i < lines.len() {
    let text = body_text(lines[i])
    # GNU patch hands the editor only the commands that start with a digit
    # (and the text that follows them); anything else is dropped.
    if !rx"^[0-9]".matches(text) {
      i += 1
      continue
    }
    let c = ED_ADDRESS_RE.captures(text)
    if c.is_empty() { return {output: buffer, failed: text, next: i} }
    let command = c[3]
    let size = buffer.len()
    var first = if c[1] == "" { current } else { ed_address(c[1], current, size) }
    var last = if c[2] == "" { first } else { ed_address(c[2], current, size) }
    if c[1] == "" and c[2] == "" and command != "" { first = current; last = current }
    i += 1
    if command == "a" or command == "i" or command == "c" {
      var added: List[Bytes] = []
      var closed = false
      while i < lines.len() {
        let body = body_text(lines[i])
        i += 1
        if body == "." { closed = true; break }
        added += [with_newline(lines[i - 1])]
      }
      if !closed { return {output: buffer, failed: text, next: i} }
      if command == "a" {
        if first < 0 or first > size { return {output: buffer, failed: text, next: i} }
        buffer = bytes_list_splice(buffer, first, first, added)
        current = first + added.len()
      } else if command == "i" {
        let at_line = if first < 1 { 1 } else { first }
        if at_line > size + 1 { return {output: buffer, failed: text, next: i} }
        buffer = bytes_list_splice(buffer, at_line - 1, at_line - 1, added)
        current = at_line - 1 + added.len()
      } else {
        if first < 1 or last > size or last < first { return {output: buffer, failed: text, next: i} }
        buffer = bytes_list_splice(buffer, first - 1, last, added)
        current = first - 1 + added.len()
      }
    } else if command == "d" {
      if first < 1 or last > size or last < first { return {output: buffer, failed: text, next: i} }
      buffer = bytes_list_splice(buffer, first - 1, last, [])
      current = if first - 1 < buffer.len() { first } else { buffer.len() }
    } else if command == "s" {
      let parts = c[4].split(c[4].byte_slice(0, length: 1))
      if parts.len() < 3 or first < 1 or last > size { return {output: buffer, failed: text, next: i} }
      let pattern = regex.compile(parts[1])
      let compiled = match pattern {
        Ok(value) => value
        Err(_) => { return {output: buffer, failed: text, next: i} }
      }
      let global = parts.len() > 3 and parts[3] == "g"
      for index in range(first - 1, last) {
        let line = body_text(buffer[index])
        let found = compiled.find(line)
        var replaced = line
        if !found.is_empty() {
          if global { replaced = compiled.replace(line, with: parts[2]) } else {
            replaced = line.byte_slice(0, found[0].start) + parts[2] + line.byte_slice(found[0].end)
          }
        }
        buffer[index] = bytes.from_text(replaced + "\n")
      }
      current = last
    } else if command == "w" or command == "q" or command == "p" or command == "" {
      # Writing and quitting are implicit; nothing else in an ed script
      # changes the buffer.
    } else {
      return {output: buffer, failed: text, next: i}
    }
  }
  {output: buffer, failed: "", next: i}
}

pure bytes_list_splice(list: List[Bytes], from: Int, to: Int, middle: List[Bytes]) -> List[Bytes] {
  var output: List[Bytes] = []
  for index in range(from) { output += [list[index]] }
  output += middle
  for index in range(to, list.len()) { output += [list[index]] }
  output
}
