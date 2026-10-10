##! GNU-compatible basic and extended regular expressions for grep.
##!
##! Patterns are parsed with the grammar and diagnostics of GNU grep: GNU
##! operators (`\<`, `\>`, `\b`, `\B`, `\w`, `\W`, `\s`, `\S`, `\``, `\'`),
##! back-references, `{,n}` intervals, bracket expressions with classes, and
##! the stray-backslash and leading-repetition warnings. Matching is POSIX
##! leftmost-longest over decoded characters, so invalid UTF-8 never matches
##! `.` or a bracket expression and a multibyte character is one unit. The
##! compiled form is a small backtracking program that reports every
##! reachable end position, which `-w` needs to retry shorter matches at the
##! same start.

use search

# Invalid UTF-8 bytes decode to RAW + byte: they match only themselves.
const RAW = 1114112
const DUP_MAX = 32767
const PROGRAM_LIMIT = 40000

const OP_MATCH = 0
const OP_CHAR = 1
const OP_ANY = 2
const OP_SET = 3
const OP_SPLIT = 4
const OP_JMP = 5
const OP_SAVE = 6
const OP_BOL = 7
const OP_EOL = 8
const OP_WORD_START = 9
const OP_WORD_END = 10
const OP_WORD_BOUND = 11
const OP_NOT_WORD_BOUND = 12
const OP_BACKREF = 13
const OP_MARK = 14
const OP_CHECK = 15
const OP_TOO_BIG = 16

const N_EMPTY = 0
const N_CHAR = 1
const N_ANY = 2
const N_SET = 3
const N_CAT = 4
const N_ALT = 5
const N_REPEAT = 6
const N_GROUP = 7
const N_BACKREF = 8
const N_ASSERT = 9

# Token kinds produced while scanning a pattern.
const T_LITERAL = 0
const T_ANY = 1
const T_BRACKET = 2
const T_STAR = 3
const T_PLUS = 4
const T_QMARK = 5
const T_INTERVAL = 6
const T_OPEN = 7
const T_CLOSE = 8
const T_ALT = 9
const T_BOL = 10
const T_EOL = 11
const T_BACKREF = 12
const T_ASSERT = 13
const T_CLASS = 14

const MSG_EBRACK = "Unmatched [, [^, [:, [., or [="
const MSG_ERANGE = "Invalid range end"
const MSG_BADBR = "Invalid content of \\{\\}"
const MSG_EBRACE = "Unmatched \\{"

## A bracket expression. `low` answers characters below 256 directly.
export type CharSet = {negated: Bool, low: List[Bool], singles: List[Int], ranges: List[Int], classes: List[Str]}
type Node = {kind: Int, a: Int, b: Int, kids: List[Int]}
type Inst = {op: Int, x: Int, y: Int}
## One compiled pattern. `literal` is set when the pattern is a plain ASCII
## byte string, so callers may search it natively.
export type Program = {ops: List[Int], xs: List[Int], ys: List[Int], sets: List[CharSet], slots: Int, backrefs: Bool, icase: Bool, utf8: Bool, memo: Bool, filter: Bool, anchored: Bool, fixed: Bool, consuming: Bool, first: List[Bool], literal: Bytes?, native: Str?, native_ere: Bool}
## A line decoded to characters; `offs` maps character index to byte offset
## and is empty when the two coincide.
export type Decoded = {chars: List[Int], offs: List[Int], native: Bool}
## Patterns compiled together, with the warnings GNU grep prints at startup.
export type Compiled = {programs: List[Program], warnings: List[Str]}
type Parsed = {nodes: List[Node], root: Int, sets: List[CharSet], groups: Int, backrefs: Bool, exotic: Bool, warnings: List[Str], error: Str}
type Bracket = {set: CharSet?, next: Int, error: Str, late: Str}
type Interval = {kind: Int, min: Int, max: Int, next: Int, error: Str}
type Frame = {seq: List[Int], alts: List[Int], group: Int, init: List[Int], acc: List[Int]}
type Element = {kind: Int, cp: Int, name: Str, next: Int, error: Str}
type Number = {value: Int, stop: Int, next: Int}

## Decode bytes to characters. Without `utf8` every byte is a character.
export pure decode(data: Bytes, utf8: Bool) -> Decoded {
  let n = data.len()
  let raw = [data.byte_at(index) ?? 0 for index in range(n)]
  if ! utf8 { return {chars: raw, offs: [], native: false} }
  var ascii = true
  for byte in raw { if byte >= 128 { ascii = false; break } }
  if ascii { return {chars: raw, offs: [], native: false} }
  var chars: List[Int] = []
  var offs: List[Int] = []
  var at = 0
  while at < n {
    let lead = raw[at]
    offs += [at]
    if lead < 128 { chars += [lead]; at += 1; continue }
    var need = 0
    var value = 0
    var low = 128
    var high = 191
    if lead >= 194 and lead <= 223 { need = 1; value = lead - 192 } else if lead >= 224 and lead <= 239 {
      need = 2
      value = lead - 224
      if lead == 224 { low = 160 }
      if lead == 237 { high = 159 }
    } else if lead >= 240 and lead <= 244 {
      need = 3
      value = lead - 240
      if lead == 240 { low = 144 }
      if lead == 244 { high = 143 }
    }
    var ok = need > 0 and at + need < n
    if ok {
      for step in range(need) {
        let cont = raw[at + 1 + step]
        let floor = if step == 0 { low } else { 128 }
        let ceil = if step == 0 { high } else { 191 }
        if cont < floor or cont > ceil { ok = false; break }
        value = value * 64 + (cont - 128)
      }
    }
    if ok { chars += [value]; at += 1 + need } else { chars += [RAW + lead]; at += 1 }
  }
  offs += [n]
  {chars: chars, offs: offs, native: false}
}

pure bytes_text(data: Bytes) -> Str? {
  match data.utf8() {
    Ok(text) => text
    Err(_) => null
  }
}

pure utf8_bytes(cp: Int) -> List[Int] {
  if cp < 128 { return [cp] }
  if cp < 2048 { return [192 + cp / 64, 128 + cp % 64] }
  if cp < 65536 { return [224 + cp / 4096, 128 + cp / 64 % 64, 128 + cp % 64] }
  [240 + cp / 262144, 128 + cp / 4096 % 64, 128 + cp / 64 % 64, 128 + cp % 64]
}

pure cp_text(cp: Int) -> Str {
  match bytes.from_ints(utf8_bytes(cp)) {
    Ok(data) => data.utf8() ?? ""
    Err(_) => ""
  }
}

pure text_cp(text: Str) -> Int {
  let decoded = decode(bytes.from_text(text), true)
  if decoded.chars.len() == 1 { decoded.chars[0] } else { -1 }
}

pure lower_cp(cp: Int, utf8: Bool) -> Int {
  if cp < 128 { return if cp >= 65 and cp <= 90 { cp + 32 } else { cp } }
  if ! utf8 or cp >= RAW { return cp }
  let folded = text_cp(cp_text(cp).lower())
  if folded < 0 { cp } else { folded }
}

