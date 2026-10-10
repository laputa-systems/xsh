##! GNU sed: script compiler and stream editor over byte records.
##!
##! A script is compiled from `-e` and `-f` chunks into a flat command vector
##! (blocks and branches are resolved to indexes) and then run over input files
##! that are read lazily, so `q` never touches later files and `$` looks ahead
##! only as far as GNU sed does. Regular expressions use the host POSIX matcher
##! with GNU syntax adjustments applied before matching.
use gnu

error SedError = Invalid : Usage

# A regular expression as handed to the host matcher. GROUPS is the number of
# capture groups, used to validate replacement back-references. BUFFER_START and
# BUFFER_END record GNU's \` and \' anchors, which only match at the ends of the
# whole pattern space even when MULTILINE is set.
type RegexSpec = {pattern: Str, ignore_case: Bool, multiline: Bool, groups: Int, buffer_start: Bool, buffer_end: Bool}
# An empty regex reuses the last one executed at run time.
enum RegexChoice { Explicit(RegexSpec), Previous }
enum Address { Line(Int), Stride(Int, Int), Last, Pattern(RegexChoice), Plus(Int), Multiple(Int) }
# One piece of a replacement: literal TEXT followed by capture GROUP (-1 for
# none, 0 for the whole match) rendered under case-conversion KIND.
type ReplNode = {text: Bytes, group: Int, kind: Int}
# PRINT is 0 for no `p` flag, 1 when `p` precedes any `e` flag (print before the
# command runs), and 2 when it follows `e` (print the command's output).
type Substitution = {regex: RegexChoice, nodes: List[ReplNode], global: Bool, occurrence: Int, print: Int, write: Str?, evaluate: Bool}
# One compiled command. Which fields matter depends on OP (the command byte):
# TEXT for a/i/c, NAME for r/R/w/W file names, NUMBER for q/Q exit codes, the
# l width, and jump targets, LABEL for b/t/T/: names.
type Instruction = {first: Address?, last: Address?, invert: Bool, op: Int, text: Bytes, name: Str, number: Int, label: Str, sub: Substitution?, table: List[Int]}
## One `-e` expression or `-f` script file given on the command line.
export type Chunk = {text: Bytes, file: Str?, number: Int}
type OpenBlock = {index: Int, where: Str}
## Parser state between chunks; `compile` returns it and `link` consumes it.
export type Parser = {commands: List[Instruction], blocks: List[OpenBlock], labels: Map[Int], pending: Int, first: Bool, quiet: Bool, opened: List[Str]}
## A compiled script: its commands and whether `#n` forced quiet mode.
export type Program = {commands: List[Instruction], quiet: Bool}
## Option state that changes how a script is compiled or run.
export type Options = {extended: Bool, sandbox: Bool, separate: Bool, null_data: Bool, line_length: Int}
## Where in-place editing writes: an empty SUFFIX keeps no backup.
export type InPlace = {enabled: Bool, suffix: Str, follow: Bool}

type SlashScan = {ok: Bool, text: Bytes, next: Int}
type NumberScan = {value: Int, next: Int}
type AddressScan = {found: Bool, address: Address?, next: Int}
type TextScan = {text: Bytes, next: Int, open: Bool}
type NameScan = {name: Str, next: Int}
type AnchorScan = {pattern: Bytes, start: Bool, end: Bool}
type Capture = {start: Int, end: Int}
type Replacement = {text: Bytes, changed: Bool}

const NEWLINE = 10
const MODE_LOWER = 1
const MODE_UPPER = 2
const FIRST_UPPER = 4
const FIRST_LOWER = 8

pure is_blank(c: Int) -> Bool { c == 32 or c == 9 }
pure is_space(c: Int) -> Bool { c == 32 or (c >= 9 and c <= 13) }
pure is_digit(c: Int) -> Bool { c >= 48 and c <= 57 }

pure at_byte(text: Bytes, at: Int) -> Int { text.byte_at(at) ?? -1 }

pure skip_blanks(text: Bytes, start: Int) -> Int {
  var at = start
  while is_blank(at_byte(text, at)) { at += 1 }
  at
}

pure read_integer(text: Bytes, start: Int) -> NumberScan {
  var at = start
  var value = 0
  while is_digit(at_byte(text, at)) {
    # GNU wraps an overlong number silently; saturating keeps it far past any line.
    if value < 400000000000000000 { value = value * 10 + (at_byte(text, at) - 48) }
    at += 1
  }
  {value: value, next: at}
}

pure line_of(text: Bytes, at: Int) -> Int {
  var line = 1
  for index in range(at) {
    if text.byte_at(index) == NEWLINE { line += 1 }
  }
  line
}

# GNU names the chunk and the number of characters read so far; a script file
# is named by its line instead.
pure place(chunk: Chunk, at: Int) -> Str {
  if let file = chunk.file { return f"file {file} line {line_of(chunk.text, at)}" }
  f"-e expression #{chunk.number}, char {at}"
}

pure complain(chunk: Chunk, at: Int, message: Str) -> Error {
  SedError.Invalid(f"{place(chunk, at)}: {message}")
}

pure digit_value(byte: Int, radix: Int) -> Int {
  let value = if byte >= 48 and byte <= 57 { byte - 48 } else if byte >= 65 and byte <= 70 { byte - 55 } else if byte >= 97 and byte <= 102 { byte - 87 } else { -1 }
  if value >= 0 and value < radix { value } else { -1 }
}

# Control byte named by a C-style escape letter, or -1.
pure control_byte(letter: Int) -> Int {
  match letter {
    97 => 7
    102 => 12
    110 => 10
    114 => 13
    116 => 9
    118 => 11
    _ => -1
  }
}

pure upper_byte(byte: Int) -> Int { if byte >= 97 and byte <= 122 { byte - 32 } else { byte } }
pure lower_byte(byte: Int) -> Int { if byte >= 65 and byte <= 90 { byte + 32 } else { byte } }

# End of the bracket expression whose `[` is at OPEN: the index after its `]`,
# or -1 when it never closes on that line. A `]` first in the list is a member,
# and `[:`, `[.`, `[=` open nested classes that close with `:]`, `.]`, `=]`.
pure bracket_end(text: Bytes, open: Int, newline_ends: Bool) -> Int {
  var at = open + 1
  if at_byte(text, at) == 94 { at += 1 }
  if at_byte(text, at) == 93 { at += 1 }
  while at < text.len() {
    let byte = at_byte(text, at)
    if byte == NEWLINE and newline_ends { return -1 }
    if byte == 91 and at_byte(text, at + 1) in [58, 46, 61] {
      let kind = at_byte(text, at + 1)
      var close = at + 2
      while close + 1 < text.len() and !(at_byte(text, close) == kind and at_byte(text, close + 1) == 93) {
        if at_byte(text, close) == NEWLINE and newline_ends { return -1 }
        close += 1
      }
      if close + 1 >= text.len() { return -1 }
      at = close + 2
      continue
    }
    if byte == 93 { return at + 1 }
    at += 1
  }
  -1
}

# GNU's match_slash: read up to the unescaped delimiter SLASH. An escaped
# delimiter loses its backslash, `\n` becomes a newline in a regex, and an
# unescaped newline ends the command unterminated without being consumed. In a
# regex a bracket expression holds the delimiter literally.
pure match_slash(text: Bytes, start: Int, slash: Int, is_regex: Bool) -> SlashScan {
  var at = start
  var out: List[Bytes] = []
  while at < text.len() {
    let byte = at_byte(text, at)
    if byte == NEWLINE { return {ok: false, text: b"", next: at} }
    at += 1
    if byte == slash { return {ok: true, text: bytes.concat(out), next: at} }
    if byte == 92 {
      if at >= text.len() { return {ok: false, text: b"", next: at} }
      let escaped = at_byte(text, at)
      at += 1
      if escaped == 110 and is_regex {
        out += [b"\n"]
      } else if escaped == NEWLINE {
        out += [b"\n"]
      } else if escaped == slash and !(!is_regex and escaped == 38) {
        out += [text[at - 1..at]]
      } else {
        out += [b"\\", text[at - 1..at]]
      }
      continue
    }
    if byte == 91 and is_regex {
      let close = bracket_end(text, at - 1, true)
      if close < 0 {
        var stop = at
        while stop < text.len() and at_byte(text, stop) != NEWLINE { stop += 1 }
        return {ok: false, text: b"", next: stop}
      }
      out += [text[at - 1..close]]
      at = close
      continue
    }
    out += [text[at - 1..at]]
  }
  {ok: false, text: b"", next: at}
}

# Which buffer a normalized escape belongs to: it decides whether an unknown
# escape keeps its backslash and how a numeric escape that yields a special
# byte must be protected.
enum TextKind { RegexText, ReplacementText, LiteralText, TextBlock }

