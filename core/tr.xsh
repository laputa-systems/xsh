#!/bin/xsh
use lib.gnu

const USAGE = """
Usage: tr [OPTION]... SET1 [SET2]
Translate, squeeze, and/or delete characters from standard input.
  -c, -C, --complement  use the complement of SET1
  -d, --delete          delete characters in SET1
  -s, --squeeze-repeats replace each sequence of repeated characters with one
  -t, --truncate-set1   first truncate SET1 to length of SET2
      --help            display this help and exit
      --version         output version information and exit
"""

type TrAtom = {value: Bytes, dash: Bool, star: Bool}
type TrRepeat = {found: Bool, size: Int, value: Bytes, count: Int, star: Bool, error: Str}
type ClassHit = {name: Str, size: Int}
type ClassPosition = {name: Str, offset: Int}
type TrInvocation = {delete: Bool, squeeze: Bool, complement: Bool, truncate: Bool, operands: List[Bytes], error: Str}

pure unit_size(data: Bytes, at: Int) -> Int {
  let lead = data.byte_at(at) ?? 0
  let size = if lead >= 194 and lead <= 223 { 2 } else if lead >= 224 and lead <= 239 { 3 } else if lead >= 240 and lead <= 244 { 4 } else { 1 }
  if size > 1 and at + size <= data.len() and (data[at..at + size].utf8() ?? "") != "" { size } else { 1 }
}

pure byte(value: Int) -> Bytes { bytes.from_ints([value % 256]) ?? b"" }

pure diagnostic_char(value: Bytes) -> Str {
  if value.len() == 1 {
    let n = value.byte_at(0) ?? 0
    if n < 32 or n == 127 { return f"\\{n / 64}{n / 8 % 8}{n % 8}" }
  }
  value.utf8() ?? "?"
}

pure class_values(name: Str) -> List[Bytes] {
  var out: List[Bytes] = []
  for n in range(0, 128) {
    let alpha = (n >= 65 and n <= 90) or (n >= 97 and n <= 122)
    let digit = n >= 48 and n <= 57
    let blank = n == 9 or n == 32
    let space = (n >= 9 and n <= 13) or n == 32
    let control = n < 32 or n == 127
    let graph = n >= 33 and n <= 126
    let printable = n >= 32 and n <= 126
    let selected = if name == "alnum" { alpha or digit } else if name == "alpha" { alpha } else if name == "blank" { blank } else if name == "cntrl" { control } else if name == "digit" { digit } else if name == "graph" { graph } else if name == "lower" { n >= 97 and n <= 122 } else if name == "print" { printable } else if name == "punct" { graph and ! alpha and ! digit } else if name == "space" { space } else if name == "upper" { n >= 65 and n <= 90 } else if name == "xdigit" { digit or (n >= 65 and n <= 70) or (n >= 97 and n <= 102) } else { false }
    if selected { out += [byte(n)] }
  }
  out
}

pure class_at(data: Bytes, at: Int) -> ClassHit {
  for name in ["alnum", "alpha", "blank", "cntrl", "digit", "graph", "lower", "print", "punct", "space", "upper", "xdigit"] {
    let token = bytes.from_text(f"[:{name}:]")
    if at + token.len() <= data.len() and data[at..at + token.len()] == token { return {name: name, size: token.len()} }
  }
  {name: "", size: 0}
}

