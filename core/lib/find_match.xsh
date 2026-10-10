##! Name matching for find: shell wildcards with POSIX character classes, and
##! the translation of find's default Emacs regular-expression dialect.

# A POSIX class name tested against one byte of the C locale.
pure in_class(name: Str, byte: Int) -> Bool {
  let upper = byte >= 65 and byte <= 90
  let lower = byte >= 97 and byte <= 122
  let digit = byte >= 48 and byte <= 57
  match name {
    "alpha" => upper or lower
    "digit" => digit
    "alnum" => upper or lower or digit
    "upper" => upper
    "lower" => lower
    "space" => byte == 32 or (byte >= 9 and byte <= 13)
    "blank" => byte == 32 or byte == 9
    "punct" => byte > 32 and byte < 127 and ! (upper or lower or digit)
    "print" => byte >= 32 and byte < 127
    "graph" => byte > 32 and byte < 127
    "cntrl" => byte < 32 or byte == 127
    "xdigit" => digit or (byte >= 65 and byte <= 70) or (byte >= 97 and byte <= 102)
    _ => false
  }
}

pure fold(byte: Int, insensitive: Bool) -> Int {
  if insensitive and byte >= 65 and byte <= 90 { byte + 32 } else { byte }
}

# Position just past the bracket expression that opens at `open`, or -1 when
# it never closes. A `]` right after the opening (or after `!`/`^`) is literal.
pure bracket_end(pattern: Bytes, open: Int) -> Int {
  var at = open + 1
  if at < pattern.len() and (pattern.byte_at(at) == 33 or pattern.byte_at(at) == 94) { at += 1 }
  if at < pattern.len() and pattern.byte_at(at) == 93 { at += 1 }
  while at < pattern.len() {
    let byte = pattern.byte_at(at) ?? 0
    if byte == 93 { return at + 1 }
    if byte == 91 and at + 1 < pattern.len() and pattern.byte_at(at + 1) == 58 {
      var close = at + 2
      while close + 1 < pattern.len() and ! (pattern.byte_at(close) == 58 and pattern.byte_at(close + 1) == 93) { close += 1 }
      if close + 1 < pattern.len() { at = close + 2; continue }
    }
    at += 1
  }
  -1
}

pure bracket_matches(pattern: Bytes, open: Int, end: Int, byte: Int, insensitive: Bool) -> Bool {
  var at = open + 1
  var negate = false
  if pattern.byte_at(at) == 33 or pattern.byte_at(at) == 94 { negate = true; at += 1 }
  let value = fold(byte, insensitive)
  var found = false
  var first = true
  while at < end - 1 {
    let current = pattern.byte_at(at) ?? 0
    if current == 91 and at + 1 < end and pattern.byte_at(at + 1) == 58 {
      var close = at + 2
      while close + 1 < end and ! (pattern.byte_at(close) == 58 and pattern.byte_at(close + 1) == 93) { close += 1 }
      if close + 1 < end {
        let name = pattern[at + 2..close].utf8() ?? ""
        if in_class(name, byte) or (insensitive and (in_class(name, fold(byte, true)) or ((name == "upper" or name == "lower") and in_class("alpha", byte)))) { found = true }
        at = close + 2
        first = false
        continue
      }
    }
    let low = fold(current, insensitive)
    if at + 2 < end - 1 and pattern.byte_at(at + 1) == 45 {
      let high = fold(pattern.byte_at(at + 2) ?? 0, insensitive)
      if value >= low and value <= high { found = true }
      at += 3
    } else {
      if value == low { found = true }
      at += 1
    }
    first = false
  }
  found != negate
}

pure glob_at(pattern: Bytes, text: Bytes, p: Int, t: Int, insensitive: Bool) -> Bool {
  return t == text.len() when p >= pattern.len()
  let char = pattern.byte_at(p) ?? -1
  if char == 42 {
    var next = p + 1
    while pattern.byte_at(next) == 42 { next += 1 }
    return true when next == pattern.len()
    for at in range(t, text.len() + 1) {
      return true when glob_at(pattern, text, next, at, insensitive)
    }
    return false
  }
  return false when t >= text.len()
  let value = text.byte_at(t) ?? -1
  if char == 63 { return glob_at(pattern, text, p + 1, t + 1, insensitive) }
  if char == 91 {
    let end = bracket_end(pattern, p)
    if end > 0 {
      return bracket_matches(pattern, p, end, value, insensitive) and glob_at(pattern, text, end, t + 1, insensitive)
    }
  }
  let escaped = char == 92 and p + 1 < pattern.len()
  let literal = if escaped { pattern.byte_at(p + 1) ?? -1 } else { char }
  fold(value, insensitive) == fold(literal, insensitive) and glob_at(pattern, text, p + (if escaped { 2 } else { 1 }), t + 1, insensitive)
}

## Whether the shell wildcard pattern matches all of `text`; `*` also matches
## `/` and a leading dot, as in fnmatch without FNM_PATHNAME or FNM_PERIOD.
export pure glob_bytes(pattern: Bytes, text: Bytes, insensitive = false) -> Bool {
  glob_at(pattern, text, 0, 0, insensitive)
}

## The last component of a path, keeping a trailing slash out of the name
## and returning `/` for a path made only of slashes.
export pure basename(raw: Bytes) -> Bytes {
  var end = raw.len()
  while end > 1 and raw.byte_at(end - 1) == 47 { end -= 1 }
  if end == 1 and raw.byte_at(0) == 47 { return b"/" }
  var at = end
  while at > 0 and raw.byte_at(at - 1) != 47 { at -= 1 }
  raw[at..end]
}

## Convert an Emacs-syntax pattern, find's default dialect, to extended
## syntax: groups, alternation and intervals are backslashed in Emacs and
## bare in extended, and the plain characters are the reverse. A dot does
## not match a newline in this dialect.
export pure emacs_to_extended(pattern: Str) -> Str {
  var out = ""
  var at = 0
  var in_bracket = false
  while at < pattern.byte_len() {
    let char = pattern.byte_slice(at, 1)
    if in_bracket {
      out = f"{out}{char}"
      if char == "]" and at > 0 { in_bracket = false }
      at += 1
    } else if char == "[" {
      in_bracket = true
      out = f"{out}["
      at += 1
      if pattern.byte_slice(at, 1) == "^" { out = f"{out}^"; at += 1 }
      if pattern.byte_slice(at, 1) == "]" { out = f"{out}]"; at += 1 }
    } else if char == "\\" and at + 1 < pattern.byte_len() {
      let next = pattern.byte_slice(at + 1, 1)
      out = f"{out}{if next in ["(", ")", "|", "{", "}"] { next } else { f"\\{next}" }}"
      at += 2
    } else if char in ["(", ")", "|", "{", "}"] {
      out = f"{out}\\{char}"
      at += 1
    } else if char == "." {
      out = f"{out}[^\n]"
      at += 1
    } else {
      out = f"{out}{char}"
      at += 1
    }
  }
  out
}