# GNU's normalize_text: C-style and numeric escapes become the bytes they name.
# Unknown escapes keep their backslash, except in a/i/c text where it is dropped.
pure normalize(text: Bytes, kind: TextKind) -> Result[Bytes, Error] {
  var out: List[Bytes] = []
  var at = 0
  while at < text.len() {
    let byte = at_byte(text, at)
    if byte != 92 or at + 1 >= text.len() {
      out += [text[at..at + 1]]
      at += 1
      continue
    }
    let letter = at_byte(text, at + 1)
    let control = control_byte(letter)
    if control >= 0 {
      out += [bytes.from_ints([control])?]
      at += 2
      continue
    }
    if letter in [100, 111, 120] {
      let radix = if letter == 120 { 16 } else if letter == 111 { 8 } else { 10 }
      let width = if radix == 16 { 2 } else { 3 }
      var value = 0
      var digits = 0
      var cursor = at + 2
      while digits < width {
        let found = digit_value(at_byte(text, cursor), radix)
        if found < 0 { break }
        value = value * radix + found
        cursor += 1
        digits += 1
      }
      if digits == 0 {
        out += [text[at + 1..at + 2]]
        at += 2
        continue
      }
      let produced = value % 256
      if kind == .ReplacementText and produced in [38, 92] { out += [b"\\"] }
      out += [bytes.from_ints([produced])?]
      at = cursor
      continue
    }
    if letter == 99 and at + 2 < text.len() {
      let source = at_byte(text, at + 2)
      var consumed = 3
      if source == 92 {
        if at_byte(text, at + 3) != 92 { return Err(SedError.Invalid("recursive escaping after \\c not allowed")) }
        consumed = 4
      }
      let upper = upper_byte(source)
      out += [bytes.from_ints([if upper / 64 % 2 == 1 { upper - 64 } else { upper + 64 }])?]
      at += consumed
      continue
    }
    if kind == .TextBlock {
      out += [text[at + 1..at + 2]]
      at += 2
      continue
    }
    out += [text[at..at + 2]]
    at += 2
  }
  Ok(bytes.concat(out))
}

# Number of capture groups in a pattern: `\(` in basic syntax, `(` in extended.
pure group_count(pattern: Bytes, extended: Bool) -> Int {
  var count = 0
  var at = 0
  while at < pattern.len() {
    let byte = at_byte(pattern, at)
    at += 1
    if byte == 92 {
      if at < pattern.len() {
        if !extended and at_byte(pattern, at) == 40 { count += 1 }
        at += 1
      }
    } else if byte == 91 {
      let close = bracket_end(pattern, at - 1, false)
      if close >= 0 { at = close }
    } else if extended and byte == 40 { count += 1 }
  }
  count
}

# GNU `\`` and `\'` anchor to the whole pattern space; the host matcher lacks
# them, so they are rewritten to `^` and `$` outside bracket expressions.
pure rewrite_anchors(pattern: Bytes) -> AnchorScan {
  var out: List[Bytes] = []
  var at = 0
  var start = false
  var end = false
  while at < pattern.len() {
    let byte = at_byte(pattern, at)
    if byte == 92 and at + 1 < pattern.len() {
      let next = at_byte(pattern, at + 1)
      if next == 96 { out += [b"^"]; start = true } else if next == 39 { out += [b"$"]; end = true } else { out += [pattern[at..at + 2]] }
      at += 2
      continue
    }
    if byte == 91 {
      let close = bracket_end(pattern, at, false)
      if close >= 0 {
        out += [pattern[at..close]]
        at = close
        continue
      }
    }
    out += [pattern[at..at + 1]]
    at += 1
  }
  {pattern: bytes.concat(out), start: start, end: end}
}

# Which parenthesis is unbalanced, as GNU words it, or null when they balance.
pure paren_failure(pattern: Bytes, extended: Bool) -> Str? {
  var depth = 0
  var at = 0
  while at < pattern.len() {
    let byte = at_byte(pattern, at)
    at += 1
    if byte == 92 {
      let next = at_byte(pattern, at)
      at += 1
      if !extended and next == 40 { depth += 1 }
      if !extended and next == 41 {
        depth -= 1
        if depth < 0 { return "Unmatched ) or \\)" }
      }
      continue
    }
    if byte == 91 {
      let close = bracket_end(pattern, at - 1, false)
      if close >= 0 { at = close }
      continue
    }
    if extended and byte == 40 { depth += 1 }
    if extended and byte == 41 {
      depth -= 1
      if depth < 0 { return "Unmatched ) or \\)" }
    }
  }
  if depth > 0 { return "Unmatched ( or \\(" }
  null
}

# Extended syntax errors the host matcher accepts: a repetition operator right
# after `^`.
pure extended_precheck(pattern: Bytes) -> Str? {
  var at = 0
  var previous = -1
  while at < pattern.len() {
    let byte = at_byte(pattern, at)
    at += 1
    if byte == 92 { at += 1; previous = 0; continue }
    if byte == 91 {
      let close = bracket_end(pattern, at - 1, false)
      if close >= 0 { at = close }
      previous = 0
      continue
    }
    if byte in [42, 43, 63] and previous == 94 { return "Invalid preceding regular expression" }
    previous = byte
  }
  null
}

# Map the host matcher's wording for a compile failure to GNU's.
pure regex_message(host: Str, pattern: Str, extended: Bool) -> Str {
  if host == "Invalid contents of {}" {
    let closer = if extended { "}" } else { "\\}" }
    if pattern.find(closer) == null { return "Unmatched \\{" }
    return "Invalid content of \\{\\}"
  }
  match host {
    "Missing '}'" => "Unmatched \\{"
    "Invalid character range" => "Invalid range end"
    "Unknown character class name" => "Invalid character class name"
    "Missing ']'" => "Unmatched [, [^, [:, [., or [="
    "Repetition not preceded by valid expression" => "Invalid preceding regular expression"
    "Unknown collating element" => "Invalid collation character"
    _ => host
  }
}

# GNU accepts `{,N}` as `{0,N}`; the host matcher does not.
pure widen_intervals(pattern: Bytes, extended: Bool) -> Bytes {
  var out: List[Bytes] = []
  var at = 0
  while at < pattern.len() {
    let byte = at_byte(pattern, at)
    if byte == 92 and at + 1 < pattern.len() {
      if !extended and at_byte(pattern, at + 1) == 123 and at_byte(pattern, at + 2) == 44 {
        out += [b"\\{0"]
        at += 2
        continue
      }
      out += [pattern[at..at + 2]]
      at += 2
      continue
    }
    if byte == 91 {
      let close = bracket_end(pattern, at, false)
      if close >= 0 {
        out += [pattern[at..close]]
        at = close
        continue
      }
    }
    if extended and byte == 123 and at_byte(pattern, at + 1) == 44 {
      out += [b"{0"]
      at += 1
      continue
    }
    out += [pattern[at..at + 1]]
    at += 1
  }
  bytes.concat(out)
}

# GNU reads `\+` and `\?` as literal bytes where nothing precedes them to
# repeat (pattern start, after `\(`, `\|`, or `^`), and accepts a single-byte
# collating symbol or equivalence class such as `[[.a.]]` and `[[=a=]]`; the
# host matcher rejects both.
pure adapt_basic(pattern: Bytes) -> Bytes {
  var out: List[Bytes] = []
  var at = 0
  var fresh = true
  while at < pattern.len() {
    let byte = at_byte(pattern, at)
    if byte == 92 and at + 1 < pattern.len() {
      let next = at_byte(pattern, at + 1)
      if fresh and next in [43, 63] {
        out += [pattern[at + 1..at + 2]]
        at += 2
        fresh = false
        continue
      }
      out += [pattern[at..at + 2]]
      fresh = next in [40, 124]
      at += 2
      continue
    }
    if byte == 91 {
      let close = bracket_end(pattern, at, false)
      if close >= 0 {
        out += [simplify_bracket(pattern[at..close])]
        at = close
        fresh = false
        continue
      }
    }
    out += [pattern[at..at + 1]]
    fresh = byte == 94 and fresh
    at += 1
  }
  bytes.concat(out)
}

# Replace `[.c.]` and `[=c=]` inside a bracket expression by the byte `c`.
pure simplify_bracket(text: Bytes) -> Bytes {
  var out: List[Bytes] = []
  var at = 0
  while at < text.len() {
    if at_byte(text, at) == 91 and at_byte(text, at + 1) in [46, 61] and at + 4 < text.len() and at_byte(text, at + 3) == at_byte(text, at + 1) and at_byte(text, at + 4) == 93 {
      out += [text[at + 2..at + 3]]
      at += 5
      continue
    }
    out += [text[at..at + 1]]
    at += 1
  }
  bytes.concat(out)
}

pure compile_regex(source: Bytes, extended: Bool, ignore_case: Bool, multiline: Bool) -> Result[RegexChoice, Error] {
  let anchors = rewrite_anchors(source)
  let adapted = if extended { anchors.pattern } else { adapt_basic(anchors.pattern) }
  let widened = widen_intervals(adapted, extended)
  if let message = paren_failure(widened, extended) { return Err(SedError.Invalid(message)) }
  if extended {
    if let message = extended_precheck(widened) { return Err(SedError.Invalid(message)) }
  }
  guard let pattern = widened.utf8() else { return Err(SedError.Invalid("regular expression is not valid UTF-8")) }
  match regex.captures_bytes(pattern, b"", 0, extended, ignore_case) {
    Err(failure) => { return Err(SedError.Invalid(regex_message(failure.message, pattern, extended))) }
    Ok(_) => {}
  }
  Ok(.Explicit({pattern: pattern, ignore_case: ignore_case, multiline: multiline, groups: group_count(source, extended), buffer_start: anchors.start, buffer_end: anchors.end}))
}

