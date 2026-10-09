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

type TrAtom = {value: Bytes, dash: Bool}
type ClassHit = {name: Str, size: Int}
type TrInvocation = {delete: Bool, squeeze: Bool, complement: Bool, truncate: Bool, operands: List[Bytes], error: Str}

pure unit_size(data: Bytes, at: Int) -> Int {
  let lead = data.byte_at(at) ?? 0
  let size = if lead >= 194 and lead <= 223 { 2 } else if lead >= 224 and lead <= 239 { 3 } else if lead >= 240 and lead <= 244 { 4 } else { 1 }
  if size > 1 and at + size <= data.len() and (data[at..at + size].utf8() ?? "") != "" { size } else { 1 }
}

pure byte(value: Int) -> Bytes { bytes.from_ints([value % 256]) ?? b"" }

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

pure parse_atoms(data: Bytes) -> List[TrAtom] {
  var atoms: List[TrAtom] = []
  var at = 0
  while at < data.len() {
    let class = class_at(data, at)
    if class.size > 0 {
      for value in class_values(class.name) { atoms += [{value: value, dash: false}] }
      at += class.size
    } else if data.byte_at(at) == 92 {
      if at + 1 >= data.len() {
        atoms += [{value: b"\\", dash: false}]
        at += 1
      } else {
        let escaped = data.byte_at(at + 1) ?? 0
        let code = if escaped == 97 { 7 } else if escaped == 98 { 8 } else if escaped == 102 { 12 } else if escaped == 110 { 10 } else if escaped == 114 { 13 } else if escaped == 116 { 9 } else if escaped == 118 { 11 } else { -1 }
        if code >= 0 {
          atoms += [{value: byte(code), dash: false}]
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
          atoms += [{value: byte(value), dash: false}]
          at += 1 + digits
        } else {
          let size = unit_size(data, at + 1)
          atoms += [{value: data[at + 1..at + 1 + size], dash: false}]
          at += 1 + size
        }
      }
    } else {
      let size = unit_size(data, at)
      let value = data[at..at + size]
      atoms += [{value: value, dash: value == b"-"}]
      at += size
    }
  }
  atoms
}

proc parse_set(data: Bytes) [error] -> Result[List[Bytes], Str] {
  let atoms = parse_atoms(data)
  var out: List[Bytes] = []
  var at = 0
  while at < atoms.len() {
    if at + 2 < atoms.len() and atoms[at + 1].dash and atoms[at].value.len() == 1 and atoms[at + 2].value.len() == 1 {
      let first = atoms[at].value.byte_at(0) ?? 0
      let last = atoms[at + 2].value.byte_at(0) ?? 0
      if first > last { return Err(f"range-endpoints of '{atoms[at].value.utf8() ?? "?"}-{atoms[at + 2].value.utf8() ?? "?"}' are in reverse collating sequence order") }
      for n in range(first, last + 1) { out += [byte(n)] }
      at += 3
    } else {
      out += [atoms[at].value]
      at += 1
    }
  }
  Ok(out)
}

pure contains(values: List[Bytes], value: Bytes) -> Bool {
  for item in values { return true when item == value }
  false
}

pure index_of(values: List[Bytes], value: Bytes) -> Int {
  for at in range(values.len()) { if values[at] == value { return at } }
  -1
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
    if count < 1 or count > (if args.squeeze { 2 } else { 1 }) { gnu.usage_error("missing operand or extra operand") }
  } else if args.squeeze and count == 1 {
  } else if count != 2 { gnu.usage_error("missing operand or extra operand") }
  let first = match parse_set(array_get(args.operands, 0)) {
    Ok(value) => value
    Err(message) => { gnu.error(message); exit 1 }
  }
  let second = if count > 1 {
    match parse_set(args.operands[1]) {
      Ok(value) => value
      Err(message) => { gnu.error(message); exit 1 }
    }
  } else { [] }
  var input_set = first
  if args.truncate and second.len() > 0 { input_set = input_set[..if input_set.len() > second.len() { second.len() } else { input_set.len() }] }
  if args.complement { input_set = complement(input_set) }
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