pure repeat_at(data: Bytes, at: Int) -> TrRepeat {
  if at + 2 >= data.len() or data.byte_at(at) != 91 { return {found: false, size: 0, value: b"", count: 0, star: false, error: ""} }
  let value_size = unit_size(data, at + 1)
  let star_at = at + 1 + value_size
  if star_at >= data.len() or data.byte_at(star_at) != 42 { return {found: false, size: 0, value: b"", count: 0, star: false, error: ""} }
  var close = star_at + 1
  while close < data.len() and data.byte_at(close) != 93 { close += 1 }
  if close >= data.len() { return {found: false, size: 0, value: b"", count: 0, star: false, error: ""} }
  let raw_count = data[star_at + 1..close]
  let text = raw_count.utf8() ?? ""
  if raw_count.len() == 0 { return {found: true, size: close - at + 1, value: data[at + 1..star_at], count: 0, star: true, error: ""} }
  if raw_count.byte_at(0) == 92 { return {found: false, size: 0, value: b"", count: 0, star: false, error: ""} }
  if ! rx"^[0-9]+$".matches(text) {
    return {found: true, size: close - at + 1, value: data[at + 1..star_at], count: 0, star: false, error: f"invalid repeat count '{text}' in [c*n] construct"}
  }
  var count = 0
  var residue = 0
  let octal = raw_count.len() > 1 and raw_count.byte_at(0) == 48
  for digit in range(raw_count.len()) {
    let n = raw_count.byte_at(digit) ?? 48
    if octal and n > 55 { return {found: true, size: close - at + 1, value: data[at + 1..star_at], count: 0, star: false, error: f"invalid repeat count '{text}' in [c*n] construct"} }
    let base = if octal { 8 } else { 10 }
    residue = (residue * base + n - 48) % 4096
    count = if count >= 8192 { 8192 } else { count * base + n - 48 }
  }
  if count >= 8192 { count = 8192 + residue }
  {found: true, size: close - at + 1, value: data[at + 1..star_at], count: count, star: false, error: ""}
}

proc parse_atoms(data: Bytes, string1: Bool) [error] -> Result[List[TrAtom], Str] {
  var atoms: List[TrAtom] = []
  var at = 0
  while at < data.len() {
    let repeat = repeat_at(data, at)
    if repeat.found {
      if repeat.error != "" { return Err(repeat.error) }
      if repeat.star and string1 { return Err("the [c*] repeat construct may not appear in string1") }
      if repeat.star {
        atoms += [{value: repeat.value, dash: false, star: true}]
      } else {
        let repeat_count = if repeat.count == 0 and ! string1 { 1 } else { repeat.count }
        for _ in range(repeat_count) { atoms += [{value: repeat.value, dash: false, star: false}] }
      }
      at += repeat.size
    } else if data.byte_at(at) == 91 and at + 1 < data.len() and data.byte_at(at + 1) == 58 {
      var close = at + 2
      while close + 1 < data.len() and !(data.byte_at(close) == 58 and data.byte_at(close + 1) == 93) { close += 1 }
      if close + 1 >= data.len() { atoms += [{value: data[at..at + 1], dash: false, star: false}]; at += 1 } else {
        let name = data[at + 2..close].utf8() ?? "?"
        if name == "" { return Err("missing character class name '[::]'") }
        if !(name in ["alnum", "alpha", "blank", "cntrl", "digit", "graph", "lower", "print", "punct", "space", "upper", "xdigit"]) { return Err(f"invalid character class '{name}'") }
        for value in class_values(name) { atoms += [{value: value, dash: false, star: false}] }
        at = close + 2
      }
    } else if data.byte_at(at) == 91 and at + 1 < data.len() and data.byte_at(at + 1) == 61 {
      var close = at + 2
      while close + 1 < data.len() and !(data.byte_at(close) == 61 and data.byte_at(close + 1) == 93) { close += 1 }
      if close + 1 >= data.len() { atoms += [{value: data[at..at + 1], dash: false, star: false}]; at += 1 } else {
        let value = data[at + 2..close]
        if value.len() == 0 { return Err("missing equivalence class character '[==]'") }
        if unit_size(value, 0) != value.len() { return Err(f"{value.utf8() ?? "?"}: equivalence class operand must be a single character") }
        atoms += [{value: value, dash: false, star: false}]
        at = close + 2
      }
    } else if class_at(data, at).size > 0 {
      let class = class_at(data, at)
      for value in class_values(class.name) { atoms += [{value: value, dash: false, star: false}] }
      at += class.size
    } else if data.byte_at(at) == 92 {
      if at + 1 >= data.len() {
        atoms += [{value: b"\\", dash: false, star: false}]
        at += 1
      } else {
        let escaped = data.byte_at(at + 1) ?? 0
        let code = if escaped == 97 { 7 } else if escaped == 98 { 8 } else if escaped == 102 { 12 } else if escaped == 110 { 10 } else if escaped == 114 { 13 } else if escaped == 116 { 9 } else if escaped == 118 { 11 } else { -1 }
        if code >= 0 {
          atoms += [{value: byte(code), dash: false, star: false}]
          at += 2
        } else if escaped >= 48 and escaped <= 55 {
          var value = 0
          var digits = 0
          while digits < 3 and at + 1 + digits < data.len() {
            let n = data.byte_at(at + 1 + digits) ?? 0
            if n < 48 or n > 55 { break }
            value = value * 8 + n - 48
            digits += 1
          }
          if digits == 3 and value > 255 {
            let first_value = ((data.byte_at(at + 1) ?? 48) - 48) * 8 + ((data.byte_at(at + 2) ?? 48) - 48)
            atoms += [{value: byte(first_value), dash: false, star: false}]
            atoms += [{value: byte(data.byte_at(at + 3) ?? 49), dash: false, star: false}]
            at += 4
          } else {
            atoms += [{value: byte(value), dash: false, star: false}]
            at += 1 + digits
          }
        } else {
          let size = unit_size(data, at + 1)
          atoms += [{value: data[at + 1..at + 1 + size], dash: false, star: false}]
          at += 1 + size
        }
      }
    } else {
      let size = unit_size(data, at)
      let value = data[at..at + size]
      atoms += [{value: value, dash: value == b"-", star: false}]
      at += size
    }
  }
  Ok(atoms)
}

