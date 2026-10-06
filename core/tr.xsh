#!/bin/xsh
use lib.gnu
use lib.text_a2 as text

type Options = {delete: Bool, squeeze: Bool, complement: Bool, truncate: Bool, help: Bool, version: Bool, sets: List[Str]}
type Atom = {value: Int, next: Int}

proc set_error(message: Str) {
  gnu.error(message)
  exit 1
}

proc warnings(spec: Str) {
  let data = bytes.from_text(spec)
  var at = 0
  while at < data.len() {
    let item = atom(data, at)
    if data.byte_at(at) == 92 {
      if at + 1 == data.len() { gnu.error("warning: an unescaped backslash at end of string is not portable") } else if at + 3 < data.len() and (data.byte_at(at + 1) ?? 0) in [52, 53, 54, 55] and (data.byte_at(at + 2) ?? 0) in [48, 49, 50, 51, 52, 53, 54, 55] and (data.byte_at(at + 3) ?? 0) in [48, 49, 50, 51, 52, 53, 54, 55] {
        let raw = data[at..at + 4].utf8()?
        let final = data[at + 3..at + 4].utf8()?
        gnu.error(f"warning: the ambiguous octal escape {raw} is being interpreted as the 2-byte sequence \\{item.value / 64}{item.value / 8 % 8}{item.value % 8}, {final}")
      }
    }
    at = item.next
  }
}

pure atom(data: Bytes, at: Int) -> Atom {
  let byte = data.byte_at(at) ?? 0
  if byte != 92 or at + 1 >= data.len() { return {value: byte, next: at + 1} }
  let escaped = data.byte_at(at + 1) ?? 0
  if escaped >= 48 and escaped <= 55 {
    var value = 0
    var next = at + 1
    while next < data.len() and next < at + 4 {
      let digit = data.byte_at(next) ?? 0
      break when digit < 48 or digit > 55
      value = value * 8 + digit - 48
      next += 1
    }
    return if value > 255 { {value: value / 8, next: next - 1} } else { {value: value, next: next} }
  }
  let value = if escaped == 97 { 7 } else if escaped == 98 { 8 } else if escaped == 102 { 12 } else if escaped == 110 { 10 } else if escaped == 114 { 13 } else if escaped == 116 { 9 } else if escaped == 118 { 11 } else { escaped }
  {value: value, next: at + 2}
}

pure class_member(name: Str, byte: Int) -> Bool {
  let lower = byte >= 97 and byte <= 122
  let upper = byte >= 65 and byte <= 90
  let digit = byte >= 48 and byte <= 57
  let alpha = lower or upper
  if name == "alnum" { alpha or digit } else if name == "alpha" { alpha } else if name == "blank" { byte == 9 or byte == 32 } else if name == "cntrl" { byte < 32 or byte == 127 } else if name == "digit" { digit } else if name == "graph" { byte >= 33 and byte <= 126 } else if name == "lower" { lower } else if name == "print" { byte >= 32 and byte <= 126 } else if name == "punct" { byte >= 33 and byte <= 126 and ! alpha and ! digit } else if name == "space" { byte == 32 or (byte >= 9 and byte <= 13) } else if name == "upper" { upper } else { digit or (byte >= 65 and byte <= 70) or (byte >= 97 and byte <= 102) }
}

# Logical set positions can exceed both a signed integer and the size of one
# repeat. Base-billion limbs keep those positions exact without expanding runs.
type Count = {major: Int, minor: Int}
type Run = {byte: Int, count: Count, character_class: Str}
const ZERO = {major: 0, minor: 0}
const ONE = {major: 0, minor: 1}
const MAX_REPEAT = {major: 18446744073, minor: 709551615}

pure compare(a: Count, b: Count) -> Int {
  if a.major < b.major or (a.major == b.major and a.minor < b.minor) { -1 } else if a == b { 0 } else { 1 }
}

pure add(a: Count, b: Count) -> Count {
  let minor = a.minor + b.minor
  {major: a.major + b.major + minor / 1000000000, minor: minor % 1000000000}
}

pure subtract(a: Count, b: Count) -> Count {
  let borrow = if a.minor < b.minor { 1 } else { 0 }
  {major: a.major - b.major - borrow, minor: a.minor - b.minor + borrow * 1000000000}
}

pure total(runs: List[Run]) -> Count {
  var size = ZERO
  for item in runs { size = add(size, item.count) }
  size
}