# Parse one address starting at the first byte, or report that there is none. In
# NUL-delimited mode GNU's M flag changes only which bytes `.` and lists match,
# never where `^` and `$` hold, so the line-by-line emulation is not applied.
pure parse_address(chunk: Chunk, start: Int, options: Options) -> Result[AddressScan, Error] {
  let text = chunk.text
  var at = start
  let byte = at_byte(text, at)
  if byte == 47 or byte == 92 {
    at += 1
    var slash = 47
    if byte == 92 {
      slash = at_byte(text, at)
      if slash < 0 { return Err(complain(chunk, at, "unterminated address regex")) }
      at += 1
    }
    let scan = match_slash(text, at, slash, true)
    if !scan.ok { return Err(complain(chunk, scan.next, "unterminated address regex")) }
    at = scan.next
    var ignore_case = false
    var multiline = false
    while true {
      let flag_at = skip_blanks(text, at)
      let flag = at_byte(text, flag_at)
      if flag == 73 { ignore_case = true; at = flag_at + 1 } else if flag == 77 { multiline = true; at = flag_at + 1 } else { break }
    }
    let normal = normalize(scan.text, .RegexText)?
    if normal.is_empty() {
      if ignore_case or multiline { return Err(complain(chunk, at, "cannot specify modifiers on empty regexp")) }
      return Ok({found: true, address: .Pattern(.Previous), next: at})
    }
    let spec = match compile_regex(normal, options.extended, ignore_case, multiline and !options.null_data) {
      Ok(value) => value
      Err(failure) => { return Err(complain(chunk, at, failure.message)) }
    }
    return Ok({found: true, address: .Pattern(spec), next: at})
  }
  if is_digit(byte) {
    let number = read_integer(text, at)
    at = number.next
    let tilde_at = skip_blanks(text, at)
    if at_byte(text, tilde_at) == 126 {
      let step = read_integer(text, skip_blanks(text, tilde_at + 1))
      if step.value > 0 { return Ok({found: true, address: .Stride(number.value, step.value), next: step.next}) }
      return Ok({found: true, address: .Line(number.value), next: step.next})
    }
    return Ok({found: true, address: .Line(number.value), next: at})
  }
  if byte == 43 or byte == 126 {
    let number = read_integer(text, skip_blanks(text, at + 1))
    let relative: Address = if byte == 43 { .Plus(number.value) } else { .Multiple(number.value) }
    return Ok({found: true, address: relative, next: number.next})
  }
  if byte == 36 { return Ok({found: true, address: .Last, next: at + 1}) }
  Ok({found: false, address: null, next: at})
}

# Characters consumed once the next non-blank byte has been read, as GNU's
# lookahead for the command character counts it.
pure consumed_through(text: Bytes, start: Int) -> Int {
  let at = skip_blanks(text, start)
  if at < text.len() { at + 1 } else { at }
}

# The end of a command: blanks, then `;`, a newline, or end of text; a closing
# brace or comment is left for the next command.
pure end_of_command(chunk: Chunk, start: Int) -> Result[Int, Error] {
  let at = skip_blanks(chunk.text, start)
  let byte = at_byte(chunk.text, at)
  if byte == 125 or byte == 35 { return Ok(at) }
  if byte < 0 { return Ok(at) }
  if byte == NEWLINE or byte == 59 { return Ok(at + 1) }
  Err(complain(chunk, at + 1, "extra characters after command"))
}

# A label operand: up to whitespace, `;`, or `}`.
pure read_label(text: Bytes, start: Int) -> NameScan {
  let begin = skip_blanks(text, start)
  var at = begin
  while at < text.len() and !is_space(at_byte(text, at)) and at_byte(text, at) != 59 and at_byte(text, at) != 125 { at += 1 }
  {name: text[begin..at].utf8() ?? "", next: at}
}

# A file name runs to the end of its line.
pure read_file_name(text: Bytes, start: Int) -> NameScan {
  let begin = skip_blanks(text, start)
  var at = begin
  while at < text.len() and at_byte(text, at) != NEWLINE { at += 1 }
  {name: text[begin..at].utf8() ?? "", next: at}
}

# The text of a/i/c runs to an unescaped newline. A backslash keeps the next
# byte (escapes such as `\t` are expanded) and a backslash-newline continues
# the text on the next line. The result always ends in a newline.
pure read_text(chunk: Chunk, start: Int) -> Result[TextScan, Error] {
  let text = chunk.text
  var at = start
  var out: List[Bytes] = []
  var open = false
  while at < text.len() {
    let byte = at_byte(text, at)
    at += 1
    if byte == NEWLINE { break }
    if byte == 92 {
      if at >= text.len() {
        open = true
        break
      }
      out += [text[at - 1..at + 1]]
      at += 1
      continue
    }
    out += [text[at - 1..at]]
  }
  let normal = normalize(bytes.concat(out), .TextBlock)?
  Ok({text: bytes.concat([normal, b"\n"]), next: at, open: open})
}

# Build the replacement nodes the way GNU's setup_replacement does: each
# backslash, `&`, and the end close the pending literal into a node, `\1`..`\9`
# and `&` give that node a group, and case escapes change the mode later nodes
# carry (`\u` and `\l` last for one node).
pure parse_replacement(text: Bytes) -> List[ReplNode] {
  var nodes: List[ReplNode] = []
  var base = 0
  var current = 0
  var saved = 0
  var at = 0
  while at < text.len() {
    let byte = at_byte(text, at)
    if byte == 92 {
      var node: ReplNode = {text: text[base..at], group: -1, kind: current}
      current = saved
      at += 1
      if at >= text.len() {
        nodes += [node]
        base = at
        break
      }
      let escaped = at_byte(text, at)
      if is_digit(escaped) {
        nodes += [{...node, group: escaped - 48}]
      } else if escaped == 76 {
        nodes += [node]
        current = MODE_LOWER
        saved = MODE_LOWER
      } else if escaped == 85 {
        nodes += [node]
        current = MODE_UPPER
        saved = MODE_UPPER
      } else if escaped == 69 {
        nodes += [node]
        current = 0
        saved = 0
      } else if escaped == 108 {
        nodes += [node]
        saved = current
        if current < 8 { current += FIRST_LOWER }
      } else if escaped == 117 {
        nodes += [node]
        saved = current
        if current / 4 % 2 == 0 { current += FIRST_UPPER }
      } else {
        # Any other escaped byte is literal and joins this node's prefix.
        nodes += [{...node, text: bytes.concat([node.text, text[at..at + 1]])}]
      }
      at += 1
      base = at
      continue
    }
    if byte == 38 {
      nodes += [{text: text[base..at], group: 0, kind: current}]
      current = saved
      at += 1
      base = at
      continue
    }
    at += 1
  }
  if base < text.len() { nodes += [{text: text[base..], group: -1, kind: current}] }
  nodes
}

pure highest_group(nodes: List[ReplNode]) -> Int {
  var highest = 0
  for node in nodes {
    if node.group > highest { highest = node.group }
  }
  highest
}

pure blank_command(op: Int) -> Instruction {
  {first: null, last: null, invert: false, op: op, text: b"", name: "", number: -1, label: "", sub: null, table: []}
}

# Decode a `y` operand: `\\` is a backslash and `\n` a newline; the delimiter's
# escape was already removed by the scan.
pure translate_operand(text: Bytes) -> List[Int] {
  var out: List[Int] = []
  var at = 0
  while at < text.len() {
    let byte = at_byte(text, at)
    if byte == 92 and at + 1 < text.len() {
      let next = at_byte(text, at + 1)
      if next == 92 { out += [92]; at += 2; continue }
      if next == 110 { out += [10]; at += 2; continue }
    }
    out += [byte]
    at += 1
  }
  out
}

# GNU creates (or truncates) a `w` file the moment its command is read, so the
# file exists even when a later syntax error stops the run, and an unwritable
# name is reported before anything after it is looked at.
proc open_output(opened: List[Str], name: Str) [fs, io, error, process, env] -> List[Str] {
  if name in ["/dev/stdout", "/dev/stderr"] or name in opened { return opened }
  match fp"{name}".write(b"") {
    Err(failure) => {
      gnu.error(f"couldn't open file {name}: {gnu.strerror(failure)}")
      exit 4
    }
    Ok(_) => {}
  }
  opened + [name]
}