pure upper_cp(cp: Int, utf8: Bool) -> Int {
  if cp < 128 { return if cp >= 97 and cp <= 122 { cp - 32 } else { cp } }
  if ! utf8 or cp >= RAW { return cp }
  let folded = text_cp(cp_text(cp).upper())
  if folded < 0 { cp } else { folded }
}

# Approximate Unicode letters: Latin, Greek, Cyrillic, other alphabetic
# scripts and CJK ideographs. ASCII is exact.
pure letter(cp: Int) -> Bool {
  if cp < 128 { return (cp >= 65 and cp <= 90) or (cp >= 97 and cp <= 122) }
  if cp == 170 or cp == 181 or cp == 186 { return true }
  if cp >= 192 and cp <= 767 { return cp != 215 and cp != 247 }
  if cp >= 880 and cp <= 1327 { return cp != 894 and cp != 903 and (cp < 1154 or cp > 1161) }
  if cp >= 1329 and cp <= 1423 { return cp <= 1366 or (cp >= 1377 and cp <= 1415) }
  if cp >= 1488 and cp <= 1514 { return true }
  if cp >= 1568 and cp <= 1610 { return true }
  if cp >= 2309 and cp <= 2361 { return true }
  if cp >= 3585 and cp <= 3630 { return true }
  if cp >= 7680 and cp <= 8191 { return true }
  if cp >= 12354 and cp <= 12438 { return true }
  if cp >= 12449 and cp <= 12538 { return true }
  if cp >= 13312 and cp <= 19903 { return true }
  if cp >= 19968 and cp <= 40959 { return true }
  if cp >= 44032 and cp <= 55203 { return true }
  if cp >= 63744 and cp <= 64255 { return true }
  if cp >= 65313 and cp <= 65338 { return true }
  if cp >= 65345 and cp <= 65370 { return true }
  if cp >= 131072 and cp <= 195103 { return true }
  false
}

pure space_cp(cp: Int) -> Bool {
  if cp < 128 { return cp == 32 or (cp >= 9 and cp <= 13) }
  cp == 5760 or (cp >= 8192 and cp <= 8198) or (cp >= 8200 and cp <= 8202) or cp == 8232 or cp == 8233 or cp == 8287 or cp == 12288
}

# Whether `cp` belongs to the POSIX character class `name`.
pure class_has(name: Str, cp: Int, utf8: Bool) -> Bool {
  if cp >= RAW { return false }
  if cp >= 128 and ! utf8 { return false }
  let wide = cp >= 128
  match name {
    "alpha" => letter(cp)
    "digit" => cp >= 48 and cp <= 57
    "alnum" => letter(cp) or (cp >= 48 and cp <= 57)
    "upper" => if wide { letter(cp) and cp_text(cp).lower() != cp_text(cp) } else { cp >= 65 and cp <= 90 }
    "lower" => if wide { letter(cp) and cp_text(cp).upper() != cp_text(cp) } else { cp >= 97 and cp <= 122 }
    "space" => space_cp(cp)
    "blank" => cp == 32 or cp == 9 or (wide and space_cp(cp) and cp != 8232 and cp != 8233)
    "punct" => if wide { cp >= 161 and ! letter(cp) and ! space_cp(cp) } else { cp > 32 and cp < 127 and ! letter(cp) and ! (cp >= 48 and cp <= 57) }
    "print" => if wide { cp >= 160 and cp != 8232 and cp != 8233 } else { cp >= 32 and cp < 127 }
    "graph" => if wide { cp > 160 and ! space_cp(cp) and cp != 8232 and cp != 8233 } else { cp > 32 and cp < 127 }
    "cntrl" => cp < 32 or cp == 127 or (wide and (cp <= 159 or cp == 8232 or cp == 8233))
    "xdigit" => (cp >= 48 and cp <= 57) or (cp >= 65 and cp <= 70) or (cp >= 97 and cp <= 102)
    _ => false
  }
}

pure valid_class(name: Str) -> Bool {
  name in ["alpha", "digit", "alnum", "upper", "lower", "space", "blank", "punct", "print", "graph", "cntrl", "xdigit"]
}

pure raw_member(cs: CharSet, cp: Int, utf8: Bool) -> Bool {
  if cp in cs.singles { return true }
  var at = 0
  while at + 1 < cs.ranges.len() {
    if cp >= cs.ranges[at] and cp <= cs.ranges[at + 1] { return true }
    at += 2
  }
  for name in cs.classes { if class_has(name, cp, utf8) { return true } }
  false
}

pure member(cs: CharSet, cp: Int, icase: Bool, utf8: Bool) -> Bool {
  if raw_member(cs, cp, utf8) { return true }
  if ! icase { return false }
  let low = lower_cp(cp, utf8)
  let up = upper_cp(cp, utf8)
  (low != cp and raw_member(cs, low, utf8)) or (up != cp and raw_member(cs, up, utf8))
}

pure make_set(negated: Bool, singles: List[Int], ranges: List[Int], names: List[Str], icase: Bool, utf8: Bool) -> CharSet {
  # Case-insensitive matching widens `upper` and `lower` to every letter.
  let classes = [if icase and (name == "upper" or name == "lower") { "alpha" } else { name } for name in names]
  let shell: CharSet = {negated: negated, low: [], singles: singles, ranges: ranges, classes: classes}
  let low = [member(shell, cp, icase, utf8) != negated for cp in range(256)]
  {negated: negated, low: low, singles: singles, ranges: ranges, classes: classes}
}

pure set_matches(cs: CharSet, cp: Int, icase: Bool, utf8: Bool) -> Bool {
  if cp >= RAW { return false }
  if cp < 256 { return cs.low[cp] }
  member(cs, cp, icase, utf8) != cs.negated
}

## Word constituent: alphanumeric or underscore.
export pure word_char(cp: Int, utf8: Bool) -> Bool {
  cp == 95 or class_has("alnum", cp, utf8)
}

# One bracket element: kind 0 character, 1 class name, 2 equivalence class,
# 3 collating symbol. `pos` is the index of the element's first character.
pure bracket_element(pc: List[Int], pos: Int, first: Bool) -> Element {
  let n = pc.len()
  let c = pc[pos]
  if c == 91 and pos + 1 < n and (pc[pos + 1] == 58 or pc[pos + 1] == 46 or pc[pos + 1] == 61) {
    let delim = pc[pos + 1]
    var at = pos + 2
    if at >= n { return {kind: 0, cp: 0, name: "", next: at, error: MSG_EBRACK} }
    var name: List[Int] = []
    while true {
      if name.len() >= 32 { return {kind: 0, cp: 0, name: "", next: at, error: MSG_EBRACK} }
      let ch = pc[at]
      at += 1
      if at >= n { return {kind: 0, cp: 0, name: "", next: at, error: MSG_EBRACK} }
      if ch == delim and pc[at] == 93 { break }
      name += [ch]
    }
    at += 1
    let kind = if delim == 58 { 1 } else if delim == 61 { 2 } else { 3 }
    if kind == 1 {
      var text = ""
      for ch in name { text += cp_text(ch) }
      if ! valid_class(text) { return {kind: 1, cp: 0, name: text, next: at, error: "Invalid character class name"} }
      return {kind: 1, cp: 0, name: text, next: at, error: ""}
    }
    if name.len() != 1 { return {kind: kind, cp: 0, name: "", next: at, error: "Invalid collation character"} }
    return {kind: kind, cp: name[0], name: "", next: at, error: ""}
  }
  if c == 45 and ! first {
    if pos + 1 < n and pc[pos + 1] == 93 { return {kind: 0, cp: 45, name: "", next: pos + 1, error: ""} }
    return {kind: 0, cp: 45, name: "", next: pos + 1, error: MSG_ERANGE}
  }
  {kind: 0, cp: c, name: "", next: pos + 1, error: ""}
}