pure contains(runs: List[Run], byte: Int) -> Bool {
  for item in runs { return true when item.byte == byte }
  false
}

proc repeat_count(raw: Str) -> Count {
  var value = ZERO
  let radix = if raw.starts_with("0") { 8 } else { 10 }
  for byte in bytes.from_text(raw) {
    let digit = byte - 48
    if digit < 0 or digit >= radix { set_error(f"invalid repeat count {gnu.quote_value(raw)} in [c*n] construct") }
    let low = value.minor * radix + digit
    value = {major: value.major * radix + low / 1000000000, minor: low % 1000000000}
    if compare(value, MAX_REPEAT) > 0 { set_error(f"invalid repeat count {gnu.quote_value(raw)} in [c*n] construct") }
  }
  value
}

proc parse_set(spec: Str, fill: Count, second: Bool) -> List[Run] {
  let data = bytes.from_text(spec)
  var at = 0
  var out: List[Run] = []
  var indefinite = false
  while at < data.len() {
    if data.byte_at(at) == 91 and data.byte_at(at + 1) == 58 and data.byte_at(at + 2) != 42 {
      let rest = data[at + 2..].utf8()?
      let close = rest.find(":]")
      if close == null { set_error("missing terminating ] in character class"); exit 1 }
      let name = rest.byte_slice(0, close)
      if name == "" { set_error(f"missing character class name {gnu.quote_value("[::]")}") }
      if name not in ["alnum", "alpha", "blank", "cntrl", "digit", "graph", "lower", "print", "punct", "space", "upper", "xdigit"] { set_error(f"invalid character class {gnu.quote_value(name)}") }
      var first = true
      for byte in range(256) {
        if class_member(name, byte) { out += [{byte: byte, count: ONE, character_class: if first { name } else { "" }}]; first = false }
      }
      at += close + 4; continue
    }
    if data.byte_at(at) == 91 and data.byte_at(at + 1) == 61 and data.byte_at(at + 2) != 42 {
      if data[at..at + 4] == b"[==]" { set_error(f"missing equivalence class character {gnu.quote_value("[==]")}") }
      let item = atom(data, at + 2)
      if data.byte_at(item.next) != 61 or data.byte_at(item.next + 1) != 93 {
        let rest = data[at + 2..].utf8()?
        let close = rest.find("=]")
        if let end = close { set_error(f"{rest.byte_slice(0, end)}: equivalence class operand must be a single character") } else { set_error("invalid equivalence class") }
      }
      out += [{byte: item.value, count: ONE, character_class: ""}]; at = item.next + 2; continue
    }
    if data.byte_at(at) == 91 {
      let item = atom(data, at + 1)
      if data.byte_at(item.next) == 42 {
        let rest = data[item.next + 1..].utf8()?
        let close = rest.find("]")
        if close != null and ! rest.starts_with("\\") {
          let raw = rest.byte_slice(0, close)
          let count = repeat_count(raw)
          let next = item.next + close + 2
          var size = count
          if count == ZERO {
            if ! second { set_error("the [c*] repeat construct may not appear in string1") }
            if indefinite { set_error("only one [c*] repeat construct may appear in string2") }
            indefinite = true
            let suffix = total(parse_set(spec.byte_slice(next), ZERO, true))
            let used = add(total(out), suffix)
            size = if compare(fill, used) > 0 { subtract(fill, used) } else { ZERO }
          }
          if size != ZERO { out += [{byte: item.value, count: size, character_class: ""}] }
          at = next; continue
        }
      }
    }
    let first = atom(data, at)
    if data.byte_at(first.next) == 45 and first.next + 1 < data.len() {
      let last = atom(data, first.next + 1)
      if first.value > last.value { set_error(f"range-endpoints of {gnu.quote_value_bytes(bytes.from_ints([first.value, 45, last.value])?)} are in reverse collating sequence order") }
      out += [{byte: byte, count: ONE, character_class: ""} for byte in range(first.value, last.value + 1)]
      at = last.next
    } else { out += [{byte: first.value, count: ONE, character_class: ""}]; at = first.next }
  }
  out
}