proc parse_chunk(state: Parser, chunk: Chunk, options: Options) [fs, io, error, process, env] -> Result[Parser, Error] {
  let text = chunk.text
  var commands = state.commands
  var blocks = state.blocks
  var labels = state.labels
  var pending = state.pending
  var quiet = state.quiet
  var opened = state.opened
  var at = 0
  if pending >= 0 {
    let scan = read_text(chunk, 0)?
    commands[pending] = {...commands[pending], text: bytes.concat([commands[pending].text, scan.text])}
    pending = if scan.open { pending } else { -1 }
    at = scan.next
  }
  if state.first and at_byte(text, 0) == 35 and at_byte(text, 1) == 110 {
    quiet = true
  }
  while true {
    while at < text.len() and (at_byte(text, at) == 59 or is_space(at_byte(text, at))) { at += 1 }
    if at >= text.len() { break }
    var command = blank_command(0)
    let first_scan = parse_address(chunk, at, options)?
    at = first_scan.next
    if first_scan.found {
      if let address = first_scan.address {
        match address {
          Plus(_) => { return Err(complain(chunk, at, "invalid usage of +N or ~N as first address")) }
          Multiple(_) => { return Err(complain(chunk, at, "invalid usage of +N or ~N as first address")) }
          _ => {}
        }
      }
      command = {...command, first: first_scan.address}
      at = skip_blanks(text, at)
      if at_byte(text, at) == 44 {
        let last_scan = parse_address(chunk, skip_blanks(text, at + 1), options)?
        if !last_scan.found { return Err(complain(chunk, consumed_through(text, last_scan.next), "unexpected ','")) }
        command = {...command, last: last_scan.address}
        at = skip_blanks(text, last_scan.next)
      }
      if command.first == .Line(0) {
        var regex_end = false
        if let last = command.last {
          match last { Pattern(_) => regex_end = true, _ => {} }
        }
        if !regex_end { return Err(complain(chunk, consumed_through(text, at), "invalid usage of line address 0")) }
      }
    }
    if at_byte(text, at) == 33 {
      command = {...command, invert: true}
      at = skip_blanks(text, at + 1)
      if at_byte(text, at) == 33 { return Err(complain(chunk, at + 1, "multiple '!'s")) }
    }
    let op = at_byte(text, at)
    if op < 0 { return Err(complain(chunk, at, "missing command")) }
    at += 1
    command = {...command, op: op}
    if op == 35 {
      if command.first != null { return Err(complain(chunk, at, "comments don't accept any addresses")) }
      while at < text.len() and at_byte(text, at) != NEWLINE { at += 1 }
      continue
    }
    if op == 118 {
      let operand = read_label(text, at)
      at = operand.next
      if operand.name != "" {
        let parts = operand.name.split(".")
        var newer = false
        let wanted = [parts[0].parse_int() ?? 0, if parts.len() > 1 { parts[1].parse_int() ?? 0 } else { 0 }]
        if wanted[0] > 4 or (wanted[0] == 4 and wanted[1] > 10) { newer = true }
        if newer { return Err(complain(chunk, at, "expected newer version of sed")) }
      }
      continue
    }
    if op == 123 {
      blocks += [{index: commands.len(), where: place(chunk, 0)}]
      commands += [command]
      continue
    }
    if op == 125 {
      if blocks.is_empty() { return Err(complain(chunk, at, "unexpected '}'")) }
      if command.first != null { return Err(complain(chunk, at, "'}' doesn't want any addresses")) }
      at = end_of_command(chunk, at)?
      let opened = blocks[blocks.len() - 1]
      blocks = blocks[0..blocks.len() - 1]
      commands[opened.index] = {...commands[opened.index], number: commands.len()}
      commands += [command]
      continue
    }
    if op in [97, 105, 99] {
      let start = skip_blanks(text, at)
      if start >= text.len() { return Err(complain(chunk, start, "expected \\ after 'a', 'c' or 'i'")) }
      var begin = start
      if at_byte(text, start) == 92 {
        let after = start + 1
        if after >= text.len() {
          commands += [command]
          pending = commands.len() - 1
          at = after
          continue
        }
        if at_byte(text, after) == NEWLINE { begin = after + 1 } else { begin = after }
      }
      let scan = read_text(chunk, begin)?
      commands += [{...command, text: scan.text}]
      if scan.open { pending = commands.len() - 1 }
      at = scan.next
      continue
    }
    if op == 58 {
      if command.first != null { return Err(complain(chunk, at, ": doesn't want any addresses")) }
      let operand = read_label(text, at)
      at = operand.next
      if operand.name == "" { return Err(complain(chunk, at, "\":\" lacks a label")) }
      labels = labels.set(operand.name, commands.len())
      commands += [{...command, label: operand.name}]
      continue
    }
    if op in [98, 116, 84] {
      let operand = read_label(text, at)
      at = operand.next
      commands += [{...command, label: operand.name}]
      continue
    }
    if op in [114, 82, 119, 87] {
      if options.sandbox { return Err(complain(chunk, at, "e/r/w commands disabled in sandbox mode")) }
      let operand = read_file_name(text, at)
      at = operand.next
      if operand.name == "" { return Err(complain(chunk, at, "missing filename in r/R/w/W commands")) }
      if op in [119, 87] { opened = open_output(opened, operand.name) }
      commands += [{...command, name: operand.name}]
      continue
    }
    if op in [113, 81] {
      if command.last != null { return Err(complain(chunk, at, "command only uses one address")) }
    }
    if op in [113, 81, 108, 76] {
      let digit_at = skip_blanks(text, at)
      var number = -1
      if is_digit(at_byte(text, digit_at)) {
        let scan = read_integer(text, digit_at)
        number = scan.value
        at = scan.next
      }
      command = {...command, number: number}
      at = end_of_command(chunk, at)?
      commands += [command]
      continue
    }
    if op == 101 {
      if options.sandbox { return Err(complain(chunk, at, "e/r/w commands disabled in sandbox mode")) }
      let start = skip_blanks(text, at)
      if start >= text.len() or at_byte(text, start) == NEWLINE {
        at = start
        commands += [command]
        continue
      }
      let scan = read_text(chunk, start)?
      commands += [{...command, text: scan.text}]
      at = scan.next
      continue
    }
    if op == 115 {
      let slash = at_byte(text, at)
      if slash < 0 { return Err(complain(chunk, at, "unterminated 's' command")) }
      at += 1
      let pattern_scan = match_slash(text, at, slash, true)
      if !pattern_scan.ok { return Err(complain(chunk, pattern_scan.next, "unterminated 's' command")) }
      let replacement_scan = match_slash(text, pattern_scan.next, slash, false)
      if !replacement_scan.ok { return Err(complain(chunk, replacement_scan.next, "unterminated 's' command")) }
      at = replacement_scan.next
      let replacement_text = normalize(replacement_scan.text, .ReplacementText)?
      var global = false
      var print = 0
      var evaluate = false
      var occurrence = 0
      var ignore_case = false
      var multiline = false
      var write: Str? = null
      var done = false
      while !done {
        let flag = at_byte(text, at)
        at += 1
        if flag == 105 or flag == 73 { ignore_case = true } else if flag == 109 or flag == 77 { multiline = true } else if flag == 101 {
          if options.sandbox { return Err(complain(chunk, at, "e/r/w commands disabled in sandbox mode")) }
          evaluate = true
        } else if flag == 112 {
          if print != 0 { return Err(complain(chunk, at, "multiple 'p' options to 's' command")) }
          print = if evaluate { 2 } else { 1 }
        } else if flag == 103 {
          if global { return Err(complain(chunk, at, "multiple 'g' options to 's' command")) }
          global = true
        } else if flag == 119 {
          if options.sandbox { return Err(complain(chunk, at, "e/r/w commands disabled in sandbox mode")) }
          let operand = read_file_name(text, at)
          at = operand.next
          if operand.name == "" { return Err(complain(chunk, at, "missing filename in r/R/w/W commands")) }
          opened = open_output(opened, operand.name)
          write = operand.name
          done = true
        } else if is_digit(flag) {
          if occurrence != 0 { return Err(complain(chunk, at, "multiple number options to 's' command")) }
          let scan = read_integer(text, at - 1)
          occurrence = scan.value
          at = scan.next
          if occurrence == 0 { return Err(complain(chunk, at, "number option to 's' command may not be zero")) }
        } else if flag == 125 or flag == 35 {
          at -= 1
          done = true
        } else if flag == NEWLINE or flag == 59 {
          done = true
        } else if flag < 0 {
          at -= 1
          done = true
        } else if is_blank(flag) {
        } else if flag == 13 and at_byte(text, at) == NEWLINE {
          at += 1
          done = true
        } else {
          return Err(complain(chunk, at, "unknown option to 's'"))
        }
      }
      let pattern_text = normalize(pattern_scan.text, .RegexText)?
      var choice: RegexChoice = .Previous
      var groups = -1
      if pattern_text.is_empty() {
        if ignore_case or multiline { return Err(complain(chunk, at, "cannot specify modifiers on empty regexp")) }
      } else {
        choice = match compile_regex(pattern_text, options.extended, ignore_case, multiline and !options.null_data) {
          Ok(value) => value
          Err(failure) => { return Err(complain(chunk, at, failure.message)) }
        }
        match choice { Explicit(spec) => groups = spec.groups, _ => {} }
      }
      let nodes = parse_replacement(replacement_text)
      let highest = highest_group(nodes)
      if groups >= 0 and highest > groups {
        return Err(complain(chunk, at, f"invalid reference \\{highest} on 's' command's RHS"))
      }
      let substitution: Substitution = {regex: choice, nodes: nodes, global: global, occurrence: occurrence, print: print, write: write, evaluate: evaluate}
      commands += [{...command, sub: substitution}]
      continue
    }
    if op == 121 {
      let slash = at_byte(text, at)
      if slash < 0 { return Err(complain(chunk, at, "unterminated 'y' command")) }
      at += 1
      let source_scan = match_slash(text, at, slash, false)
      if !source_scan.ok { return Err(complain(chunk, source_scan.next, "unterminated 'y' command")) }
      let dest_scan = match_slash(text, source_scan.next, slash, false)
      if !dest_scan.ok { return Err(complain(chunk, dest_scan.next, "unterminated 'y' command")) }
      at = dest_scan.next
      let source = translate_operand(normalize(source_scan.text, .LiteralText)?)
      let dest = translate_operand(normalize(dest_scan.text, .LiteralText)?)
      if source.len() != dest.len() { return Err(complain(chunk, at, "'y' command strings have different lengths")) }
      var table = [index for index in range(256)]
      for index in range(source.len()) { table[source[index]] = dest[index] }
      at = end_of_command(chunk, at)?
      commands += [{...command, table: table}]
      continue
    }
    if op in [61, 100, 68, 70, 103, 71, 104, 72, 110, 78, 112, 80, 122, 120] {
      at = end_of_command(chunk, at)?
      commands += [command]
      continue
    }
    return Err(complain(chunk, at, f"unknown command: '{text[at - 1..at].utf8() ?? "?"}'"))
  }
  Ok({commands: commands, blocks: blocks, labels: labels, pending: pending, first: false, quiet: quiet, opened: opened})
}

## Compile `-e` and `-f` chunks into a program. Syntax failures carry GNU's
## location prefix.
export proc compile(chunks: List[Chunk], options: Options) [fs, io, error, process, env] -> Result[Parser, Error] {
  var state: Parser = {commands: [], blocks: [], labels: {}, pending: -1, first: true, quiet: false, opened: []}
  for chunk in chunks { state = parse_chunk(state, chunk, options)? }
  if !state.blocks.is_empty() {
    return Err(SedError.Invalid(f"{state.blocks[0].where}: unmatched '{{'"))
  }
  Ok(state)
}

## Resolve every branch to its target index. An unknown label is a separate
## failure because GNU ends the run with a different status for it.
export pure link(state: Parser) -> Result[Program, Error] {
  var commands = state.commands
  for index in range(commands.len()) {
    let command = commands[index]
    if command.op in [98, 116, 84] {
      if command.label == "" {
        commands[index] = {...command, number: commands.len()}
      } else if command.label in state.labels {
        commands[index] = {...command, number: state.labels[command.label]}
      } else {
        return Err(SedError.Invalid(f"can't find label for jump to '{command.label}'"))
      }
    }
  }
  Ok({commands: commands, quiet: state.quiet})
}

