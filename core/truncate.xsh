#!/bin/xsh
use lib.gnu
use lib.fs_misc

type Options = {size: Str?, reference: Str?, no_create: Bool, blocks: Bool, help: Bool, version: Bool, paths: List[Str]}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1},
    size: {form: "-s --size SIZE"},
    reference: {form: "-r --reference FILE"},
    no_create: {form: "-c --no-create", default: false},
    blocks: {form: "-o --io-blocks", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    paths: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: truncate OPTION... FILE...\nShrink or extend each FILE to the specified size.\n  -s, --size=SIZE\n  -r, --reference=FILE\n  -c, --no-create\n  -o, --io-blocks\n"); return }
  if opts.version { gnu.version("truncate"); return }
  if opts.size == null and opts.reference == null { gnu.usage_error("you must specify either '--size' or '--reference'") }
  if opts.blocks and opts.size == null { gnu.usage_error("'--io-blocks' was specified but '--size' was not") }
  if opts.paths.is_empty() { gnu.missing_operand() }
  let raw = opts.size ?? ""
  var operation = ""
  var quantity = 0
  if opts.size != null {
    var value = raw.trim()
    if value != "" and value.byte_slice(0, length: 1) in ["+", "-", "<", ">", "/", "%"] {
      operation = value.byte_slice(0, length: 1)
      value = value.byte_slice(1)
    }
    let parsed: Int? = if value.ends_with("b") { null } else { fs_misc.size_value(value) }
    if parsed == null {
      let detail = if fs_misc.size_overflow(value) { ": Value too large for defined data type" } else { "" }
      gnu.error(f"Invalid number: {gnu.quote_value(raw)}{detail}")
      exit 1
    }
    quantity = parsed ?? 0
    if quantity == 0 and operation in ["/", "%"] { gnu.error("division by zero"); exit 1 }
  }
  var reference = 0
  if opts.reference != null {
    let name = opts.reference ?? ""
    let info = fs.stat(fp"{name}", follow_symlinks: true)
    if let Err(failure) = info { gnu.cannot("stat", name, failure); exit 1 }
    reference = info?.size
  }
  if opts.reference != null and opts.size != null and operation == "" { gnu.error("you must specify a relative size with --reference"); exit 1 }
  var failed = false
  for name in opts.paths {
    let target = fp"{name}"
    let metadata = fs.stat(target, follow_symlinks: true)
    if let Err(failure) = metadata {
      if opts.no_create and failure.errno == 2 { continue }
    }
    var current = if opts.reference != null { reference } else { if let Ok(info) = metadata { info.size } else { 0 } }
    var amount = quantity
    if opts.blocks {
      let info = if metadata is Ok(_) { metadata } else { fs.stat(target.parent(), follow_symlinks: true) }
      if let Err(failure) = info { gnu.cannot("stat", name, failure); failed = true; continue }
      let block = info?.blksize
      if amount > 9223372036854775807 / block { gnu.error(f"Invalid number: {gnu.quote_value(raw)}"); failed = true; continue }
      amount *= block
    }
    if opts.size != null {
      if operation == "+" {
        if current > 9223372036854775807 - amount { gnu.error(f"overflow in {gnu.quote(name)}"); failed = true; continue }
        current += amount
      } else if operation == "-" { current = if amount > current { 0 } else { current - amount } } else if operation == "<" { current = if current < amount { current } else { amount } } else if operation == ">" { current = if current > amount { current } else { amount } } else if operation == "/" { current -= current % amount } else if operation == "%" {
        let remainder = current % amount
        if remainder != 0 {
          if current > 9223372036854775807 - (amount - remainder) { gnu.error(f"overflow in {gnu.quote(name)}"); failed = true; continue }
          current += amount - remainder
        }
      } else { current = amount }
    }
    if metadata is Err(_) {
      if let Err(failure) = target.touch() { gnu.cannot_open(name, failure, mode: "writing"); failed = true; continue }
    }
    if let Err(failure) = target.truncate(current) { gnu.error(f"failed to truncate {gnu.quote(name)} at {current} bytes: {gnu.strerror(failure)}"); failed = true }
  }
  if failed { exit 1 }
}