proc validate_classes(first_spec: Str, second_spec: Str, first: List[Run], second: List[Run], truncate: Bool) {
  var position = ZERO
  for item in second {
    if item.character_class != "" {
      if item.character_class not in ["upper", "lower"] { set_error("when translating, the only character classes that may appear in string2 are 'upper' and 'lower'") }
      var matched = false
      var first_position = ZERO
      for source in first {
        if source.character_class in ["upper", "lower"] and first_position == position { matched = true }
        first_position = add(first_position, source.count)
      }
      if ! matched { set_error("misaligned [:upper:] and/or [:lower:] construct") }
    }
    position = add(position, item.count)
  }
  if ! truncate and compare(total(first), total(second)) > 0 and (second_spec.ends_with("[:upper:]") or second_spec.ends_with("[:lower:]")) {
    set_error("when translating with string1 longer than string2, the latter string must not end with a character class")
  }
}

pure value_at(runs: List[Run], position: Count) -> Int {
  var start = ZERO
  for item in runs {
    let end = add(start, item.count)
    if compare(position, end) < 0 { return item.byte }
    start = end
  }
  runs[-1].byte
}

proc main(...argv: List[Str]) {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1, permute: false},
    delete: {form: "-d --delete", default: false},
    squeeze: {form: "-s --squeeze-repeats", default: false},
    complement: {form: "-c -C --complement", default: false},
    truncate: {form: "-t --truncate-set1", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    sets: {form: "...STRING"},
  })?
  if opts.help { gnu.help("Usage: tr [OPTION]... STRING1 [STRING2]\nTranslate, squeeze, or delete characters from standard input."); return }
  if opts.version { gnu.version("tr"); return }
  if opts.sets.is_empty() { gnu.missing_operand() }
  if opts.sets.len() > 2 { gnu.extra_operand(opts.sets[2]) }
  if opts.delete and ! opts.squeeze and opts.sets.len() > 1 { gnu.usage_error(f"extra operand {gnu.quote_value(opts.sets[1])}\nOnly one string may be given when deleting without squeezing repeats.") }
  if opts.sets.len() == 1 and opts.delete and opts.squeeze { gnu.usage_error(f"missing operand after {gnu.quote_value(opts.sets[0])}\nTwo strings must be given when both deleting and squeezing repeats.") }
  if opts.sets.len() == 1 and ! opts.delete and ! opts.squeeze { gnu.missing_operand_after(opts.sets[0]) }
  warnings(opts.sets[0])
  if opts.sets.len() == 2 { warnings(opts.sets[1]) }
  let original = parse_set(opts.sets[0], ZERO, false)
  let first: List[Run] = if opts.complement { [{byte: byte, count: ONE, character_class: ""} for byte in range(256) if ! contains(original, byte)] } else { original }
  let second = if opts.sets.len() == 2 { parse_set(opts.sets[1], total(first), true) } else { [] }
  let translate = ! opts.delete and opts.sets.len() == 2
  if translate { validate_classes(opts.sets[0], opts.sets[1], original, second, opts.truncate) }
  if translate and ! opts.truncate and second.is_empty() and ! first.is_empty() { set_error("when not truncating set1, string2 must be non-empty") }
  if translate and opts.complement and "[:" in opts.sets[0] {
    let unique = [byte for byte in range(256) if contains(second, byte)]
    if unique.len() > 1 or compare(total(second), total(first)) > 0 or (opts.truncate and compare(total(first), total(second)) > 0) { set_error("when translating with complemented character classes,\nstring2 must map all characters in the domain to one") }
  }
  let squeeze_set = if opts.sets.len() == 2 { second } else { first }
  let translation: List[Int] = collect {
    for byte in range(256) {
      var value = byte
      var start = ZERO
      for item in first {
        let end = add(start, item.count)
        if translate and item.byte == byte and (! opts.truncate or compare(start, total(second)) < 0) {
          let boundary = if opts.truncate and compare(end, total(second)) > 0 { total(second) } else { end }
          value = value_at(second, subtract(boundary, ONE))
        }
        start = end
      }
      yield value
    }
  }
  let deletes = [opts.delete and contains(first, byte) for byte in range(256)]
  let squeezes = [opts.squeeze and contains(squeeze_set, byte) for byte in range(256)]
  var previous = -1
  loop {
    guard let source = io.stdin_read(65536) else { |failure|
      gnu.error(f"read error: {gnu.strerror(failure)}")
      exit 1
    }
    break when source.is_empty()
    let output: List[Int] = collect {
      for at in range(source.len()) {
        let original = source.byte_at(at) ?? 0
        if deletes[original] { continue }
        let value = translation[original]
        if previous != value or ! squeezes[value] { yield value }
        previous = value
      }
    }
    gnu.write_bytes(bytes.from_ints(output)?)
  }
  exit text.finish(false)
}