proc parse_set(data: Bytes, string1: Bool, target: Int) [error] -> Result[List[Bytes], Str] {
  let atoms = parse_atoms(data, string1)?
  var star_count = 0
  for atom in atoms { if atom.star { star_count += 1 } }
  if star_count > 1 { return Err("only one [c*] repeat construct may appear in string2") }
  var out: List[Bytes] = []
  var at = 0
  while at < atoms.len() {
    if atoms[at].star {
      var suffix = 0
      for later in range(at + 1, atoms.len()) { if ! atoms[later].star { suffix += 1 } }
      let repeats = if target > out.len() + suffix { target - out.len() - suffix } else { 0 }
      for _ in range(repeats) { out += [atoms[at].value] }
      at += 1
    } else if at + 2 < atoms.len() and atoms[at + 1].dash and atoms[at].value.len() == 1 and atoms[at + 2].value.len() == 1 {
      let first = atoms[at].value.byte_at(0) ?? 0
      let last = atoms[at + 2].value.byte_at(0) ?? 0
      if first > last { return Err(f"range-endpoints of '{diagnostic_char(atoms[at].value)}-{diagnostic_char(atoms[at + 2].value)}' are in reverse collating sequence order") }
      for n in range(first, last + 1) { out += [byte(n)] }
      at += 3
    } else {
      out += [atoms[at].value]
      at += 1
    }
  }
  Ok(out)
}

proc class_positions(data: Bytes) [error] -> Result[List[ClassPosition], Str] {
  var out: List[ClassPosition] = []
  var at = 0
  while at + 1 < data.len() {
    if data.byte_at(at) == 91 and data.byte_at(at + 1) == 58 {
      var close = at + 2
      while close + 1 < data.len() and !(data.byte_at(close) == 58 and data.byte_at(close + 1) == 93) { close += 1 }
      if close + 1 < data.len() {
        let before = parse_set(data[..at], false, 0)?
        let name = data[at + 2..close].utf8() ?? "?"
        out += [{name: name, offset: before.len()}]
        at = close + 2
      } else { at += 1 }
    } else { at += 1 }
  }
  Ok(out)
}

pure contains(values: List[Bytes], value: Bytes) -> Bool {
  for item in values { return true when item == value }
  false
}

pure index_of(values: List[Bytes], value: Bytes) -> Int {
  var found = -1
  for at in range(values.len()) { if values[at] == value { found = at } }
  found
}

pure unique_count(values: List[Bytes]) -> Int {
  var unique: List[Bytes] = []
  for value in values { if ! contains(unique, value) { unique += [value] } }
  unique.len()
}

pure has_unescaped_trailing_backslash(value: Bytes) -> Bool {
  var count = 0
  var at = value.len() - 1
  while at >= 0 and value.byte_at(at) == 92 { count += 1; at -= 1 }
  count % 2 == 1
}

pure complement(values: List[Bytes]) -> List[Bytes] {
  var out: List[Bytes] = []
  for n in range(0, 256) {
    let item = byte(n)
    if ! contains(values, item) { out += [item] }
  }
  out
}