# Parse a bracket expression whose body starts at `start` (just past `[`).
# GNU grep reports malformed expressions with the glibc wording; the
# `[:space:]` confusion is a later, fatal dfa diagnostic.
pure parse_bracket(pc: List[Int], start: Int, icase: Bool, utf8: Bool) -> Bracket {
  let n = pc.len()
  var at = start
  var negated = false
  if at >= n { return {set: null, next: at, error: "Invalid regular expression", late: ""} }
  if pc[at] == 94 {
    negated = true
    at += 1
    if at >= n { return {set: null, next: at, error: "Invalid regular expression", late: ""} }
  }
  var singles: List[Int] = []
  var ranges: List[Int] = []
  var classes: List[Str] = []
  var plain: List[Int] = []
  var fancy = false
  var first = true
  while true {
    if at >= n { return {set: null, next: at, error: MSG_EBRACK, late: ""} }
    let elem = bracket_element(pc, at, first)
    if elem.error != "" { return {set: null, next: elem.next, error: elem.error, late: ""} }
    first = false
    at = elem.next
    if elem.kind == 1 {
      classes += [elem.name]
      fancy = true
    } else if elem.kind == 2 {
      singles += [elem.cp]
      fancy = true
    } else {
      if at >= n { return {set: null, next: at, error: MSG_EBRACK, late: ""} }
      if pc[at] == 45 {
        if at + 1 >= n { return {set: null, next: at, error: MSG_EBRACK, late: ""} }
        if pc[at + 1] != 93 {
          let upper = bracket_element(pc, at + 1, true)
          if upper.error != "" { return {set: null, next: upper.next, error: upper.error, late: ""} }
          if upper.kind == 1 or upper.kind == 2 { return {set: null, next: upper.next, error: MSG_ERANGE, late: ""} }
          if elem.cp > upper.cp { return {set: null, next: upper.next, error: MSG_ERANGE, late: ""} }
          ranges += [elem.cp, upper.cp]
          fancy = true
          at = upper.next
        } else {
          singles += [elem.cp]
          plain += [elem.cp]
        }
      } else {
        singles += [elem.cp]
        plain += [elem.cp]
        if elem.kind == 3 { fancy = true }
      }
    }
    if at >= n { return {set: null, next: at, error: MSG_EBRACK, late: ""} }
    if pc[at] == 93 { at += 1; break }
  }
  var late = ""
  if ! fancy and plain.len() >= 2 and plain[0] == 58 and plain[plain.len() - 1] == 58 {
    var other = false
    for cp in plain { if cp != 58 { other = true } }
    if other { late = "character class syntax is [[:space:]], not [:space:]" }
  }
  {set: make_set(negated, singles, ranges, classes, icase, utf8), next: at, error: "", late: late}
}

# Scan an interval number up to a comma or the closing brace. Stop codes:
# 0 end of pattern, 1 comma, 2 closing brace. Invalid digits give -2 and an
# empty number gives -1, as in glibc.
pure interval_number(pc: List[Int], start: Int, ere: Bool) -> Number {
  let n = pc.len()
  var at = start
  var value = -1
  var bad = false
  while true {
    if at >= n { return {value: -2, stop: 0, next: at} }
    let c = pc[at]
    if ere and c == 125 { return {value: if bad { -2 } else { value }, stop: 2, next: at + 1} }
    if ! ere and c == 92 and at + 1 < n and pc[at + 1] == 125 { return {value: if bad { -2 } else { value }, stop: 2, next: at + 2} }
    if c == 44 { return {value: if bad { -2 } else { value }, stop: 1, next: at + 1} }
    if c >= 48 and c <= 57 and ! bad {
      let digit = c - 48
      value = if value < 0 { digit } else if value * 10 + digit > DUP_MAX + 1 { DUP_MAX + 1 } else { value * 10 + digit }
    } else { bad = true }
    if ! ere and c == 92 and at + 1 < n { at += 2 } else { at += 1 }
  }
  {value: -2, stop: 0, next: at}
}

# Interval kinds: 0 valid, 1 literal brace (extended syntax tolerates a
# malformed interval), 2 error.
pure parse_interval(pc: List[Int], start: Int, ere: Bool) -> Interval {
  let first = interval_number(pc, start, ere)
  var low = first.value
  var high = -2
  var stop = first.stop
  var next = first.next
  if low != -2 {
    if low == -1 {
      if stop != 1 { return {kind: 2, min: 0, max: 0, next: next, error: MSG_BADBR} }
      low = 0
    }
    if stop == 2 { high = low } else if stop == 1 {
      let second = interval_number(pc, next, ere)
      high = second.value
      stop = second.stop
      next = second.next
    }
  }
  if low == -2 or high == -2 {
    if ere { return {kind: 1, min: 0, max: 0, next: start, error: ""} }
    return {kind: 2, min: 0, max: 0, next: next, error: if stop == 0 { MSG_EBRACE } else { MSG_BADBR }}
  }
  if (high != -1 and low > high) or stop != 2 { return {kind: 2, min: 0, max: 0, next: next, error: MSG_BADBR} }
  if (if high == -1 { low } else { high }) > DUP_MAX { return {kind: 2, min: 0, max: 0, next: next, error: "Regular expression too big"} }
  {kind: 0, min: low, max: high, next: next, error: ""}
}

pure union(left: List[Int], right: List[Int]) -> List[Int] {
  var out = left
  for item in right { if item not in out { out += [item] } }
  out
}

# Characters after a backslash that GNU grep accepts as plain escapes
# without a stray-backslash warning.
pure plain_escape(cp: Int, ere: Bool) -> Bool {
  if cp in [36, 42, 46, 91, 92, 93, 94, 125] { return true }
  ere and cp in [40, 41, 123, 124, 43, 63]
}

