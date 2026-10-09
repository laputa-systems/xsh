#!/bin/xsh
use lib.gnu

const USAGE = """Usage: truncate OPTION... FILE...
Shrink or extend the size of each FILE to the specified size.

  -c, --no-create     do not create files that do not exist
  -o, --io-blocks     treat SIZE as number of filesystem I/O blocks
  -r, --reference=RFILE  base size on RFILE
  -s, --size=SIZE     set or adjust the file size by SIZE bytes
      --help          display this help and exit
      --version       output version information and exit
"""

type TruncateOptions = {no_create: Bool, io_blocks: Bool, reference: Str, size: Str, help: Bool, version: Bool, files: List[Str]}
type SizeMode = {op: Str, amount: Int}

pure parse_digits(text: Str) -> Int? {
  return null when text == ""
  var value = 0
  var at = 0
  while at < text.byte_len() {
    let digit = (text.byte_slice(at, length: 1).byte_at(0) ?? 255) - 48
    return null when digit < 0 or digit > 9
    return null when value > 922337203685477580 or (value == 922337203685477580 and digit > 7)
    value = value * 10 + digit
    at += 1
  }
  value
}

pure suffix_factor(text: Str) -> Int? {
  return 1 when text == ""

  let upper = text.upper()
  let base = if upper.ends_with("B") and upper.byte_len() == 2 { 1000 } else { 1024 }
  let unit = upper.byte_slice(0, length: 1)
  let power = if unit == "K" { 1 } else if unit == "M" { 2 } else if unit == "G" { 3 } else if unit == "T" { 4 } else if unit == "P" { 5 } else if unit == "E" { 6 } else if unit == "Z" { 7 } else if unit == "Y" { 8 } else if unit == "R" { 9 } else if unit == "Q" { 10 } else { return null }
  return null when upper.byte_len() > 3 or (upper.byte_len() == 3 and upper.byte_slice(1) != "IB" and upper.byte_slice(1) != "BD")
  var factor = 1
  for _ in range(power) {
    return null when factor > 9223372036854775807 / base
    factor *= base
  }
  factor
}

pure parse_size(text: Str) -> SizeMode? {
  let value = text.trim()
  return null when value == ""
  let first = if value == "" { "" } else { value.byte_slice(0, length: 1) }
  let op = if "+-<>/%".find(first) != null { first } else { "=" }
  let body = if op == "=" { value } else if value == "" { "" } else { value.byte_slice(1) }
  return null when body == ""

  var digits_end = 0
  while digits_end < body.byte_len() {
    let byte = body.byte_slice(digits_end, length: 1).byte_at(0) ?? 0
    break when byte < 48 or byte > 57
    digits_end += 1
  }
  let number: Int? = if digits_end == 0 and body != "" { 0 } else { parse_digits(body.byte_slice(0, length: digits_end)) }
  let factor = suffix_factor(body.byte_slice(digits_end))
  return null when number == null or factor == null
  let n = number ?? 0
  let f = factor ?? 1
  return null when n > 9223372036854775807 / f
  {op: op, amount: n * f}
}

pure size_overflows(text: Str) -> Bool {
  let value = text.trim()
  let first = if value == "" { "" } else { value.byte_slice(0, length: 1) }
  let op = if "+-<>/%".find(first) != null { first } else { "=" }
  let body = if op == "=" { value } else if value == "" { "" } else { value.byte_slice(1) }
  var digits_end = 0
  while digits_end < body.byte_len() {
    let byte = body.byte_slice(digits_end, length: 1).byte_at(0) ?? 0
    break when byte < 48 or byte > 57
    digits_end += 1
  }
  return false when digits_end == 0
  let digits = body.byte_slice(0, length: digits_end)
  return true when digits.byte_len() > 19 or (digits.byte_len() == 19 and digits > "9223372036854775807")
  let suffix = body.byte_slice(digits_end).upper()
  let unit = suffix.byte_slice(0, length: 1)
  if suffix_factor(suffix) == null {
    return unit in ["K", "M", "G", "T", "P", "E", "Z", "Y", "R", "Q"] and (suffix == unit or suffix == f"{unit}B" or suffix == f"{unit}IB")
  }
  let number = parse_digits(digits) ?? 0
  let factor = suffix_factor(suffix) ?? 1
  number > 9223372036854775807 / factor
}