# ---- Execution ----

type Space = {text: Bytes, chomped: Bool}
# A numeric-start range that has finished stays Closed; other ranges restart.
enum RangeState { Inactive, Active, Closed }
type Selection = {matched: Bool, state: RangeState, end: Int, last_regex: RegexSpec?}
# Queued by a, r, and R and written when the next input line is read: KIND 0 is
# literal text, 1 the contents of file NAME, 2 one raw line read by R.
type QueueItem = {kind: Int, data: Bytes, name: Str}
type Outputs = {bufs: List[List[Bytes]], missing: List[Bool]}
# Where the stream stands in its input files. The records of the current file
# live beside the cursor, not inside it, because a record holding a long list
# is copied whenever it is passed or updated.
type Cursor = {next_name: Int, position: Int, count: Int, trailing: Bool, current: Str, line: Int, bad: Int, fatal: Bool}
type Loaded = {cursor: Cursor, records: List[Bytes]}
type Machine = {hold: Space, out: Outputs, last_regex: RegexSpec?, status: Int, quit: Bool, bad: Int, rdata: Map[Bytes], roffset: Map[Int]}
# FILE_TARGETS maps each command to its w-file output target (-1 if none).
type Context = {program: Program, options: Options, in_place: Bool, delim: Int, delim_bytes: Bytes, targets: List[Int], file_names: List[Str]}

# Output targets: the main output (stdout, or the in-place file), the real
# stdout and stderr named by `/dev/stdout` and `/dev/stderr`, and `w` files.
const MAIN = 0
const STDOUT_FILE = 1
const STDERR_FILE = 2
const FIRST_FILE = 3

pure buffer_of(target: Int, in_place: Bool) -> Int {
  if target == MAIN { return if in_place { 1 } else { 0 } }
  if target == STDOUT_FILE { return 0 }
  target
}

# Write one record. A record without its trailing newline is remembered so a
# later write to the same target first supplies the missing newline.
pure emit(out: Outputs, target: Int, in_place: Bool, text: Bytes, newline: Bool, delim: Bytes) -> Outputs {
  let index = buffer_of(target, in_place)
  var parts = out.bufs[index]
  var missing = out.missing
  if missing[target] { parts += [delim]; missing[target] = false }
  parts += [text]
  if newline { parts += [delim] } else { missing[target] = true }
  # Output kept until the end (in-place and `w` files) is merged in chunks so
  # that appending stays cheap for long inputs.
  if index != 0 and parts.len() > 128 { parts = [bytes.concat(parts)] }
  var bufs = out.bufs
  bufs[index] = parts
  {bufs: bufs, missing: missing}
}

# Write bytes verbatim after completing an unterminated record.
pure emit_raw(out: Outputs, target: Int, in_place: Bool, text: Bytes, delim: Bytes) -> Outputs {
  let index = buffer_of(target, in_place)
  var parts = out.bufs[index]
  var missing = out.missing
  if missing[target] { parts += [delim]; missing[target] = false }
  parts += [text]
  if index != 0 and parts.len() > 128 { parts = [bytes.concat(parts)] }
  var bufs = out.bufs
  bufs[index] = parts
  {bufs: bufs, missing: missing}
}

pure complete_record(out: Outputs, target: Int, in_place: Bool, delim: Bytes) -> Outputs {
  emit_raw(out, target, in_place, b"", delim)
}

pure octal(value: Int) -> Str { f"\\{value / 64}{value / 8 % 8}{value % 8}" }

# The `l` rendering: escapes, `$` at the end, and `\` line wraps so that no
# output row is longer than WIDTH characters (0 never wraps).
pure list_text(text: Bytes, width: Int, delim: Bytes) -> Bytes {
  var parts: List[Bytes] = []
  var column = 0
  for index in range(text.len()) {
    let byte = at_byte(text, index)
    var piece = text[index..index + 1]
    if byte == 92 { piece = b"\\\\" } else if byte == 7 { piece = b"\\a" } else if byte == 8 { piece = b"\\b" } else if byte == 12 { piece = b"\\f" } else if byte == 10 { piece = b"\\n" } else if byte == 13 { piece = b"\\r" } else if byte == 9 { piece = b"\\t" } else if byte == 11 { piece = b"\\v" } else if byte < 32 or byte > 126 { piece = bytes.from_text(octal(byte)) }
    if width > 0 and column + piece.len() > width - 1 {
      parts += [b"\\", delim]
      column = 0
    }
    parts += [piece]
    column += piece.len()
  }
  parts += [b"$", delim]
  bytes.concat(parts)
}

pure contains_byte(input: Bytes, code: Int) -> Bool {
  for at in range(input.len()) { return true when input.byte_at(at) == code }
  false
}

# The host matcher works on C strings and cannot see NUL. A subject holding NUL
# is matched as a copy where each NUL is one control byte absent from both the
# pattern and the text; the copy keeps every offset.
pure regex_subject(pattern: Str, text: Bytes) -> Result[Bytes, Error] {
  if !contains_byte(text, 0) { return Ok(text) }
  let pattern_bytes = bytes.from_text(pattern)
  var stand_in = -1
  for code in range(1, 32) {
    if code != 10 and !contains_byte(pattern_bytes, code) and !contains_byte(text, code) {
      stand_in = code
      break
    }
  }
  if stand_in < 0 { return Err(SedError.Invalid("no control byte is free to stand in for NUL")) }
  var out: List[Bytes] = []
  for at in range(text.len()) {
    out += [if text.byte_at(at) == 0 { bytes.from_ints([stand_in])? } else { text[at..at + 1] }]
  }
  Ok(bytes.concat(out))
}

# Whether the pattern itself spells a newline outside any bracket expression.
pure newline_outside_brackets(pattern: Bytes) -> Bool {
  var at = 0
  while at < pattern.len() {
    let byte = at_byte(pattern, at)
    if byte == NEWLINE { return true }
    if byte == 92 { at += 2; continue }
    if byte == 91 {
      let close = bracket_end(pattern, at, false)
      if close >= 0 {
        at = close
        continue
      }
    }
    at += 1
  }
  false
}

# First match at or after START. A multiline regex is searched line by line so
# that `^` and `$` hold at every line boundary; a pattern that spells a newline
# itself is searched over the whole buffer instead.
pure find_match(spec: RegexSpec, text: Bytes, start: Int, extended: Bool) -> Result[List[Capture?], Error] {
  let subject = regex_subject(spec.pattern, text)?
  if !spec.multiline or newline_outside_brackets(bytes.from_text(spec.pattern)) {
    return regex.captures_bytes(spec.pattern, subject, start, extended, spec.ignore_case)
  }
  var line_start = if start > subject.len() { subject.len() } else { start }
  while line_start > 0 and at_byte(subject, line_start - 1) != NEWLINE { line_start -= 1 }
  while true {
    var line_end = line_start
    while line_end < subject.len() and at_byte(subject, line_end) != NEWLINE { line_end += 1 }
    let allowed = !(spec.buffer_start and line_start != 0) and !(spec.buffer_end and line_end != subject.len())
    if allowed {
      let offset = if start > line_start { start - line_start } else { 0 }
      let found: List[Capture?] = regex.captures_bytes(spec.pattern, subject[line_start..line_end], offset, extended, spec.ignore_case)?
      if !found.is_empty() {
        var shifted: List[Capture?] = []
        for item in found {
          if let span = item { shifted += [{start: span.start + line_start, end: span.end + line_start}] } else { shifted += [null] }
        }
        return Ok(shifted)
      }
    }
    if line_end >= subject.len() { break }
    line_start = line_end + 1
  }
  Ok([])
}

pure resolve_regex(choice: RegexChoice, last: RegexSpec?) -> Result[RegexSpec, Error] {
  match choice {
    Explicit(spec) => Ok(spec)
    Previous => {
      guard let spec = last else { return Err(SedError.Invalid("-e expression #1, char 0: no previous regular expression")) }
      Ok(spec)
    }
  }
}

type Hit = {matched: Bool, last_regex: RegexSpec?}

pure address_hit(address: Address, line_no: Int, is_last: Bool, text: Bytes, last_regex: RegexSpec?, extended: Bool) -> Result[Hit, Error] {
  match address {
    Line(value) => Ok({matched: line_no == value, last_regex: last_regex})
    Stride(first, step) => Ok({matched: line_no >= first and (line_no - first) % step == 0, last_regex: last_regex})
    Last => Ok({matched: is_last, last_regex: last_regex})
    Pattern(choice) => {
      let spec = resolve_regex(choice, last_regex)?
      let found = find_match(spec, text, 0, extended)?
      Ok({matched: !found.is_empty(), last_regex: spec})
    }
    Plus(_) => Ok({matched: false, last_regex: last_regex})
    Multiple(_) => Ok({matched: false, last_regex: last_regex})
  }
}

pure after_range(first: Address) -> RangeState {
  match first {
    Line(_) => .Closed
    _ => .Inactive
  }
}