# Parse a pattern into a node arena. Errors from the glibc-compatible stage
# carry no warnings; the dfa-stage warnings survive only on success.
pure parse(pattern: Bytes, ere: Bool, icase: Bool, utf8: Bool) -> Parsed {
  let pc = decode(pattern, utf8).chars
  let n = pc.len()
  var nodes: List[Node] = []
  var sets: List[CharSet] = []
  var warnings: List[Str] = []
  var late = ""
  var late_at = 0
  var groups = 0
  var backrefs = false
  var exotic = false
  var frames: List[Frame] = []
  var seq: List[Int] = []
  var alts: List[Int] = []
  var completed: List[Int] = []
  var init: List[Int] = []
  var acc: List[Int] = []
  var cur_group = 0
  var laststart = true
  var error = ""
  var at = 0
  while at < n {
    let c = pc[at]
    var tok = T_LITERAL
    var lit = c
    var width = 1
    if c == 92 {
      if at + 1 >= n { error = "Trailing backslash"; break }
      let d = pc[at + 1]
      lit = d
      width = 2
      if ! ere and d == 40 { tok = T_OPEN } else if ! ere and d == 41 { tok = T_CLOSE } else if ! ere and d == 124 { tok = T_ALT } else if ! ere and d == 123 { tok = T_INTERVAL } else if ! ere and d == 43 { tok = T_PLUS } else if ! ere and d == 63 { tok = T_QMARK } else if d >= 49 and d <= 57 { tok = T_BACKREF } else if d in [60, 62, 98, 66, 96, 39] { tok = T_ASSERT } else if d in [119, 87, 115, 83] { tok = T_CLASS } else {
        tok = T_LITERAL
        if ! plain_escape(d, ere) { warnings += [f"stray \\ before {cp_text(d)}"] }
      }
    } else if c == 46 { tok = T_ANY } else if c == 91 { tok = T_BRACKET } else if c == 42 { tok = T_STAR } else if ere and c == 43 { tok = T_PLUS } else if ere and c == 63 { tok = T_QMARK } else if ere and c == 123 { tok = T_INTERVAL } else if ere and c == 40 { tok = T_OPEN } else if ere and c == 41 { tok = T_CLOSE } else if ere and c == 124 { tok = T_ALT } else if c == 94 {
      if ere or seq.is_empty() { tok = T_BOL }
    } else if c == 36 {
      if ere or at + 1 == n or (pc[at + 1] == 92 and at + 2 < n and (pc[at + 2] == 41 or pc[at + 2] == 124)) { tok = T_EOL }
    }

    # A repetition operator with nothing to repeat is literal in basic
    # syntax and a warned no-op repetition of the empty string otherwise.
    var repeat = false
    var low = 0
    var high = -1
    if tok == T_STAR or tok == T_PLUS or tok == T_QMARK or tok == T_INTERVAL {
      var op_text = "*"
      if ! ere and (tok == T_PLUS or tok == T_QMARK) { exotic = true }
      if tok == T_INTERVAL and at + width < n and pc[at + width] == 44 { exotic = true }
      if tok == T_PLUS { op_text = "+" } else if tok == T_QMARK { op_text = "?" } else if tok == T_INTERVAL { op_text = "{...}" }
      var interval_next = at + width
      var valid = true
      if laststart and ! ere {
        # Basic syntax: `*` is an ordinary character here; the backslash
        # operators become literals that only warn.
        valid = false
        tok = T_LITERAL
        if c == 92 { warnings += [f"stray \\ before {cp_text(lit)}"] }
      } else if tok == T_INTERVAL {
        let span = parse_interval(pc, at + width, ere)
        if span.kind == 2 and ! (ere and laststart) { error = span.error; break }
        if span.kind != 0 {
          valid = false
          tok = T_LITERAL
          lit = c
          width = 1
        } else {
          low = span.min
          high = span.max
          interval_next = span.next
        }
      } else if tok == T_PLUS { low = 1 } else if tok == T_QMARK { high = 1 }
      if valid {
        if laststart and ere {
          warnings += [f"{op_text} at start of expression"]
          nodes += [{kind: N_EMPTY, a: 0, b: 0, kids: []}]
          seq += [nodes.len() - 1]
        }
        repeat = true
        width = interval_next - at
      }
    }

    if repeat {
      let atom = seq[seq.len() - 1]
      nodes += [{kind: N_REPEAT, a: low, b: high, kids: [atom]}]
      seq[seq.len() - 1] = nodes.len() - 1
    } else if tok == T_LITERAL {
      nodes += [{kind: N_CHAR, a: lit, b: 0, kids: []}]
      seq += [nodes.len() - 1]
      laststart = false
    } else if tok == T_ANY {
      nodes += [{kind: N_ANY, a: 0, b: 0, kids: []}]
      seq += [nodes.len() - 1]
      laststart = false
    } else if tok == T_BRACKET {
      let parsed = parse_bracket(pc, at + 1, icase, utf8)
      if parsed.error != "" { error = parsed.error; break }
      if parsed.late != "" and late == "" { late = parsed.late; late_at = warnings.len() }
      sets += [parsed.set ?? make_set(false, [], [], [], false, false)]
      nodes += [{kind: N_SET, a: sets.len() - 1, b: 0, kids: []}]
      seq += [nodes.len() - 1]
      laststart = false
      width = parsed.next - at
    } else if tok == T_BOL or tok == T_EOL {
      nodes += [{kind: N_ASSERT, a: if tok == T_BOL { OP_BOL } else { OP_EOL }, b: 0, kids: []}]
      seq += [nodes.len() - 1]
    } else if tok == T_ASSERT {
      exotic = true
      let kind = if lit == 60 { OP_WORD_START } else if lit == 62 { OP_WORD_END } else if lit == 98 { OP_WORD_BOUND } else if lit == 66 { OP_NOT_WORD_BOUND } else if lit == 96 { OP_BOL } else { OP_EOL }
      nodes += [{kind: N_ASSERT, a: kind, b: 0, kids: []}]
      seq += [nodes.len() - 1]
    } else if tok == T_CLASS {
      exotic = true
      let negated = lit == 87 or lit == 83
      let cs = if lit == 119 or lit == 87 { make_set(negated, [95], [], ["alnum"], false, utf8) } else { make_set(negated, [], [], ["space"], false, utf8) }
      sets += [cs]
      nodes += [{kind: N_SET, a: sets.len() - 1, b: 0, kids: []}]
      seq += [nodes.len() - 1]
      laststart = false
    } else if tok == T_BACKREF {
      let number = lit - 48
      if number not in completed { error = "Invalid back reference"; break }
      backrefs = true
      exotic = true
      nodes += [{kind: N_BACKREF, a: number, b: 0, kids: []}]
      seq += [nodes.len() - 1]
      laststart = false
    } else if tok == T_OPEN {
      groups += 1
      frames += [{seq: seq, alts: alts, group: cur_group, init: init, acc: acc}]
      cur_group = groups
      seq = []
      alts = []
      init = completed
      acc = []
      laststart = true
    } else if tok == T_ALT {
      if ! ere { exotic = true }
      nodes += [{kind: if seq.is_empty() { N_EMPTY } else { N_CAT }, a: 0, b: 0, kids: seq}]
      alts += [nodes.len() - 1]
      seq = []
      acc = union(acc, completed)
      completed = init
      laststart = true
    } else if tok == T_CLOSE {
      if frames.is_empty() {
        if ere {
          nodes += [{kind: N_CHAR, a: 41, b: 0, kids: []}]
          seq += [nodes.len() - 1]
          laststart = false
        } else { error = "Unmatched ) or \\)"; break }
      } else {
        nodes += [{kind: if seq.is_empty() { N_EMPTY } else { N_CAT }, a: 0, b: 0, kids: seq}]
        alts += [nodes.len() - 1]
        nodes += [{kind: N_ALT, a: 0, b: 0, kids: alts}]
        let body = nodes.len() - 1
        acc = union(acc, completed)
        completed = acc
        let gid = cur_group
        nodes += [{kind: N_GROUP, a: gid, b: 0, kids: [body]}]
        let frame = frames[frames.len() - 1]
        frames = frames[0..frames.len() - 1]
        seq = frame.seq + [nodes.len() - 1]
        alts = frame.alts
        cur_group = frame.group
        init = frame.init
        acc = frame.acc
        completed = union(completed, [gid])
        laststart = false
      }
    }
    at += width
  }
  if error == "" and ! frames.is_empty() { error = "Unmatched ( or \\(" }
  if error != "" { return {nodes: nodes, root: 0, sets: sets, groups: groups, backrefs: backrefs, exotic: exotic, warnings: [], error: error} }
  nodes += [{kind: if seq.is_empty() { N_EMPTY } else { N_CAT }, a: 0, b: 0, kids: seq}]
  alts += [nodes.len() - 1]
  nodes += [{kind: N_ALT, a: 0, b: 0, kids: alts}]
  if late != "" {
    return {nodes: nodes, root: nodes.len() - 1, sets: sets, groups: groups, backrefs: backrefs, exotic: exotic, warnings: warnings[0..late_at], error: late}
  }
  {nodes: nodes, root: nodes.len() - 1, sets: sets, groups: groups, backrefs: backrefs, exotic: exotic, warnings: warnings, error: ""}
}

