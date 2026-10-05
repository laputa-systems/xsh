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

proc parse_set(spec: Str, fill: Int, second: Bool) -> List[Int] {
  let data = bytes.from_text(spec)
  var at = 0
  var out: List[Int] = []
  var indefinite = false
  while at < data.len() {
    if data.byte_at(at) == 91 and data.byte_at(at + 1) == 58 and data.byte_at(at + 2) != 42 {
      let rest = data[at + 2..].utf8()?
      let close = rest.find(":]")
      if close == null { set_error("missing terminating ] in character class"); exit 1 }
      let name = rest.byte_slice(0, close)
      if name not in ["alnum", "alpha", "blank", "cntrl", "digit", "graph", "lower", "print", "punct", "space", "upper", "xdigit"] { set_error(f"invalid character class {gnu.quote_value(name)}") }
      out += [byte for byte in range(256) if class_member(name, byte)]
      at += close + 4
      continue
    }
    if data.byte_at(at) == 91 and data.byte_at(at + 1) == 61 {
      let item = atom(data, at + 2)
      if data.byte_at(item.next) != 61 or data.byte_at(item.next + 1) != 93 { set_error("invalid equivalence class") }
      out += [item.value]; at = item.next + 2; continue
    }
    if data.byte_at(at) == 91 {
      let item = atom(data, at + 1)
      if data.byte_at(item.next) == 42 {
        let rest = data[item.next + 1..].utf8()?
        let close = rest.find("]")
        if close != null {
          let count_text = rest.byte_slice(0, close)
          var count = if count_text == "" { 0 } else { count_text.parse_int() ?? -1 }
          if count_text.starts_with("0") {
            count = 0
            for digit in bytes.from_text(count_text) {
              if digit < 48 or digit > 55 { set_error("invalid repeat count") }
              count = count * 8 + digit - 48
            }
          }
          if ! second and count == 0 { set_error("the [c*] repeat construct may not appear in string1") }
          if count < 0 { set_error("invalid repeat count") }
          let next = item.next + close + 2
          var size = count
          if count == 0 {
            if indefinite { set_error("only one [c*] repeat construct may appear in string2") }
            indefinite = true
            let suffix = parse_set(spec.byte_slice(next), 0, true)
            let available = fill - out.len() - suffix.len()
            size = if available > 0 { available } else { 0 }
          }
          out += [item.value for _ in range(size)]
          at = next; continue
        }
      }
    }
    let first = atom(data, at)
    if data.byte_at(first.next) == 45 and first.next + 1 < data.len() {
      let last = atom(data, first.next + 1)
      if first.value > last.value { set_error("range-endpoints are in reverse collating sequence order") }
      out += [byte for byte in range(first.value, last.value + 1)]
      at = last.next
    } else { out += [first.value]; at = first.next }
  }
  out
}

proc validate_classes(first_spec: Str, second_spec: Str, first: List[Int], second: List[Int], truncate: Bool) {
  for name in ["alnum", "alpha", "blank", "cntrl", "digit", "graph", "print", "punct", "space", "xdigit"] {
    if f"[:{name}:]" in second_spec { set_error("when translating, the only character classes that may appear in string2 are 'upper' and 'lower'") }
  }
  var at = 0
  while at < second_spec.byte_len() {
    let tail = second_spec.byte_slice(at)
    if tail.starts_with("[:lower:]") or tail.starts_with("[:upper:]") {
      let position = parse_set(second_spec.byte_slice(0, at), first.len(), true).len()
      var matched = false
      for offset in range(first_spec.byte_len()) {
        let rest = first_spec.byte_slice(offset)
        if rest.starts_with("[:lower:]") or rest.starts_with("[:upper:]") {
          if parse_set(first_spec.byte_slice(0, offset), 0, false).len() == position { matched = true }
        }
      }
      if ! matched { set_error("misaligned [:upper:] and/or [:lower:] construct") }
      at += 9
    } else { at += 1 }
  }
  if ! truncate and first.len() > second.len() and (second_spec.ends_with("[:upper:]") or second_spec.ends_with("[:lower:]")) {
    set_error("when translating with string1 longer than string2, the latter string must not end with a character class")
  }
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
  if opts.delete and ! opts.squeeze and opts.sets.len() > 1 { gnu.extra_operand(opts.sets[1]) }
  if opts.sets.len() == 1 and ((! opts.delete and ! opts.squeeze) or (opts.delete and opts.squeeze)) { gnu.missing_operand_after(opts.sets[0]) }
  warnings(opts.sets[0])
  if opts.sets.len() == 2 { warnings(opts.sets[1]) }
  var first = parse_set(opts.sets[0], 0, false)
  if opts.complement { first = [byte for byte in range(256) if byte not in first] }
  let second = if opts.sets.len() == 2 { parse_set(opts.sets[1], first.len(), true) } else { [] }
  let translate = ! opts.delete and opts.sets.len() == 2
  if translate { validate_classes(opts.sets[0], opts.sets[1], first, second, opts.truncate) }
  if translate and second.is_empty() and ! first.is_empty() { set_error("when not truncating set1, string2 must be non-empty") }
  if translate and opts.complement and "[:" in opts.sets[0] {
    var unique: List[Int] = []
    for value in second { if value not in unique { unique += [value] } }
    if unique.len() > 1 { set_error("when translating with complemented character classes,\nstring2 must map all characters in the domain to one") }
  }
  if opts.truncate and first.len() > second.len() and translate { first = first[..second.len()] }
  let squeeze_set = if opts.sets.len() == 2 { second } else { first }
  let translation: List[Int] = collect {
    for byte in range(256) {
      var value = byte
      if translate {
        for item in first |> enumerate() {
          if item.value == byte { value = second[if item.index < second.len() { item.index } else { second.len() - 1 }] }
        }
      }
      yield value
    }
  }
  let deletes = [opts.delete and byte in first for byte in range(256)]
  let squeezes = [opts.squeeze and byte in squeeze_set for byte in range(256)]
  let source = io.stdin_bytes()?
  var previous = -1
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
  exit text.finish(false)
}