# GNU's match_address_p: whether COMMAND applies to this line, and the range
# state it leaves behind. A numeric start opens its range at the first line at
# or past it; a numeric end at or before the start line selects one line. A
# line-number end stops matching once passed, but a `+N` or `~N` end still
# matches the first line seen past it (lines may have been consumed by n or N).
pure select(command: Instruction, state: RangeState, end: Int, line_no: Int, is_last: Bool, text: Bytes, last_regex: RegexSpec?, extended: Bool) -> Result[Selection, Error] {
  var saved = last_regex
  guard let first = command.first else { return Ok({matched: true, state: state, end: end, last_regex: saved}) }
  guard let last = command.last else {
    let found = address_hit(first, line_no, is_last, text, saved, extended)?
    return Ok({matched: found.matched, state: state, end: end, last_regex: found.last_regex})
  }
  var current = state
  if current == .Active {
    var closes = false
    var matched = true
    match last {
      Line(_) => {
        closes = line_no >= end
        matched = line_no <= end
      }
      Plus(_) => closes = line_no >= end
      Multiple(_) => closes = line_no >= end
      _ => {
        let found = address_hit(last, line_no, is_last, text, saved, extended)?
        closes = found.matched
        saved = found.last_regex
      }
    }
    let next: RangeState = if closes { after_range(first) } else { .Active }
    return Ok({matched: matched, state: next, end: end, last_regex: saved})
  }
  var starts = false
  match first {
    Line(value) => starts = current != .Closed and line_no >= value
    _ => {
      let found = address_hit(first, line_no, is_last, text, saved, extended)?
      starts = found.matched
      saved = found.last_regex
    }
  }
  if !starts { return Ok({matched: false, state: current, end: end, last_regex: saved}) }
  var stop = 0
  var single = false
  match last {
    Line(value) => {
      stop = value
      single = value <= line_no
    }
    Plus(value) => {
      stop = line_no + value
      single = value == 0
    }
    Multiple(value) => {
      stop = if value > 0 { (line_no / value + 1) * value } else { line_no }
      single = value <= 0
    }
    _ => {}
  }
  # A numeric start already passed (its line was consumed by n, N, or d) fires
  # once on the first line past it when the end line is also behind. GNU 4.10
  # selects only the lines up to the end line; the pinned BusyBox suite
  # ("sed with N skipping lines past ranges on next cmds", "sed 2d;2,1p")
  # requires the once-only match.
  if single { return Ok({matched: true, state: after_range(first), end: end, last_regex: saved}) }
  Ok({matched: true, state: .Active, end: stop, last_regex: saved})
}

# Case conversion for a replacement piece. KIND carries a persistent mode (1
# lower, 2 upper) plus one-shot flags (4 upper-first, 8 lower-first).
pure convert_case(text: Bytes, kind: Int) -> Bytes {
  if kind == 0 or text.is_empty() { return text }
  let mode = kind % 4
  let upper_first = kind / 4 % 2 == 1
  let lower_first = kind / 8 % 2 == 1
  var out: List[Int] = []
  for index in range(text.len()) {
    let byte = at_byte(text, index)
    var value = byte
    if index == 0 and upper_first { value = upper_byte(byte) } else if index == 0 and lower_first { value = lower_byte(byte) } else if mode == MODE_UPPER { value = upper_byte(byte) } else if mode == MODE_LOWER { value = lower_byte(byte) }
    out += [value]
  }
  bytes.from_ints(out) ?? text
}

# GNU's append_replacement: a pending `\u` or `\l` left over from an empty group
# applies to the next piece that has no modifier of its own.
pure expand_replacement(nodes: List[ReplNode], text: Bytes, captures: List[Capture?]) -> Bytes {
  var out: List[Bytes] = []
  var carried = 0
  for node in nodes {
    var kind = if node.kind >= 4 { node.kind } else { node.kind + carried }
    carried = 0
    if !node.text.is_empty() {
      out += [convert_case(node.text, kind)]
      kind = kind % 4
    }
    if node.group >= 0 {
      var span: Capture? = null
      if node.group < captures.len() { span = captures[node.group] }
      if let found = span {
        if found.start == found.end {
          if node.kind >= 4 { carried = node.kind - node.kind % 4 }
        } else {
          out += [convert_case(text[found.start..found.end], kind)]
        }
      } else if node.kind >= 4 {
        carried = node.kind - node.kind % 4
      }
    }
  }
  bytes.concat(out)
}

# Apply `s`: every match is visited so occurrence counting and empty matches
# behave like GNU's loop. An empty match directly after the previous match is
# skipped, and an empty match advances one byte.
pure substitute(spec: RegexSpec, text: Bytes, sub: Substitution, extended: Bool) -> Result[Replacement, Error] {
  var out: List[Bytes] = []
  var search = 0
  var copied = 0
  var count = 0
  var changed = false
  var previous_end = -1
  while search <= text.len() {
    let captures = find_match(spec, text, search, extended)?
    if captures.is_empty() { break }
    guard let whole = captures[0] else { return Err(SedError.Invalid("missing whole-match capture")) }
    let begin = whole.start
    let end = whole.end
    if begin == end and previous_end == begin {
      if end == text.len() { break }
      search = end + 1
      continue
    }
    count += 1
    let selected = if sub.occurrence == 0 { sub.global or count == 1 } else { count == sub.occurrence or (sub.global and count > sub.occurrence) }
    if selected {
      changed = true
      out += [text[copied..begin]]
      out += [expand_replacement(sub.nodes, text, captures)]
      copied = end
      if !sub.global { break }
    }
    previous_end = end
    if begin == end {
      if end == text.len() { break }
      search = end + 1
    } else { search = end }
  }
  out += [text[copied..]]
  Ok({text: bytes.concat(out), changed: changed})
}

# `lines()` also swallows a carriage return before each newline; when the
# line lengths do not account for every byte, fall back to the byte scan.
pure split_records(data: Bytes, delim: Int) -> List[Bytes] {
  if delim == NEWLINE {
    let lines = data.lines()
    var covered = data.count_lines()
    for item in lines { covered += item.len() }
    if covered == data.len() or (covered == data.len() - 1 and !data.ends_with(b"\n")) { return lines }
  }
  var records: List[Bytes] = []
  var start = 0
  for at in range(data.len()) {
    if data.byte_at(at) == delim {
      records += [data[start..at]]
      start = at + 1
    }
  }
  if start < data.len() { records += [data[start..]] }
  records
}

# Open the next named input. A missing or unreadable file is reported and
# skipped (status 2 at exit); a directory ends the run.
proc open_next(cursor: Cursor, name: Str, delim: Int) [fs, io, error, process, env] -> Loaded {
  let next: Cursor = {...cursor, next_name: cursor.next_name + 1, position: 0, count: 0}
  if name == "" {
    gnu.error("can't read : No such file or directory")
    return {cursor: {...next, bad: next.bad + 1}, records: []}
  }
  match gnu.read_operand(name) {
    Ok(data) => {
      let records = split_records(data, delim)
      return {cursor: {...next, count: records.len(), trailing: data.is_empty() or data.byte_at(data.len() - 1) == delim, current: name}, records: records}
    }
    Err(failure) => {
      if gnu.errno(failure) == 21 {
        gnu.error(f"read error on {name}: Is a directory")
        return {cursor: {...next, fatal: true}, records: []}
      }
      gnu.error(f"can't read {name}: {gnu.strerror(failure)}")
      return {cursor: {...next, bad: next.bad + 1}, records: []}
    }
  }
}

# Whether the current file is exhausted while another input remains to open.
pure needs_file(cursor: Cursor, names: Int) -> Bool {
  cursor.position >= cursor.count and cursor.next_name < names and !cursor.fatal
}

# Write buffered standard output.
proc flush_stdout(m: Machine) [io, error, process, env] -> Machine {
  var bufs = m.out.bufs
  if !bufs[0].is_empty() {
    gnu.write_bytes(bytes.concat(bufs[0]))
    bufs[0] = []
  }
  if !bufs[2].is_empty() {
    let text = bytes.concat(bufs[2]).utf8() ?? ""
    let _ = io.write_stderr(text)
    let _ = io.flush_stderr()
    bufs[2] = []
  }
  {...m, out: {bufs: bufs, missing: m.out.missing}}
}

# Create every `w` file when the program starts and write the collected
# records out when it ends, as GNU's immediate writes would leave them.
proc write_files(m: Machine, ctx: Context) [fs, error, process, env] -> Unit {
  for index in range(ctx.file_names.len()) {
    let data = bytes.concat(m.out.bufs[FIRST_FILE + index])
    match fp"{ctx.file_names[index]}".write(data) {
      Err(failure) => {
        gnu.error(f"couldn't flush <unknown>: {gnu.strerror(failure)}")
        exit 4
      }
      Ok(_) => {}
    }
  }
}

proc shutdown(m: Machine, ctx: Context, status: Int) [fs, io, error, process, env] -> Unit {
  let flushed = flush_stdout(m)
  write_files(flushed, ctx)
  exit status
}

# Dump queued appends in order: text, whole files for `r`, raw lines for `R`.
proc flush_queue(m: Machine, queue: List[QueueItem], ctx: Context) [fs, io, error, process, env] -> Machine {
  var out = m.out
  for item in queue {
    if item.kind == 1 {
      var data: Bytes = b""
      if item.name == "/dev/stdin" {
        data = io.stdin_bytes() ?? b""
      } else {
        match fp"{item.name}".read_bytes() {
          Ok(content) => data = content
          Err(failure) => {
            if gnu.errno(failure) == 21 {
              gnu.error(f"read error on {item.name}: Is a directory")
              shutdown({...m, out: out}, ctx, 4)
            }
          }
        }
      }
      out = emit_raw(out, MAIN, ctx.in_place, data, ctx.delim_bytes)
    } else {
      out = emit_raw(out, MAIN, ctx.in_place, item.data, ctx.delim_bytes)
    }
  }
  {...m, out: out}
}

type ReadLine = {machine: Machine, data: Bytes, failed: Bool}

# a/i/c text always ends in a newline; `i` and `c` write it as the record
# delimiter, so `-z` output stays NUL separated.
pure delimited_text(text: Bytes, delim: Bytes) -> Bytes {
  if text.is_empty() or delim == b"\n" { return text }
  bytes.concat([text[0..text.len() - 1], delim])
}