proc invocation(argv: List[Str], raw: List[Bytes]) [env] -> TrInvocation {
  var delete = false
  var squeeze = false
  var complement_set = false
  var truncate = false
  var operands: List[Bytes] = []
  var stopped = false
  var error = ""
  var index = 0
  while index < argv.len() {
    let arg = argv[index]
    if ! stopped and arg == "--" { stopped = true; index += 1; continue }
    if ! stopped and operands.len() == 0 and arg.starts_with("--") {
      if arg == "--help" or arg == "--version" { stopped = true; operands += [raw[index]]; index += 1; continue }
      if arg == "--delete" { delete = true } else if arg == "--squeeze-repeats" { squeeze = true } else if arg == "--complement" { complement_set = true } else if arg == "--truncate-set1" { truncate = true } else { error = f"unrecognized option {gnu.quote_bytes(raw[index])}" }
      index += 1
      continue
    }
    if ! stopped and operands.len() == 0 and arg.starts_with("-") and arg != "-" {
      for at in range(1, arg.byte_len()) {
        let flag = arg.byte_slice(at, length: 1)
        if flag == "d" { delete = true } else if flag == "s" { squeeze = true } else if flag == "c" or flag == "C" { complement_set = true } else if flag == "t" { truncate = true } else { error = f"invalid option -- '{flag}'" }
      }
      index += 1
      continue
    }
    operands += [raw[index]]
    index += 1
  }
  {delete: delete, squeeze: squeeze, complement: complement_set, truncate: truncate, operands: operands, error: error}
}

pure array_get(values: List[Bytes], index: Int) -> Bytes {
  if index >= 0 and index < values.len() { values[index] } else { b"" }
}

pure transform(input: Bytes, set1: List[Bytes], set2: List[Bytes], delete: Bool, squeeze: Bool,
  squeeze_set: List[Bytes]) -> Bytes {
  var out: List[Bytes] = []
  var previous = b""
  var at = 0
  while at < input.len() {
    let unit = input[at..at + unit_size(input, at)]
    var keep = true
    var value = unit
    let index = index_of(set1, unit)
    if delete and index >= 0 { keep = false } else if ! delete and index >= 0 and set2.len() > 0 {
      value = array_get(set2, if index < set2.len() { index } else { set2.len() - 1 })
    }
    if keep and squeeze and value == previous and contains(squeeze_set, value) { keep = false }
    if keep { out += [value]; previous = value } else if ! delete { previous = value }
    at += unit.len()
  }
  bytes.concat(out)
}