pure nullable(nodes: List[Node], idx: Int) -> Bool {
  let node = nodes[idx]
  if node.kind == N_CHAR or node.kind == N_ANY or node.kind == N_SET { return false }
  if node.kind == N_EMPTY or node.kind == N_ASSERT or node.kind == N_BACKREF { return true }
  if node.kind == N_REPEAT { return node.a == 0 or nullable(nodes, node.kids[0]) }
  if node.kind == N_CAT {
    for kid in node.kids { if ! nullable(nodes, kid) { return false } }
    return true
  }
  if node.kind == N_ALT {
    for kid in node.kids { if nullable(nodes, kid) { return true } }
    return false
  }
  nullable(nodes, node.kids[0])
}

# Code generation with relative jumps, so a sub-program can be copied for
# counted repetition. `marks` adds empty-iteration guards, which only
# back-reference programs need because the others memoize states.
pure generate(nodes: List[Node], idx: Int, icase: Bool, utf8: Bool, marks: Bool, slot_base: Int) -> List[Inst] {
  let node = nodes[idx]
  if node.kind == N_EMPTY { return [] }
  if node.kind == N_CHAR { return [{op: OP_CHAR, x: if icase { lower_cp(node.a, utf8) } else { node.a }, y: 0}] }
  if node.kind == N_ANY { return [{op: OP_ANY, x: 0, y: 0}] }
  if node.kind == N_SET { return [{op: OP_SET, x: node.a, y: 0}] }
  if node.kind == N_ASSERT { return [{op: node.a, x: 0, y: 0}] }
  if node.kind == N_BACKREF { return [{op: OP_BACKREF, x: node.a, y: 0}] }
  if node.kind == N_CAT {
    var out: List[Inst] = []
    for kid in node.kids {
      # Single-character atoms are emitted in place; a recursive call per
      # character would copy the whole node arena each time.
      let atom = nodes[kid]
      if atom.kind == N_CHAR { out += [{op: OP_CHAR, x: if icase { lower_cp(atom.a, utf8) } else { atom.a }, y: 0}] } else if atom.kind == N_ANY { out += [{op: OP_ANY, x: 0, y: 0}] } else if atom.kind == N_SET { out += [{op: OP_SET, x: atom.a, y: 0}] } else if atom.kind == N_ASSERT { out += [{op: atom.a, x: 0, y: 0}] } else {
        out += generate(nodes, kid, icase, utf8, marks, slot_base)
      }
    }
    return out
  }
  if node.kind == N_GROUP {
    let body = generate(nodes, node.kids[0], icase, utf8, marks, slot_base)
    return [{op: OP_SAVE, x: 2 * node.a, y: 0}] + body + [{op: OP_SAVE, x: 2 * node.a + 1, y: 0}]
  }
  if node.kind == N_ALT {
    if node.kids.len() == 1 { return generate(nodes, node.kids[0], icase, utf8, marks, slot_base) }
    var bodies: List[List[Inst]] = []
    var total = 0
    for index in range(node.kids.len()) {
      let body = generate(nodes, node.kids[index], icase, utf8, marks, slot_base)
      bodies += [body]
      total += body.len() + (if index < node.kids.len() - 1 { 2 } else { 0 })
    }
    var out: List[Inst] = []
    for index in range(bodies.len()) {
      let body = bodies[index]
      if index < bodies.len() - 1 {
        out += [{op: OP_SPLIT, x: 1, y: body.len() + 2}] + body
        out += [{op: OP_JMP, x: total - out.len(), y: 0}]
      } else { out += body }
    }
    return out
  }
  # Repetition
  let kid = node.kids[0]
  let body = generate(nodes, kid, icase, utf8, marks, slot_base)
  let width = body.len()
  let copies = if node.b == -1 { node.a + 1 } else { node.b }
  if width * copies > PROGRAM_LIMIT { return [{op: OP_TOO_BIG, x: 0, y: 0}] }
  var out: List[Inst] = []
  for _ in range(node.a) { out += body }
  if node.b == -1 {
    if marks and nullable(nodes, kid) {
      out += [{op: OP_SPLIT, x: 1, y: width + 4}, {op: OP_MARK, x: slot_base + idx, y: 0}] + body + [{op: OP_CHECK, x: slot_base + idx, y: 0}, {op: OP_JMP, x: -(width + 3), y: 0}]
    } else {
      out += [{op: OP_SPLIT, x: 1, y: width + 2}] + body + [{op: OP_JMP, x: -(width + 1), y: 0}]
    }
  } else {
    let optional = node.b - node.a
    for step in range(optional) {
      out += [{op: OP_SPLIT, x: 1, y: (optional - step) * (width + 1)}] + body
    }
  }
  out
}