pure computed_size(mode: SizeMode, current: Int) -> Int? {
  let n = mode.amount
  if mode.op == "=" { return n }
  if mode.op == "+" { return null when current > 9223372036854775807 - n; return current + n }
  if mode.op == "-" { return if n > current { 0 } else { current - n } }
  if mode.op == "<" { return if current < n { current } else { n } }
  if mode.op == ">" { return if current > n { current } else { n } }
  if n == 0 { return null }
  if mode.op == "/" { return current - current % n }
  return null when current > 9223372036854775807 - (n - 1)
  (current + n - 1) / n * n
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: TruncateOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      no_create: {form: "-c --no-create", default: false},
      io_blocks: {form: "-o --io-blocks", default: false},
      reference: {form: "-r --reference RFILE", default: ""},
      size: {form: "-s --size SIZE", default: ""},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      files: {form: "...FILE"},
    },
  )?
  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("truncate"); return }
  if opts.files.len() == 0 { gnu.missing_operand() }
  var size_given = opts.size != ""
  for arg in argv {
    if arg == "-s" or arg == "--size" or arg.starts_with("--size=") or (arg.starts_with("-s") and arg != "-s") { size_given = true }
  }
  if ! size_given and opts.reference == "" { gnu.usage_error("you must specify either --size or --reference") }
  if opts.io_blocks and ! size_given { gnu.usage_error("option --io-blocks requires --size") }

  var reference_size: Int? = null
  if opts.reference != "" {
    if let Err(failure) = fs.stat(fp"{opts.reference}", true) {
      gnu.error(f"cannot stat {gnu.quote(opts.reference)}: {gnu.strerror(failure)}")
      exit 1
    } else if let Ok(meta) = fs.stat(fp"{opts.reference}", true) {
      reference_size = meta.size
    }
  }

  let mode = if ! size_given { {op: "+", amount: 0} } else { parse_size(opts.size) ?? {op: "?", amount: 0} }
  if mode.op == "?" {
    let message = if size_overflows(opts.size) { f"Invalid number: {gnu.quote_value(opts.size)}: Value too large for defined data type" } else if opts.size == "" { "Invalid number: ''" } else { f"Invalid number: {gnu.quote_value(opts.size)}" }
    gnu.error(message)
    exit 1
  }
  if reference_size != null and mode.op == "=" { gnu.usage_error("size must be relative when --reference is used") }

  var failed = false
  for name in opts.files {
    let target = fp"{name}"
    var exists = false
    var current = 0
    var io_block_size = 4096
    if let Ok(meta) = fs.stat(target, true) {
      exists = true
      current = meta.size
      io_block_size = meta.blksize
      if meta.kind == "fifo" {
        gnu.error(f"cannot open {gnu.quote(name)} for writing: No such device or address")
        failed = true
        continue
      }
    }
    if ! exists and opts.no_create { continue }
    if ! exists {
      if let Err(failure) = target.write(b"") {
        gnu.error(f"cannot open {gnu.quote(name)} for writing: {gnu.strerror(failure)}")
        failed = true
        continue
      }
    }

    let baseline = reference_size ?? current
    var active = mode
    if opts.io_blocks {
      if mode.amount > 9223372036854775807 / io_block_size {
        gnu.error(f"invalid number {gnu.quote_value(opts.size)}")
        failed = true
        continue
      }
      active = {op: mode.op, amount: mode.amount * io_block_size}
    }
    let desired = computed_size(active, baseline)
    if desired == null {
      if (active.op == "/" or active.op == "%") and active.amount == 0 {
        gnu.error(f"division by zero in {gnu.quote_value(opts.size)}")
      } else {
        gnu.error(f"Invalid number: {gnu.quote_value(opts.size)}")
      }
      failed = true
      continue
    }
    if let Err(failure) = target.truncate(desired ?? 0) {
      gnu.error(f"cannot open {gnu.quote(name)} for writing: {gnu.strerror(failure)}")
      failed = true
    }
  }
  if failed { exit 1 }
}
