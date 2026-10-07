#!/bin/xsh
use lib.gnu
use lib.fs_misc

type Options = {size: Str?, reference: Str?, no_create: Bool, blocks: Bool, help: Bool, version: Bool, paths: List[Str]}
type RawArgument = {marker: Str, value: Bytes}
type PreparedArguments = {text: List[Str], raw: List[RawArgument]}
type SizeSource = {index: Int, start: Int}

# The shared option parser takes text; NUL-marked operands preserve Unix argv bytes.
pure prepare_arguments(argv: List[Bytes]) -> PreparedArguments {
  var text: List[Str] = []
  var raw: List[RawArgument] = []
  for index in range(argv.len()) {
    let argument = argv[index]
    match argument.utf8() {
      Ok(value) => text += [value]
      Err(_) => {
        let marker = f"\0truncate-raw-argument-{index}\0"
        text += [marker]
        raw += [{marker: marker, value: argument}]
      }
    }
  }
  {text: text, raw: raw}
}

pure argument_bytes(value: Str, raw: List[RawArgument]) -> Bytes {
  for argument in raw {
    if argument.marker == value { return argument.value }
  }
  bytes.from_text(value)
}

proc cannot_stat(name: Bytes, failure: Error) [process, env] {
  gnu.error(f"cannot stat {gnu.quote_bytes(name)}: {gnu.strerror(failure)}")
}

proc cannot_open(name: Bytes, failure: Error) [process, env] {
  gnu.error(f"cannot open {gnu.quote_bytes(name)} for writing: {gnu.strerror(failure)}")
}

pure size_source(argv: List[Bytes]) -> SizeSource {
  for index in range(argv.len()) {
    let arg = argv[index].utf8() ?? ""
    if arg in ["-s", "--size"] { return {index: index + 1, start: 0} }
    if arg.starts_with("--size=") { return {index: index, start: 7} }
    if arg.starts_with("-s") and arg.byte_len() > 2 { return {index: index, start: 2} }
  }
  {index: 0, start: 0}
}

pure numeric_prefix_length(value: Str) -> Int {
  var index = 0
  while index < value.byte_len() {
    let char = value.byte_slice(index, length: 1)
    break when char not in "0123456789"
    index += 1
  }
  index
}

proc size_error(argv: List[Bytes], raw: Str, value: Str, message: Str) [env, process, error] {
  gnu.error(message)
  let requested = env.get_or("UUTILS_DIAG", "") ?? ""
  return when requested == "never"
  if requested != "always" and ! unix.isatty(2) { return }
  let source = size_source(argv)
  let program = gnu.prog()
  var command = program
  for arg in argv { command += " " + (arg.utf8() ?? gnu.quote_bytes(arg)) }
  var column = program.byte_len() + 1
  for index in range(source.index) { column += argv[index].len() + 1 }
  let trimmed = raw.trim()
  let operation_length = if rx"^[+<>/%-]".matches(trimmed) { 1 } else { 0 }
  let number_length = numeric_prefix_length(value)
  let span_start = source.start + operation_length + (if number_length > 0 { number_length } else { 0 })
  let span_length = if number_length > 0 { value.byte_len() - number_length } else { raw.byte_len() }
  column += span_start
  let spacing = [" " for _ in range(column)].join("")
  let marker = if number_length > 0 { "─┬" } else { ["─" for _ in range(span_length)].join("") }
  eprint f"   ╭─[ {program}:1:{column + 1} ]"
  eprint "   │"
  eprint f" 1 │ {command}"
  eprint f"   │ {spacing}{marker}"
  if number_length > 0 {
    eprint f"   │ {spacing} ╰─ not a known unit"
    eprint "   │ Help: a size is a number and an optional unit: K, M, G and so on for 1024, KB, MB, GB for 1000"
  }
  eprint "───╯"
}

proc main(...argv: List[Bytes]) [fs, process, env, error, io] {
  let prepared = prepare_arguments(argv)
  let opts: Options = cli.applet(prepared.text, {
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
  if opts.size == null and opts.reference == null { gnu.usage_error("error: the following required arguments were not provided: --size or --reference") }
  if opts.blocks and opts.size == null { gnu.usage_error("'--io-blocks' was specified but '--size' was not") }
  if opts.paths.is_empty() { gnu.missing_operand() }
  let raw = opts.size ?? ""
  var operation = ""
  var quantity = 0
  if opts.size != null {
    var value = raw.trim()
    if rx"^[+<>/%-]".matches(value) {
      operation = value.byte_slice(0, length: 1)
      value = value.byte_slice(1)
    }
    let parsed: Int? = if value.ends_with("b") { null } else { fs_misc.size_value(value) }
    if parsed == null {
      let detail = if fs_misc.size_overflow(value) { ": Value too large for defined data type" } else { "" }
      size_error(argv, raw, value, f"Invalid number: {gnu.quote_value(raw)}{detail}")
      exit 1
    }
    quantity = parsed ?? 0
    if quantity == 0 and operation in ["/", "%"] { gnu.error("division by zero"); exit 1 }
  }
  var reference = 0
  if opts.reference != null {
    let name = argument_bytes(opts.reference ?? "", prepared.raw)
    let info = fs.stat(Path.parse_bytes(name)?, follow_symlinks: true)
    if let Err(failure) = info { cannot_stat(name, failure); exit 1 }
    reference = info?.size
  }
  if opts.reference != null and opts.size != null and operation == "" { gnu.error("you must specify a relative size with --reference"); exit 1 }
  var failed = false
  for operand in opts.paths {
    let name = argument_bytes(operand, prepared.raw)
    let target = Path.parse_bytes(name)?
    let metadata = fs.stat(target, follow_symlinks: true)
    if let Err(failure) = metadata {
      if opts.no_create and failure.errno == 2 { continue }
    }
    var current = if opts.reference != null { reference } else { if let Ok(info) = metadata { info.size } else { 0 } }
    var amount = quantity
    if opts.blocks {
      let info = if metadata is Ok(_) { metadata } else { fs.stat(target.parent(), follow_symlinks: true) }
      if let Err(failure) = info { cannot_stat(name, failure); failed = true; continue }
      let block = info?.blksize
      if amount > 9223372036854775807 / block { gnu.error(f"Invalid number: {gnu.quote_value(raw)}"); failed = true; continue }
      amount *= block
    }
    if opts.size != null {
      if operation == "+" {
        if current > 9223372036854775807 - amount { gnu.error(f"overflow in {gnu.quote_bytes(name)}"); failed = true; continue }
        current += amount
      } else if operation == "-" { current = if amount > current { 0 } else { current - amount } } else if operation == "<" { current = if current < amount { current } else { amount } } else if operation == ">" { current = if current > amount { current } else { amount } } else if operation == "/" { current -= current % amount } else if operation == "%" {
        let remainder = current % amount
        if remainder != 0 {
          if current > 9223372036854775807 - (amount - remainder) { gnu.error(f"overflow in {gnu.quote_bytes(name)}"); failed = true; continue }
          current += amount - remainder
        }
      } else { current = amount }
    }
    if metadata is Err(_) {
      if let Err(failure) = target.touch() { cannot_open(name, failure); failed = true; continue }
    }
    if let Err(failure) = target.truncate(current) {
      if (failure.errno ?? 0) in [2, 6, 13, 20, 40] { cannot_open(name, failure) } else { gnu.error(f"failed to truncate {gnu.quote_bytes(name)} at {current} bytes: {gnu.strerror(failure)}") }
      failed = true
    }
  }
  if failed { exit 1 }
}