# Possible first characters, from walking the program until a consuming
# instruction. The 257th entry covers every character above 255. An empty
# result disables the filter.
pure first_set(ops: List[Int], xs: List[Int], ys: List[Int], sets: List[CharSet], icase: Bool, utf8: Bool) -> List[Bool] {
  var table = [false for _ in range(257)]
  var seen = [false for _ in range(ops.len() + 1)]
  var stack = [0]
  while ! stack.is_empty() {
    let pc = stack[stack.len() - 1]
    stack = stack[0..stack.len() - 1]
    if pc >= ops.len() or seen[pc] { continue }
    seen[pc] = true
    let op = ops[pc]
    if op == OP_MATCH or op == OP_BACKREF or op == OP_ANY { return [] }
    if op == OP_CHAR {
      let x = xs[pc]
      if x >= 256 {
        if icase { return [] }
        table[256] = true
      } else {
        table[x] = true
        if icase {
          if x >= 128 and utf8 { return [] }
          table[upper_cp(x, false)] = true
          table[lower_cp(x, false)] = true
        }
      }
    } else if op == OP_SET {
      let cs = sets[xs[pc]]
      for cp in range(256) { if cs.low[cp] { table[cp] = true } }
      table[256] = true
    } else if op == OP_SPLIT {
      stack += [pc + xs[pc], pc + ys[pc]]
    } else if op == OP_JMP {
      stack += [pc + xs[pc]]
    } else {
      stack += [pc + 1]
    }
  }
  table
}

## Compile one pattern. A malformed pattern is an error whose message is
## GNU grep's wording; the warnings GNU prints for the pattern are returned.
export pure compile_one(pattern: Bytes, ere: Bool, icase: Bool, utf8: Bool) -> Result[Compiled, Error] {
  let parsed = parse(pattern, ere, icase, utf8)
  if parsed.error != "" { search.reject(parsed.error)? }
  build(parsed, icase, utf8, parsed.warnings, false, pattern, ere)
}

# Whether a pattern can go to the C library matcher unchanged: ASCII only,
# no GNU operators, assertions only at the edges of a top-level alternative,
# and repetition only of non-empty atoms, where POSIX semantics are
# unambiguous.
pure native_pattern(parsed: Parsed, icase: Bool) -> Bool {
  let root = parsed.nodes[parsed.root]
  for alt in root.kids {
    let node = parsed.nodes[alt]
    var items: List[Int] = []
    if node.kind == N_CAT { items = node.kids } else { items = [alt] }
    if items.is_empty() { return false }
    var begin = 0
    var end = items.len()
    if parsed.nodes[items[0]].kind == N_ASSERT and parsed.nodes[items[0]].a == OP_BOL { begin = 1 }
    if end > begin and parsed.nodes[items[end - 1]].kind == N_ASSERT and parsed.nodes[items[end - 1]].a == OP_EOL { end -= 1 }
    for position in range(begin, end) {
      if ! native_atom(parsed, items[position], icase) { return false }
    }
  }
  true
}

pure native_atom(parsed: Parsed, idx: Int, icase: Bool) -> Bool {
  let node = parsed.nodes[idx]
  if node.kind == N_CHAR { return node.a < 128 }
  if node.kind == N_ANY { return true }
  if node.kind == N_SET {
    let cs = parsed.sets[node.a]
    for cp in cs.singles { if cp >= 128 { return false } }
    for cp in cs.ranges { if cp >= 128 { return false } }
    for name in cs.classes { if icase and name == "alpha" { return false } }
    return true
  }
  if node.kind == N_GROUP { return native_atom(parsed, node.kids[0], icase) }
  if node.kind == N_CAT or node.kind == N_ALT {
    if node.kids.is_empty() { return false }
    for kid in node.kids { if ! native_atom(parsed, kid, icase) { return false } }
    return true
  }
  if node.kind == N_REPEAT {
    let kid = parsed.nodes[node.kids[0]]
    if kid.kind == N_REPEAT or kid.kind == N_EMPTY or kid.kind == N_ASSERT { return false }
    if nullable(parsed.nodes, node.kids[0]) { return false }
    if node.a > 255 or node.b > 255 { return false }
    return native_atom(parsed, node.kids[0], icase)
  }
  false
}

pure build(parsed: Parsed, icase: Bool, utf8: Bool, warnings: List[Str], fixed: Bool, source: Bytes, ere: Bool) -> Result[Compiled, Error] {
  let slot_base = 2 * (parsed.groups + 1)
  let code = generate(parsed.nodes, parsed.root, icase, utf8, parsed.backrefs, slot_base) + [{op: OP_MATCH, x: 0, y: 0}]
  for inst in code { if inst.op == OP_TOO_BIG { search.reject("Regular expression too big")? } }
  let ops = [inst.op for inst in code]
  let xs = [inst.x for inst in code]
  let ys = [inst.y for inst in code]
  var splits = 0
  var looping = false
  for pc in range(ops.len()) {
    if ops[pc] == OP_SPLIT { splits += 1 }
    if ops[pc] == OP_JMP and xs[pc] < 0 { looping = true }
  }
  var literal: Bytes? = null
  var native: Str? = null
  var text: List[Int] = []
  var plain = true
  for pc in range(ops.len() - 1) {
    if ops[pc] == OP_CHAR and xs[pc] < 128 { text += [xs[pc]] } else { plain = false; break }
  }
  if plain and ! text.is_empty() {
    match bytes.from_ints(text) {
      Ok(data) => {
        literal = data
        # The same bytes as a basic regular expression for the native
        # matcher, with its few special characters escaped.
        var quoted: List[Int] = []
        for byte in text {
          if byte in [92, 91, 93, 46, 42, 94, 36] { quoted += [92] }
          quoted += [byte]
        }
        match bytes.from_ints(quoted) {
          Ok(escaped) => native = bytes_text(escaped)
          Err(_) => native = null
        }
      }
      Err(_) => literal = null
    }
  }
  var consuming = false
  for pc in range(ops.len()) {
    if ops[pc] in [OP_CHAR, OP_ANY, OP_SET, OP_BACKREF] { consuming = true }
  }
  var native_ere = false
  if literal == null and ! fixed and ! parsed.exotic and parsed.warnings.is_empty() and ! parsed.backrefs {
    let shaped = native_pattern(parsed, icase)
    if shaped { native = bytes_text(source); native_ere = ere }
  }
  var lead = 0
  while lead < ops.len() and ops[lead] == OP_SAVE { lead += 1 }
  let anchored = lead < ops.len() and ops[lead] == OP_BOL
  let first = if parsed.backrefs { [] } else { first_set(ops, xs, ys, parsed.sets, icase, utf8) }
  let program: Program = {ops: ops, xs: xs, ys: ys, sets: parsed.sets, slots: slot_base + parsed.nodes.len(), backrefs: parsed.backrefs, icase: icase, utf8: utf8, memo: looping or splits > 3, filter: ! first.is_empty(), anchored: anchored, fixed: fixed, consuming: consuming, first: first, literal: literal, native: native, native_ere: native_ere}
  Ok({programs: [program], warnings: warnings})
}

## Compile a fixed string: every character is literal.
export pure compile_fixed(pattern: Bytes, icase: Bool, utf8: Bool) -> Result[Compiled, Error] {
  let chars = decode(pattern, utf8).chars
  var nodes: List[Node] = []
  var kids: List[Int] = []
  for cp in chars {
    nodes += [{kind: N_CHAR, a: cp, b: 0, kids: []}]
    kids += [nodes.len() - 1]
  }
  nodes += [{kind: N_CAT, a: 0, b: 0, kids: kids}]
  nodes += [{kind: N_ALT, a: 0, b: 0, kids: [nodes.len() - 1]}]
  build({nodes: nodes, root: nodes.len() - 1, sets: [], groups: 0, backrefs: false, exotic: false, warnings: [], error: ""}, icase, utf8, [], true, pattern, false)
}