proc main(...argv: List[Str]) [process, env, error, io] {
  if "--help" in argv { gnu.help(USAGE); return }
  if "--version" in argv { gnu.version("tr"); return }
  let args = invocation(argv, cli.argv_bytes())
  if args.error != "" { gnu.usage_error(args.error) }
  let count = args.operands.len()
  if args.delete {
    if count == 0 { gnu.error("missing operand"); exit 1 }
    if args.squeeze and count < 2 { gnu.error(f"missing operand after {gnu.quote_bytes(args.operands[0])}\nTwo strings must be given when both deleting and squeezing repeats."); exit 1 }
    if count > 1 and ! args.squeeze { gnu.error(f"extra operand {gnu.quote_bytes(args.operands[1])}\nOnly one string may be given when deleting without squeezing repeats."); exit 1 }
    if count > 2 { gnu.error(f"extra operand {gnu.quote_bytes(args.operands[2])}"); exit 1 }
  } else if args.squeeze and count == 1 {
  } else if count < 2 {
    if count == 1 { gnu.error(f"missing operand after {gnu.quote_bytes(args.operands[0])}") } else { gnu.error("missing operand") }
    exit 1
  } else if count > 2 { gnu.error(f"extra operand {gnu.quote_bytes(args.operands[2])}"); exit 1 }
  for operand in args.operands {
    if has_unescaped_trailing_backslash(operand) { gnu.error("warning: an unescaped backslash at end of string is not portable") }
    var at = 0
    while at + 3 < operand.len() {
      if operand.byte_at(at) == 92 and (operand.byte_at(at + 1) ?? 0) >= 48 and (operand.byte_at(at + 1) ?? 0) <= 55 and (operand.byte_at(at + 2) ?? 0) >= 48 and (operand.byte_at(at + 2) ?? 0) <= 55 and (operand.byte_at(at + 3) ?? 0) >= 48 and (operand.byte_at(at + 3) ?? 0) <= 55 {
        let first = operand.byte_at(at + 1) ?? 48
        let middle = operand.byte_at(at + 2) ?? 48
        let last = operand.byte_at(at + 3) ?? 48
        let parsed = (first - 48) * 64 + (middle - 48) * 8 + last - 48
        if parsed > 255 { gnu.error(f"warning: the ambiguous octal escape \\{operand[at + 1..at + 4].utf8() ?? ""} is being interpreted as the 2-byte sequence \\0{operand[at + 1..at + 3].utf8() ?? ""}, {operand[at + 3..at + 4].utf8() ?? ""}") }
      }
      at += 1
    }
    match operand.utf8() {
      Ok(_) => {}
      Err(_) => gnu.error("warning: invalid utf8 sequence")
    }
  }
  let first = match parse_set(array_get(args.operands, 0), true, 0) {
    Ok(value) => value
    Err(message) => { gnu.error(message); exit 1 }
  }
  let second_base = if count > 1 {
    match parse_set(args.operands[1], false, 0) {
      Ok(value) => value
      Err(message) => { gnu.error(message); exit 1 }
    }
  } else { [] }
  let first_classes = match class_positions(array_get(args.operands, 0)) {
    Ok(value) => value
    Err(message) => { gnu.error(message); exit 1 }
  }
  let second_classes = if count > 1 {
    match class_positions(args.operands[1]) {
      Ok(value) => value
      Err(message) => { gnu.error(message); exit 1 }
    }
  } else { [] }
  for target_class in second_classes {
    var matched = false
    for source_class in first_classes { matched = matched or source_class.offset == target_class.offset }
    if ! matched { gnu.error("when translating, every 'upper'/'lower' in set2 must be matched by a 'upper'/'lower' in the same position in set1"); exit 1 }
    if target_class.name == "blank" and ! args.delete { gnu.error("blank character class is not allowed in string2 when translating"); exit 1 }
  }
  if second_classes.len() > 0 and first.len() > second_base.len() and ! args.truncate {
    let last_class = second_classes[second_classes.len() - 1]
    if last_class.name == "upper" or last_class.name == "lower" { gnu.error("string1 is longer than string2 and string2 ends in a character class"); exit 1 }
  }
  var input_set = first
  let truncate_now = args.truncate and ! args.delete and ! args.squeeze
  if truncate_now and args.complement and first_classes.len() == 0 {
    input_set = complement(input_set)
    input_set = input_set[..if input_set.len() > second_base.len() { second_base.len() } else { input_set.len() }]
  } else {
    if truncate_now and second_base.len() > 0 { input_set = input_set[..if input_set.len() > second_base.len() { second_base.len() } else { input_set.len() }] }
    if args.complement { input_set = complement(input_set) }
  }
  let second = if count > 1 {
    match parse_set(args.operands[1], false, input_set.len()) {
      Ok(value) => value
      Err(message) => { gnu.error(message); exit 1 }
    }
  } else { [] }
  if first_classes.len() > 0 and args.complement and ! args.delete and (unique_count(second) > 1 or second.len() > input_set.len() or (truncate_now and second.len() < input_set.len())) {
    gnu.error("when translating with complemented character classes,\nstring2 must map all characters in the domain to one"); exit 1
  }
  if count > 1 and second.len() == 0 and first.len() > 0 and ! args.truncate { gnu.error("when not truncating set1, string2 must be non-empty"); exit 1 }
  var output_set = second
  if ! args.delete and output_set.len() > 0 and input_set.len() > output_set.len() {
    while output_set.len() < input_set.len() { output_set += [output_set[output_set.len() - 1]] }
  }
  let squeeze_set = if args.delete and second.len() > 0 { second } else if args.delete or count == 1 { input_set } else { output_set }
  let input = match io.stdin_bytes() {
    Ok(data) => data
    Err(failure) => { gnu.error(f"read error: {gnu.strerror(failure)}"); exit 1 }
  }
  gnu.write_bytes(transform(input, input_set, output_set, args.delete, args.squeeze, squeeze_set))
}
