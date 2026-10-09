#!/bin/xsh
use lib.gnu

const USAGE = """Usage: mknod [OPTION]... NAME TYPE [MAJOR MINOR]
Create the special file NAME of the given TYPE.

  -m, --mode=MODE  set file permission bits to MODE, not a=rw - umask
      --help       display this help and exit
      --version    output version information and exit

TYPE may be b (block), c or u (character), or p (FIFO).
"""

type MknodOptions = {mode: Str, help: Bool, version: Bool, operands: List[Str]}

pure has_bit(value: Int, bit: Int) -> Bool { value / bit % 2 == 1 }

pure filter_bits(value: Int, mask: Int) -> Int {
  var result = value
  for bit in [4, 2, 1] {
    if has_bit(result, bit) and has_bit(mask, bit) { result -= bit }
  }
  result
}

pure edit_class(old: Int, requested: Int, op: Str) -> Int {
  if op == "=" { return requested }
  var result = old
  for bit in [4, 2, 1] {
    if has_bit(requested, bit) {
      if op == "+" and ! has_bit(result, bit) { result += bit }
      if op == "-" and has_bit(result, bit) { result -= bit }
    }
  }
  result
}

pure octal_mode(text: Str) -> Int? {
  return null when text == ""
  var value = 0
  var at = 0
  while at < text.byte_len() {
    let digit = (text.byte_slice(at, length: 1).byte_at(0) ?? 255) - 48
    return null when digit < 0 or digit > 7
    value = value * 8 + digit
    return null when value > 4095
    at += 1
  }
  value
}

pure parse_mode(text: Str, mask: Int) -> Int? {
  let numeric = octal_mode(text)
  return numeric when numeric != null
  for clause in text.split(",") {
    return null when ! rx"^[ugoa]*[+=-][rwxX]*$".matches(clause)
  }
  var mode = 0o666
  for clause in text.split(",") {
    var at = 0
    var u = false
    var g = false
    var o = false
    var all = false
    while at < clause.byte_len() {
      let c = clause.byte_slice(at, length: 1)
      if c == "u" { u = true } else if c == "g" { g = true } else if c == "o" { o = true } else if c == "a" { all = true } else { break }
      at += 1
    }
    let implicit = ! u and ! g and ! o and ! all
    if implicit or all { u = true; g = true; o = true }
    return null when at >= clause.byte_len()
    let op = clause.byte_slice(at, length: 1)
    return null when op != "+" and op != "-" and op != "="
    at += 1
    let perms = clause.byte_slice(at)
    var requested = 0
    if perms.find("r") != null { requested += 4 }
    if perms.find("w") != null { requested += 2 }
    if perms.find("x") != null { requested += 1 }
    let ur = mode / 0o100 % 8
    let gr = mode / 0o010 % 8
    let other_old = mode % 8
    let ur_new = if u { edit_class(ur, if implicit { filter_bits(requested, mask / 0o100 % 8) } else { requested }, op) } else { ur }
    let gr_new = if g { edit_class(gr, if implicit { filter_bits(requested, mask / 0o010 % 8) } else { requested }, op) } else { gr }
    let other_new = if o { edit_class(other_old, if implicit { filter_bits(requested, mask % 8) } else { requested }, op) } else { other_old }
    mode = ur_new * 0o100 + gr_new * 0o010 + other_new
  }
  mode
}

pure parse_uint(text: Str) -> Int? {
  return null when text == ""
  var value = 0
  var at = 0
  while at < text.byte_len() {
    let digit = (text.byte_slice(at, length: 1).byte_at(0) ?? 255) - 48
    return null when digit < 0 or digit > 9
    value = value * 10 + digit
    return null when value > 4294967295
    at += 1
  }
  value
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: MknodOptions = cli.applet(
    argv,
    {
      gnu: {status: 1, unsupported: {"-Z": "SELinux context setting is unavailable", "--context": "SELinux context setting is unavailable"}},
      mode: {form: "-m --mode MODE", default: ""},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      operands: {form: "...ARG"},
    },
  )?
  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("mknod"); return }
  if opts.operands.len() < 2 { gnu.missing_operand() }

  let name = opts.operands[0]
  let type_name = opts.operands[1]
  let kind = if type_name == "p" or type_name == "pipe" { "fifo" } else if type_name == "b" { "block" } else if type_name == "c" or type_name == "u" { "char" } else { gnu.usage_error(f"invalid node type {gnu.quote_value(type_name)}"); "" }
  let device = kind == "char" or kind == "block"
  if device and opts.operands.len() > 4 { gnu.extra_operand(opts.operands[4]) }
  if device and opts.operands.len() < 4 { gnu.error("Special files require major and minor device numbers."); exit 1 }
  if device and (parse_uint(opts.operands[2]) == null or parse_uint(opts.operands[3]) == null) {
    let bad = if parse_uint(opts.operands[2]) == null { opts.operands[2] } else { opts.operands[3] }
    gnu.error(f"invalid value {gnu.quote_value(bad)}")
    exit 1
  }
  if ! device and opts.operands.len() != 2 {
    gnu.error("Fifos do not have major and minor device numbers")
    exit 1
  }

  let mask = fs.umask()?
  let implicit = filter_bits(0o666 / 0o100 % 8, mask / 0o100 % 8) * 0o100
    + filter_bits(0o666 / 0o010 % 8, mask / 0o010 % 8) * 0o010
    + filter_bits(0o666 % 8, mask % 8)
  var mode = implicit
  let explicit = opts.mode != ""
  if explicit {
    let parsed = parse_mode(opts.mode, mask)
    if parsed == null { gnu.usage_error(f"invalid mode {gnu.quote_value(opts.mode)}") }
    mode = parsed ?? 0
    if mode / 512 > 0 { gnu.error("mode must specify only file permission bits"); exit 1 }
  }

  var major = 0
  var minor = 0
  if device {
    let parsed_major = parse_uint(opts.operands[2])
    let parsed_minor = parse_uint(opts.operands[3])
    if parsed_major == null or parsed_minor == null { gnu.usage_error("invalid major or minor device number") }
    major = parsed_major ?? 0
    minor = parsed_minor ?? 0
  }

  let target = fp"{name}"
  if let Err(failure) = fs.mknod(target, kind, if explicit { mode } else { 0o666 }, major, minor) {
    gnu.error(f"cannot create special file {gnu.quote(name)}: {gnu.strerror(failure)}")
    exit 1
  }
  if explicit {
    if let Err(failure) = target.chmod(mode) {
      gnu.error(f"cannot set permissions of {gnu.quote(name)}: {gnu.strerror(failure)}")
      exit 1
    }
  }
}