pure strip_delimiter(text: Bytes, delim: Int) -> Bytes {
  if !text.is_empty() and text.byte_at(text.len() - 1) == delim { return text[0..text.len() - 1] }
  text
}

# Run COMMAND with the shell and return its standard output; its standard error
# stays the caller's, as with popen.
proc shell_output(command: Bytes) [process, io, error] -> Bytes {
  let text = command.utf8() ?? ""
  let captured = run.capture --bytes /bin/sh -c $text
  let _ = io.write_stderr(captured.stderr.utf8() ?? "")
  captured.stdout
}

# One `R` line: raw bytes through the next delimiter, remembered per file.
proc read_line_from(m: Machine, name: Str, delim: Int) [fs, io, env] -> ReadLine {
  var rdata = m.rdata
  var roffset = m.roffset
  if name not in rdata {
    var data: Bytes = b""
    if name == "/dev/stdin" {
      data = io.stdin_bytes() ?? b""
    } else {
      match fp"{name}".read_bytes() {
        Ok(content) => data = content
        Err(failure) => {
          if gnu.errno(failure) == 21 { return {machine: m, data: b"", failed: true} }
        }
      }
    }
    rdata = rdata.set(name, data)
    roffset = roffset.set(name, 0)
  }
  let data = rdata[name]
  let offset = roffset[name]
  if offset >= data.len() { return {machine: {...m, rdata: rdata, roffset: roffset}, data: b"", failed: false} }
  var end = offset
  while end < data.len() and data.byte_at(end) != delim { end += 1 }
  let stop = if end < data.len() { end + 1 } else { end }
  roffset = roffset.set(name, stop)
  {machine: {...m, rdata: rdata, roffset: roffset}, data: data[offset..stop], failed: false}
}

# A run-time error from the matcher or a missing previous regex.
proc runtime_failure(m: Machine, ctx: Context, failure: Error) [fs, io, error, process, env] -> Unit {
  gnu.error(failure.message)
  shutdown(m, ctx, 1)
}

# Run the program over NAMES as one stream; hold space, outputs, and the last
# regex persist in the machine across streams (separate files).
proc run_stream(machine: Machine, names: List[Str], ctx: Context) [fs, io, error, process, env] -> Machine {
  var m = machine
  var cursor: Cursor = {next_name: 0, position: 0, count: 0, trailing: true, current: "-", line: 0, bad: 0, fatal: false}
  var records: List[Bytes] = []
  let name_count = names.len()
  let commands = ctx.program.commands
  let total = commands.len()
  let extended = ctx.options.extended
  let delim = ctx.delim
  let in_place = ctx.in_place
  var ranges: List[RangeState] = []
  for command in commands {
    let initial: RangeState = if command.first == .Line(0) { .Active } else { .Inactive }
    ranges += [initial]
  }
  var ends: List[Int] = [0 for command in commands]
  if ctx.options.separate { m = {...m, roffset: {}, rdata: {}, hold: {text: b"", chomped: true}} }
  var queue: List[QueueItem] = []
  var line: Space = {text: b"", chomped: true}
  var replaced = false
  var restart = false
  while true {
    if !restart {
      if !queue.is_empty() {
        m = flush_queue(m, queue, ctx)
        queue = []
      }
      replaced = false
      while needs_file(cursor, name_count) {
        let loaded = open_next(cursor, names[cursor.next_name], delim)
        cursor = loaded.cursor
        records = loaded.records
      }
      if cursor.fatal {
        m = {...m, bad: m.bad + cursor.bad}
        shutdown(m, ctx, 4)
      }
      if cursor.position >= cursor.count { break }
      line = {text: records[cursor.position], chomped: cursor.position + 1 < cursor.count or cursor.trailing}
      cursor = {...cursor, position: cursor.position + 1, line: cursor.line + 1}
    }
    restart = false
    var autoprint = !ctx.program.quiet
    var pc = 0
    var ended = false
    while pc < total and !ended {
      let index = pc
      let command = commands[index]
      pc += 1
      var selected = true
      if command.first != null {
        var is_last = false
        let needs_last = command.first == .Last or command.last == .Last
        if needs_last {
          while needs_file(cursor, name_count) {
            let loaded = open_next(cursor, names[cursor.next_name], delim)
            cursor = loaded.cursor
            records = loaded.records
          }
          if cursor.fatal {
            m = {...m, bad: m.bad + cursor.bad}
            shutdown(m, ctx, 4)
          }
          is_last = cursor.position >= cursor.count
        }
        match select(command, ranges[index], ends[index], cursor.line, is_last, line.text, m.last_regex, extended) {
          Ok(chosen) => {
            ranges[index] = chosen.state
            ends[index] = chosen.end
            m = {...m, last_regex: chosen.last_regex}
            selected = chosen.matched
          }
          Err(failure) => runtime_failure(m, ctx, failure)
        }
      }
      if command.invert { selected = !selected }
      if !selected {
        if command.op == 123 { pc = command.number + 1 }
        continue
      }
      match command.op {
        97 => queue += [{kind: 0, data: command.text, name: ""}]
        105 => m = {...m, out: emit_raw(m.out, MAIN, in_place, delimited_text(command.text, ctx.delim_bytes), ctx.delim_bytes)}
        99 => {
          if ranges[index] != .Active or command.last == null {
            m = {...m, out: emit_raw(m.out, MAIN, in_place, delimited_text(command.text, ctx.delim_bytes), ctx.delim_bytes)}
          }
          autoprint = false
          ended = true
        }
        100 => {
          autoprint = false
          ended = true
        }
        68 => {
          var newline = -1
          for at in range(line.text.len()) {
            if line.text.byte_at(at) == delim { newline = at; break }
          }
          autoprint = false
          ended = true
          if newline >= 0 {
            line = {...line, text: line.text[newline + 1..]}
            restart = true
          }
        }
        101 => {
          if command.text.is_empty() {
            line = {...line, text: strip_delimiter(shell_output(line.text), delim)}
          } else {
            let output = shell_output(strip_delimiter(command.text, NEWLINE))
            m = {...m, out: emit_raw(m.out, MAIN, in_place, output, ctx.delim_bytes)}
          }
        }
        70 => m = {...m, out: emit_raw(m.out, MAIN, in_place, bytes.concat([bytes.from_text(cursor.current), ctx.delim_bytes]), ctx.delim_bytes)}
        103 => line = m.hold
        71 => line = {text: bytes.concat([line.text, ctx.delim_bytes, m.hold.text]), chomped: m.hold.chomped}
        104 => m = {...m, hold: line}
        72 => m = {...m, hold: {text: bytes.concat([m.hold.text, ctx.delim_bytes, line.text]), chomped: line.chomped}}
        120 => {
          let saved = m.hold
          m = {...m, hold: line}
          line = saved
        }
        108 => {
          let width = if command.number >= 0 { command.number } else { ctx.options.line_length }
          m = {...m, out: emit_raw(m.out, MAIN, in_place, list_text(line.text, width, ctx.delim_bytes), ctx.delim_bytes)}
        }
        110 => {
          if !ctx.program.quiet { m = {...m, out: emit(m.out, MAIN, in_place, line.text, line.chomped, ctx.delim_bytes)} }
          while needs_file(cursor, name_count) {
            let loaded = open_next(cursor, names[cursor.next_name], delim)
            cursor = loaded.cursor
            records = loaded.records
          }
          if cursor.fatal {
            m = {...m, bad: m.bad + cursor.bad}
            shutdown(m, ctx, 4)
          }
          if cursor.position >= cursor.count {
            autoprint = false
            ended = true
          } else {
            if !queue.is_empty() {
              m = flush_queue(m, queue, ctx)
              queue = []
            }
            replaced = false
            line = {text: records[cursor.position], chomped: cursor.position + 1 < cursor.count or cursor.trailing}
            cursor = {...cursor, position: cursor.position + 1, line: cursor.line + 1}
          }
        }
        78 => {
          while needs_file(cursor, name_count) {
            let loaded = open_next(cursor, names[cursor.next_name], delim)
            cursor = loaded.cursor
            records = loaded.records
          }
          if cursor.fatal {
            m = {...m, bad: m.bad + cursor.bad}
            shutdown(m, ctx, 4)
          }
          if cursor.position >= cursor.count {
            if !ctx.program.quiet { m = {...m, out: emit(m.out, MAIN, in_place, line.text, line.chomped, ctx.delim_bytes)} }
            autoprint = false
            ended = true
          } else {
            if !queue.is_empty() {
              m = flush_queue(m, queue, ctx)
              queue = []
            }
            replaced = false
            let joined = bytes.concat([line.text, ctx.delim_bytes, records[cursor.position]])
            line = {text: joined, chomped: cursor.position + 1 < cursor.count or cursor.trailing}
            cursor = {...cursor, position: cursor.position + 1, line: cursor.line + 1}
          }
        }
        112 => m = {...m, out: emit(m.out, MAIN, in_place, line.text, line.chomped, ctx.delim_bytes)}
        80 => {
          var newline = -1
          for at in range(line.text.len()) {
            if line.text.byte_at(at) == delim { newline = at; break }
          }
          if newline >= 0 {
            m = {...m, out: emit(m.out, MAIN, in_place, line.text[0..newline], true, ctx.delim_bytes)}
          } else {
            m = {...m, out: emit(m.out, MAIN, in_place, line.text, line.chomped, ctx.delim_bytes)}
          }
        }
        113 => {
          if !ctx.program.quiet { m = {...m, out: emit(m.out, MAIN, in_place, line.text, line.chomped, ctx.delim_bytes)} }
          m = {...m, out: complete_record(m.out, MAIN, in_place, ctx.delim_bytes)}
          m = flush_queue(m, queue, ctx)
          queue = []
          m = {...m, status: if command.number >= 0 { command.number % 256 } else { 0 }, quit: true}
          autoprint = false
          ended = true
        }
        81 => {
          m = {...m, status: if command.number >= 0 { command.number % 256 } else { 0 }, quit: true}
          autoprint = false
          ended = true
        }
        114 => queue += [{kind: 1, data: b"", name: command.name}]
        82 => {
          let read = read_line_from(m, command.name, delim)
          if read.failed {
            gnu.error(f"read error on {command.name}: Is a directory")
            shutdown(m, ctx, 4)
          }
          m = read.machine
          if !read.data.is_empty() { queue += [{kind: 2, data: read.data, name: ""}] }
        }
        115 => {
          guard let sub = command.sub else { continue }
          match resolve_regex(sub.regex, m.last_regex) {
            Err(failure) => runtime_failure(m, ctx, failure)
            Ok(spec) => {
              m = {...m, last_regex: spec}
              match substitute(spec, line.text, sub, extended) {
                Err(failure) => runtime_failure(m, ctx, failure)
                Ok(result) => {
                  line = {...line, text: result.text}
                  if result.changed {
                    replaced = true
                    if sub.print == 1 { m = {...m, out: emit(m.out, MAIN, in_place, line.text, line.chomped, ctx.delim_bytes)} }
                    if sub.evaluate {
                      let output = shell_output(line.text)
                      line = {...line, text: strip_delimiter(output, delim)}
                    }
                    if sub.print == 2 { m = {...m, out: emit(m.out, MAIN, in_place, line.text, line.chomped, ctx.delim_bytes)} }
                    if ctx.targets[index] >= 0 { m = {...m, out: emit(m.out, ctx.targets[index], in_place, line.text, line.chomped, ctx.delim_bytes)} }
                  }
                }
              }
            }
          }
        }
        116 => {
          if replaced {
            replaced = false
            pc = command.number
          }
        }
        84 => {
          if !replaced { pc = command.number } else { replaced = false }
        }
        98 => pc = command.number
        119 => m = {...m, out: emit(m.out, ctx.targets[index], in_place, line.text, line.chomped, ctx.delim_bytes)}
        87 => {
          var newline = -1
          for at in range(line.text.len()) {
            if line.text.byte_at(at) == delim { newline = at; break }
          }
          if newline >= 0 {
            m = {...m, out: emit(m.out, ctx.targets[index], in_place, line.text[0..newline], true, ctx.delim_bytes)}
          } else {
            m = {...m, out: emit(m.out, ctx.targets[index], in_place, line.text, line.chomped, ctx.delim_bytes)}
          }
        }
        121 => {
          var translated: List[Int] = []
          for at in range(line.text.len()) { translated += [command.table[at_byte(line.text, at)]] }
          line = {...line, text: bytes.from_ints(translated) ?? line.text}
        }
        122 => line = {...line, text: b""}
        61 => m = {...m, out: emit_raw(m.out, MAIN, in_place, bytes.concat([bytes.from_text(f"{cursor.line}"), ctx.delim_bytes]), ctx.delim_bytes)}
        _ => {}
      }
    }
    if autoprint { m = {...m, out: emit(m.out, MAIN, in_place, line.text, line.chomped, ctx.delim_bytes)} }
    if m.out.bufs[0].len() > 256 { m = flush_stdout(m) }
    if m.quit { break }
  }
  if !m.quit and !queue.is_empty() { m = flush_queue(m, queue, ctx) }
  {...m, bad: m.bad + cursor.bad}
}