pure char_ok(cp: Int, want: Int, icase: Bool, utf8: Bool) -> Bool {
  cp == want or (icase and cp < RAW and lower_cp(cp, utf8) == want)
}

# Explore the program from `from`. With `single` the result lists every end
# reachable from exactly that start; otherwise it is [start, longest end] of
# the leftmost start that matches, or empty.
pure explore(program: Program, chars: List[Int], from: Int, single: Bool) -> List[Int] {
  let n = chars.len()
  let size = program.ops.len()
  let width = n + 1
  let memo = program.memo and ! program.backrefs
  let tracked = program.backrefs
  var seen: List[Bool] = if memo { [false for _ in range(size * width)] } else { [] }
  let blank = if tracked { [-1 for _ in range(program.slots)] } else { [] }
  let icase = program.icase
  let utf8 = program.utf8
  var start = from
  while start <= n {
    if ! single and program.anchored and start > 0 { break }
    if ! single and program.filter {
      while start < n {
        let c = chars[start]
        if program.first[if c < 256 { c } else { 256 }] { break }
        start += 1
      }
      if start >= n { break }
    }
    var stack_pc: List[Int] = [0]
    var stack_at: List[Int] = [start]
    var stack_caps: List[List[Int]] = if tracked { [blank] } else { [] }
    var top = 1
    var ends: List[Int] = []
    while top > 0 {
      top -= 1
      var pc = stack_pc[top]
      var at = stack_at[top]
      var caps = if tracked { stack_caps[top] } else { [] }
      while true {
        if memo {
          let key = pc * width + at
          if seen[key] { break }
          seen[key] = true
        }
        let op = program.ops[pc]
        if op == OP_CHAR {
          if at < n and char_ok(chars[at], program.xs[pc], icase, utf8) { pc += 1; at += 1 } else { break }
        } else if op == OP_ANY {
          if at < n and chars[at] < RAW { pc += 1; at += 1 } else { break }
        } else if op == OP_SET {
          if at < n and set_matches(program.sets[program.xs[pc]], chars[at], icase, utf8) { pc += 1; at += 1 } else { break }
        } else if op == OP_SPLIT {
          let other = pc + program.ys[pc]
          if top < stack_pc.len() {
            stack_pc[top] = other
            stack_at[top] = at
            if tracked { stack_caps[top] = caps }
          } else {
            stack_pc += [other]
            stack_at += [at]
            if tracked { stack_caps += [caps] }
          }
          top += 1
          pc += program.xs[pc]
        } else if op == OP_JMP {
          pc += program.xs[pc]
        } else if op == OP_MATCH {
          if at not in ends { ends += [at] }
          break
        } else if op == OP_SAVE or op == OP_MARK {
          if tracked { caps[program.xs[pc]] = at }
          pc += 1
        } else if op == OP_CHECK {
          # An iteration that consumed nothing ends the loop but keeps its
          # captures, as an empty group match does in glibc.
          if tracked and caps[program.xs[pc]] == at { pc += 2 } else { pc += 1 }
        } else if op == OP_BOL {
          if at != 0 { break }
          pc += 1
        } else if op == OP_EOL {
          if at != n { break }
          pc += 1
        } else if op == OP_BACKREF {
          let gid = program.xs[pc]
          let lo = caps[2 * gid]
          let hi = caps[2 * gid + 1]
          if lo < 0 or hi < lo { break }
          let length = hi - lo
          if at + length > n { break }
          var same = true
          for step in range(length) {
            if ! char_ok(chars[at + step], chars[lo + step], false, utf8) {
              if ! (icase and lower_cp(chars[at + step], utf8) == lower_cp(chars[lo + step], utf8)) { same = false; break }
            }
          }
          if ! same { break }
          at += length
          pc += 1
        } else {
          let before = at > 0 and word_char(chars[at - 1], utf8)
          let after = at < n and word_char(chars[at], utf8)
          var ok = false
          if op == OP_WORD_START { ok = after and ! before } else if op == OP_WORD_END { ok = before and ! after } else if op == OP_WORD_BOUND { ok = before != after } else { ok = before == after }
          if ! ok { break }
          pc += 1
        }
      }
    }
    if ! ends.is_empty() {
      if single { return ends }
      var best = ends[0]
      for end in ends { if end > best { best = end } }
      return [start, best]
    }
    if single { return [] }
    start += 1
  }
  []
}

## A set of compiled patterns searched together under GNU `-w` / `-x`
## semantics, plus the flags that choose the cheapest search path.
export type Matcher = {programs: List[Program], utf8: Bool, word: Bool, whole: Bool, literal_only: Bool, ascii_only: Bool, literal_words: Bool, empty_only: Bool}

## Combine compiled patterns into a matcher.
export pure matcher(programs: List[Program], utf8: Bool, word: Bool, whole: Bool) -> Matcher {
  var native_all = true
  var literals = true
  var ascii_only = false
  var empty_only = true
  for program in programs {
    if program.consuming { empty_only = false }
    if program.native == null { native_all = false }
    if program.literal == null {
      literals = false
      if utf8 { ascii_only = true }
    }
  }
  {programs: programs, utf8: utf8, word: word, whole: whole, literal_only: native_all and ! word and ! whole, ascii_only: ascii_only, literal_words: literals and word and ! whole, empty_only: empty_only}
}

pure boundary_ok(chars: List[Int], start: Int, end: Int, utf8: Bool, unchecked_prev: Bool) -> Bool {
  if start > 0 and ! unchecked_prev and word_char(chars[start - 1], utf8) { return false }
  if end < chars.len() and word_char(chars[end], utf8) { return false }
  true
}

pure offset_of(offs: List[Int], index: Int) -> Int {
  if offs.is_empty() { index } else { offs[index] }
}

# Leftmost start over a pack of programs and, there, the longest end.
pure group_leftmost(pack: List[Program], chars: List[Int], from: Int) -> List[Int] {
  var best: List[Int] = []
  for program in pack {
    let found = explore(program, chars, from, false)
    if found.is_empty() { continue }
    if best.is_empty() or found[0] < best[0] or (found[0] == best[0] and found[1] > best[1]) { best = found }
  }
  best
}

# Every end reachable from `start` by any program of the pack.
pure group_ends(pack: List[Program], chars: List[Int], start: Int) -> List[Int] {
  var ends: List[Int] = []
  for program in pack {
    for end in explore(program, chars, start, true) { if end not in ends { ends += [end] } }
  }
  ends
}

