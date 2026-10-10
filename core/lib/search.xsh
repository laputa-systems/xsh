##! Byte-oriented glob, literal search and path helpers shared by find, diff,
##! xargs and grep.

error SearchError = Invalid : Usage

## Return an input error so applets can choose their own diagnostic status.
export pure reject(message: Str) -> Result[Unit, Error] {
  Err(SearchError.Invalid(message))
}

## The final path component, preserving undecodable filename bytes.
export pure basename_bytes(raw: Bytes) -> Bytes {
  var end = raw.len()
  while end > 1 and raw.byte_at(end - 1) == 47 { end -= 1 }
  var at = end
  while at > 0 and raw.byte_at(at - 1) != 47 { at -= 1 }
  raw[at..end]
}

## Join a parent with a raw filename without decoding it.
export pure child(parent: Path, name: Bytes) -> Result[Path, Error] {
  let leaf = Path.parse_bytes(name)?
  Ok(if parent.bytes().byte_at(parent.bytes().len() - 1) == 47 { fp"{parent}{leaf}" } else { fp"{parent}/{leaf}" })
}

## ASCII case folding leaves all other bytes unchanged.
export pure fold(byte: Int, insensitive: Bool) -> Int {
  if insensitive and byte >= 65 and byte <= 90 { byte + 32 } else { byte }
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
  let value = fold(text.byte_at(t) ?? -1, insensitive)
  if char == 63 { return glob_at(pattern, text, p + 1, t + 1, insensitive) }
  if char == 91 {
    var at = p + 1
    let negate = pattern.byte_at(at) == 33 or pattern.byte_at(at) == 94
    if negate { at += 1 }
    var matched = false
    var items = 0
    while at < pattern.len() and (pattern.byte_at(at) != 93 or items == 0) {
      let low = fold(pattern.byte_at(at) ?? -1, insensitive)
      if pattern.byte_at(at + 1) == 45 and at + 2 < pattern.len() and pattern.byte_at(at + 2) != 93 {
        let high = fold(pattern.byte_at(at + 2) ?? -1, insensitive)
        matched = matched or (value >= low and value <= high)
        at += 3
      } else {
        matched = matched or value == low
        at += 1
      }
      items += 1
    }
    if pattern.byte_at(at) == 93 {
      return matched != negate and glob_at(pattern, text, at + 1, t + 1, insensitive)
    }
  }
  let escaped = char == 92 and p + 1 < pattern.len()
  let literal = if escaped { pattern.byte_at(p + 1) ?? -1 } else { char }
  value == fold(literal, insensitive) and glob_at(pattern, text, p + (if escaped { 2 } else { 1 }), t + 1, insensitive)
}

## Match shell filename wildcards over bytes, with optional ASCII case folding.
export pure glob(pattern: Str, text: Bytes, insensitive = false) -> Bool {
  glob_at(bytes.from_text(pattern), text, 0, 0, insensitive)
}

## Find a literal byte sequence starting at the requested offset.
export pure find_fixed(text: Bytes, needle: Bytes, offset = 0, insensitive = false) -> Int? {
  return offset when needle.is_empty() and offset <= text.len()
  for at in range(offset, text.len() - needle.len() + 1) {
    var same = true
    for index in range(needle.len()) {
      if fold(text.byte_at(at + index) ?? -1, insensitive) != fold(needle.byte_at(index) ?? -1, insensitive) {
        same = false
        break
      }
    }
    return at when same
  }
  null
}

## Split records without inventing an extra record after a final delimiter.
export pure records(data: Bytes, delimiter: Int) -> List[Bytes] {
  var out: List[Bytes] = []
  var start = 0
  for at in range(data.len()) {
    if data.byte_at(at) == delimiter { out += [data[start..at]]; start = at + 1 }
  }
  if start < data.len() { out += [data[start..]] }
  out
}
