#!/bin/xsh
use lib.gnu

const USAGE = """Usage: mkfifo [OPTION]... NAME...
Create named pipes (FIFOs) with the given NAMEs.

  -m, --mode=MODE  set file permission bits to MODE, not a=rw - umask
      --help       display this help and exit
      --version    output version information and exit
"""

type MkfifoOptions = {mode: Str, help: Bool, version: Bool, paths: List[Str]}

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

pure parse_mode(text: Str, initial: Int, mask: Int) -> Int? {
  let numeric = octal_mode(text)
  return numeric when numeric != null

  var mode = initial
  let clauses = text.split(",")
  return null when clauses.len() == 0
  for clause in clauses {
    return null when ! rx"^[ugoa]*[+=-][rwxX]*$".matches(clause)
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
    let um = if implicit { mask / 0o100 % 8 } else { 0 }
    let gm = if implicit { mask / 0o010 % 8 } else { 0 }
    let om = if implicit { mask % 8 } else { 0 }
    let ur_new = if u { edit_class(ur, filter_bits(requested, um), op) } else { ur }
    let gr_new = if g { edit_class(gr, filter_bits(requested, gm), op) } else { gr }
    let other_new = if o { edit_class(other_old, filter_bits(requested, om), op) } else { other_old }
    mode = ur_new * 0o100 + gr_new * 0o010 + other_new
  }
  mode
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: MkfifoOptions = cli.applet(
    argv,
    {
      gnu: {status: 1, unsupported: {"-Z": "SELinux context setting is unavailable", "--context": "SELinux context setting is unavailable"}},
      mode: {form: "-m --mode MODE", default: ""},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      paths: {form: "...NAME"},
    },
  )?
  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("mkfifo"); return }
  if opts.paths.len() == 0 { gnu.missing_operand() }

  let mask = fs.umask()?
  var mode = filter_bits(0o666 / 0o100 % 8, mask / 0o100 % 8) * 0o100
    + filter_bits(0o666 / 0o010 % 8, mask / 0o010 % 8) * 0o010
    + filter_bits(0o666 % 8, mask % 8)
  let explicit = opts.mode != ""
  if explicit {
    let parsed = parse_mode(opts.mode, 0o666, mask)
    if parsed == null { gnu.usage_error(f"invalid mode {gnu.quote_value(opts.mode)}") }
    mode = parsed ?? 0
    if mode / 512 > 0 { gnu.error("mode must specify only file permission bits"); exit 1 }
  }

  var failed = false
  for name in opts.paths {
    let target = fp"{name}"
    if let Err(failure) = fs.mkfifo(target, if explicit { mode } else { 0o666 }) {
      gnu.error(f"cannot create fifo {gnu.quote(name)}: {gnu.strerror(failure)}")
      failed = true
      continue
    }
    if explicit {
      if let Err(failure) = target.chmod(mode) {
        gnu.error(f"cannot set permissions of {gnu.quote(name)}: {gnu.strerror(failure)}")
        failed = true
      }
    }
  }
  if failed { exit 1 }
}