# The `-w` search over a pack of programs that GNU grep compiles as one
# regular expression. Selecting a line asks whether any match has non-word
# edges, which is how GNU grep's automaton decides it. Printing matches
# (`iterating`) and patterns with back-references go through GNU's retry
# loop instead: take the leftmost-longest match, and while its edges are
# word characters retry with the longest strictly shorter non-empty match
# at the same start, then move one position on. That loop measures the
# shorter match's limit from the position the search resumed at, so on a
# resumed search shorter matches are cut short by that offset. GNU grep's
# fixed-string matcher does not look at the character before the position a
# repeated search resumes from, so adjacent matches may touch.
pure find_word(pack: List[Program], chars: List[Int], offs: List[Int], from: Int, iterating: Bool) -> List[Int] {
  let fixed = pack[0].fixed
  let utf8 = pack[0].utf8
  var references = false
  for program in pack { if program.backrefs { references = true } }
  let retry_loop = (iterating or references) and ! fixed
  let shift = if iterating { offset_of(offs, from) } else { 0 }
  var at = from
  while at <= chars.len() {
    let found = group_leftmost(pack, chars, at)
    if found.is_empty() { return [] }
    let start = found[0]
    let ends = group_ends(pack, chars, start)
    var stop = found[1]
    while true {
      if boundary_ok(chars, start, stop, utf8, fixed and start == from) { return [start, stop] }
      var next = -1
      if retry_loop {
        let begin = offset_of(offs, start)
        let reach = offset_of(offs, stop) - begin
        if reach > 0 {
          let limit = begin + reach - 1 - shift
          for end in ends {
            let edge = offset_of(offs, end)
            if end < stop and edge > begin and edge <= limit and end > next { next = end }
          }
        }
      } else {
        for end in ends { if end < stop and end >= start and end > next { next = end } }
      }
      if next < 0 { break }
      stop = next
    }
    at = start + 1
  }
  []
}

pure find_group(m: Matcher, pack: List[Program], chars: List[Int], offs: List[Int], from: Int, iterating: Bool) -> List[Int] {
  if m.whole {
    if from != 0 { return [] }
    if chars.len() in group_ends(pack, chars, 0) { return [0, chars.len()] }
    return []
  }
  if m.word { return find_word(pack, chars, offs, from, iterating) }
  group_leftmost(pack, chars, from)
}

## Find the leftmost-longest match at or after character index `from`
## across every pattern; the result is [start, end] or empty. Patterns
## without back-references act as one alternation, as in GNU grep, while
## each back-reference pattern is matched on its own.
export pure find(m: Matcher, chars: List[Int], offs: List[Int], from: Int, iterating: Bool) -> List[Int] {
  var candidates: List[List[Int]] = []
  var plain: List[Program] = []
  for program in m.programs {
    if program.backrefs { candidates += [find_group(m, [program], chars, offs, from, iterating)] } else { plain += [program] }
  }
  if ! plain.is_empty() { candidates += [find_group(m, plain, chars, offs, from, iterating)] }
  var best: List[Int] = []
  for found in candidates {
    if found.is_empty() { continue }
    if best.is_empty() or found[0] < best[0] or (found[0] == best[0] and found[1] > best[1]) { best = found }
  }
  best
}

## Map a character index to its byte offset.
export pure byte_offset(dec: Decoded, index: Int) -> Int {
  if dec.offs.is_empty() { index } else { dec.offs[index] }
}

## The character index at or after a byte offset.
export pure char_index(dec: Decoded, byte: Int) -> Int {
  if dec.offs.is_empty() { return byte }
  var low = 0
  var high = dec.offs.len() - 1
  while low < high {
    let mid = (low + high) / 2
    if dec.offs[mid] < byte { low = mid + 1 } else { high = mid }
  }
  low
}

# Printable ASCII only (tab and other controls count as non-ASCII here, which
# merely sends such a line down the slower character path).
pure ascii_line(line: Bytes) -> Bool {
  match regex.find_bytes("[^ -~]", line) {
    Ok(found) => found.is_empty()
    Err(_) => false
  }
}

## Prepare a line for matching. Patterns the C library can answer are
## matched on the raw bytes; anything else is decoded to characters.
export pure subject(m: Matcher, line: Bytes) -> Decoded {
  if (m.literal_only or m.literal_words) and (! m.ascii_only or ascii_line(line)) { return {chars: [], offs: [], native: true} }
  decode(line, m.utf8)
}

pure find_decoded(m: Matcher, dec: Decoded, from_byte: Int, iterating: Bool) -> List[Int] {
  let found = find(m, dec.chars, dec.offs, char_index(dec, from_byte), iterating)
  if found.is_empty() { return [] }
  [byte_offset(dec, found[0]), byte_offset(dec, found[1])]
}

## Like `find` but over a line prepared by `subject`; positions are byte
## offsets. `iterating` is set when listing the matches of an already
## selected line (`-o`, colors) rather than deciding whether it matches.
export pure find_bytes(m: Matcher, line: Bytes, dec: Decoded, from_byte: Int, iterating: Bool) -> List[Int] {
  if iterating and m.empty_only { return [] }
  if ! dec.native { return find_decoded(m, dec, from_byte, iterating) }
  var best: List[Int] = []
  for program in m.programs {
    let text = program.native ?? ""
    var at = from_byte
    while at <= line.len() {
      match regex.captures_bytes(text, line, at, program.native_ere, program.icase) {
        Ok(caps) => {
          if caps.is_empty() { break }
          let span = caps[0] ?? {start: 0, end: 0}
          if m.word {
            # Whole-word check on ASCII neighbors; a multibyte neighbor
            # needs character decoding.
            let before = if span.start > 0 { line.byte_at(span.start - 1) ?? 0 } else { 0 }
            let after = if span.end < line.len() { line.byte_at(span.end) ?? 0 } else { 0 }
            if m.utf8 and (before >= 128 or after >= 128) { return find_decoded(m, decode(line, m.utf8), from_byte, iterating) }
            let prev_word = span.start > 0 and ! (program.fixed and span.start == from_byte) and ascii_word(before)
            if prev_word or ascii_word(after) {
              at = span.start + 1
              continue
            }
          }
          if best.is_empty() or span.start < best[0] or (span.start == best[0] and span.end > best[1]) { best = [span.start, span.end] }
          break
        }
        Err(_) => {
          # The native matcher refuses NUL bytes: take the character path.
          return find_decoded(m, decode(line, m.utf8), from_byte, iterating)
        }
      }
    }
  }
  best
}

pure ascii_word(byte: Int) -> Bool {
  byte == 95 or (byte >= 48 and byte <= 57) or (byte >= 65 and byte <= 90) or (byte >= 97 and byte <= 122)
}

## An extended regular expression matching any of the programs' plain ASCII
## strings, or null when some program is not one. A buffer-wide search with
## it finds the records that can match at all.
export pure alternation(programs: List[Program]) -> Str? {
  if programs.is_empty() or programs.len() > 2000 { return null }
  var parts: List[Str] = []
  for program in programs {
    let literal = program.literal
    if literal == null or program.backrefs { return null }
    var quoted: List[Int] = []
    for byte in literal ?? b"" {
      if byte in [92, 94, 36, 46, 91, 93, 124, 40, 41, 42, 43, 63, 123, 125] { quoted += [92] }
      quoted += [byte]
    }
    match bytes.from_ints(quoted) {
      Ok(data) => { parts += [bytes_text(data) ?? ""] }
      Err(_) => { return null }
    }
  }
  parts.join("|")
}