# Resolve a symlink chain the way `--follow-symlinks` does: each hop reads the
# link until a name that is not a link; the failing name is reported.
proc follow_link(name: Str) [fs] -> Result[Str, Str] {
  var current = name
  var hops = 0
  while hops < 40 {
    match fp"{current}".readlink() {
      Ok(target) => {
        let text = target.display()
        current = if text.starts_with("/") { text } else { f"{parent_prefix(current)}{text}" }
      }
      Err(failure) => {
        if gnu.errno(failure) == 22 { return Ok(current) }
        return Err(f"couldn't readlink {current}: {gnu.strerror(failure)}")
      }
    }
    hops += 1
  }
  Err(f"couldn't readlink {current}: Too many levels of symbolic links")
}

# NAME up to and including its last `/`, or the empty string.
pure parent_prefix(name: Str) -> Str {
  var cursor = 0
  while true {
    let found = name.byte_slice(cursor).find("/") ?? -1
    if found < 0 { break }
    cursor += found + 1
  }
  name.byte_slice(0, cursor)
}

# The directory part of NAME for diagnostics (`./` when there is none).
pure directory_of(name: Str) -> Str {
  let prefix = parent_prefix(name)
  if prefix == "" { "./" } else { prefix }
}

# Replace TARGET with OUTPUT keeping its permission bits, first renaming the
# original to the backup name when a suffix was given.
proc commit_in_place(target: Str, output: Bytes, mode: Int, suffix: Str) [fs, io, error, process, env] -> Unit {
  let destination = fp"{target}"
  let temporary = match fs.temp_sibling(destination) {
    Ok(made) => made
    Err(failure) => {
      gnu.error(f"couldn't open temporary file {directory_of(target)}sedXXXXXX: {gnu.strerror(failure)}")
      exit 4
    }
  }
  match temporary.write(output, mode) {
    Err(failure) => {
      gnu.error(f"couldn't open temporary file {directory_of(target)}sedXXXXXX: {gnu.strerror(failure)}")
      exit 4
    }
    Ok(_) => {}
  }
  if suffix != "" {
    let backup = if "*" in suffix { suffix.replace("*", with: target) } else { f"{target}{suffix}" }
    match destination.rename(to: fp"{backup}", overwrite: true) {
      Err(failure) => {
        let _ = temporary.remove()
        gnu.error(f"cannot rename {target} to {backup}: {gnu.strerror(failure)}")
        exit 4
      }
      Ok(_) => {}
    }
  }
  match temporary.rename(to: destination, overwrite: true) {
    Err(failure) => {
      let _ = temporary.remove()
      gnu.error(f"cannot rename {temporary.display()} to {target}: {gnu.strerror(failure)}")
      exit 4
    }
    Ok(_) => {}
  }
}

## Run a compiled program over NAMES and return the exit status.
export proc execute(program: Program, options: Options, in_place: InPlace, names: List[Str]) [fs, io, error, process, env] -> Int {
  let delim = if options.null_data { 0 } else { NEWLINE }
  # Collect every output file the program names, in first-use order.
  var file_names: List[Str] = []
  var targets: List[Int] = []
  for command in program.commands {
    var name = ""
    if command.op in [119, 87] { name = command.name }
    if command.op == 115 {
      if let sub = command.sub {
        if let file = sub.write { name = file }
      }
    }
    if name == "" {
      targets += [-1]
    } else if name == "/dev/stdout" {
      targets += [STDOUT_FILE]
    } else if name == "/dev/stderr" {
      targets += [STDERR_FILE]
    } else {
      var found = -1
      for index in range(file_names.len()) {
        if file_names[index] == name { found = index }
      }
      if found < 0 {
        file_names += [name]
        found = file_names.len() - 1
      }
      targets += [FIRST_FILE + found]
    }
  }
  let ctx: Context = {program: program, options: options, in_place: in_place.enabled, delim: delim, delim_bytes: bytes.from_ints([delim]) ?? b"\n", targets: targets, file_names: file_names}
  var bufs: List[List[Bytes]] = [[]]
  var missing: List[Bool] = [false]
  while bufs.len() < FIRST_FILE + file_names.len() {
    bufs += [[]]
    missing += [false]
  }
  var m: Machine = {hold: {text: b"", chomped: true}, out: {bufs: bufs, missing: missing}, last_regex: null, status: -1, quit: false, bad: 0, rdata: {}, roffset: {}}
  if in_place.enabled {
    if names.is_empty() {
      gnu.error("no input files")
      exit 4
    }
    for name in names {
      if m.quit { break }
      var target = name
      if in_place.follow {
        match follow_link(name) {
          Ok(resolved) => target = resolved
          Err(message) => {
            gnu.error(message)
            shutdown(m, ctx, 4)
          }
        }
      }
      let source = fp"{target}"
      match fs.stat(source, true) {
        Err(failure) => {
          gnu.error(f"can't read {name}: {gnu.strerror(failure)}")
          m = {...m, bad: m.bad + 1}
          continue
        }
        Ok(info) => {
          if info.kind != "file" {
            gnu.error(f"couldn't edit {name}: not a regular file")
            shutdown(m, ctx, 4)
          }
          match source.read_bytes() {
            Err(failure) => {
              gnu.error(f"can't read {name}: {gnu.strerror(failure)}")
              m = {...m, bad: m.bad + 1}
              continue
            }
            Ok(_) => {}
          }
          var bufs_now = m.out.bufs
          bufs_now[1] = []
          var missing_now = m.out.missing
          missing_now[MAIN] = false
          m = {...m, out: {bufs: bufs_now, missing: missing_now}}
          m = run_stream(m, [target], ctx)
          commit_in_place(target, bytes.concat(m.out.bufs[1]), info.mode % 4096, in_place.suffix)
        }
      }
    }
  } else if options.separate {
    var files = names
    if files.is_empty() { files = ["-"] }
    for name in files {
      if m.quit { break }
      m = run_stream(m, [name], ctx)
    }
  } else {
    m = run_stream(m, if names.is_empty() { ["-"] } else { names }, ctx)
  }
  let flushed = flush_stdout(m)
  write_files(flushed, ctx)
  if m.bad > 0 { return 2 }
  if m.status >= 0 { return m.status }
  0
}
