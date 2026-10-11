#!/bin/xsh
use lib.gnu
use lib.textio_a1 as tio

type Format = {kind: Str, size: Int, chars: Int, ascii: Bool, float_format: Str}
const DIGITS = "0123456789abcdef"
# Keep wide record padding bounded while still exposing stdout write failures.
const OUTPUT_CHUNK = 1048576
const NAMES = ["nul", "soh", "stx", "etx", "eot", "enq", "ack", "bel", "bs", "ht", "nl", "vt", "ff", "cr", "so", "si", "dle", "dc1", "dc2", "dc3", "dc4", "nak", "syn", "etb", "can", "em", "sub", "esc", "fs", "gs", "rs", "us"]

pure spaces(count: Int) -> Str {
  var remaining = count
  var block = " "
  var out = ""
  while remaining > 0 {
    if remaining % 2 == 1 { out += block }
    remaining /= 2
    if remaining > 0 { block += block }
  }
  out
}

proc write_spaces(count: Int) [process, env, io] {
  let block = bytes.from_text(spaces(OUTPUT_CHUNK))
  var remaining = count

  while remaining >= OUTPUT_CHUNK {
    gnu.write_bytes(block)
    remaining -= OUTPUT_CHUNK
  }

  if remaining > 0 { gnu.write_text(spaces(remaining)) }
}

pure number(value: Int, base: Int, width: Int) -> Str {
  var n = value
  var out = ""
  loop {
    out = DIGITS.byte_slice(n % base, length: 1) + out
    n /= base
    break when n == 0
  }
  while out.byte_len() < width { out = "0" + out }
  out
}

pure formats(text: Str) -> Result[List[Format]] {
  var out: List[Format] = []
  var i = 0
  while i < text.byte_len() {
    let kind = text.byte_slice(i, length: 1)
    i += 1
    if ! (kind in ["a", "c", "d", "o", "u", "x", "f"]) { fail f"invalid type string {text}" }
    var size = if kind in ["a", "c"] { 1 } else if kind == "f" { 8 } else { 4 }
    var float_format = if kind == "f" { "binary64" } else { "" }
    if i < text.byte_len() and kind != "a" and kind != "c" {
      let next = text.byte_slice(i, length: 1)
      if kind == "f" and next in ["2", "4", "8", "H", "B", "F", "D"] {
        size = if next in ["2", "H", "B"] { 2 } else if next in ["4", "F"] { 4 } else { 8 }
        float_format = if next in ["2", "H"] { "binary16" } else if next == "B" { "bfloat16" } else if next in ["4", "F"] { "binary32" } else { "binary64" }
        i += 1
      } else if kind != "f" and next in ["1", "2", "4", "8", "C", "S", "I", "L"] {
        size = if next in ["1", "C"] { 1 } else if next in ["2", "S"] { 2 } else if next in ["4", "I"] { 4 } else { 8 }
        i += 1
      }
    }
    var ascii = false
    if i < text.byte_len() and text.byte_slice(i, length: 1) == "z" { ascii = true; i += 1 }
    let chars = if kind in ["a", "c"] { 3 } else if kind == "f" { if size == 8 { 24 } else { 15 } } else if kind == "x" { size * 2 } else if kind == "o" { (size * 8 + 2) / 3 } else if kind == "d" { if size == 1 { 4 } else if size == 2 { 6 } else if size == 4 { 11 } else { 20 } } else { if size == 1 { 3 } else if size == 2 { 5 } else if size == 4 { 10 } else { 20 } }
    out += [{kind: kind, size: size, chars: chars, ascii: ascii, float_format: float_format}]
  }
  Ok(out)
}

# Long division over byte limbs formats unsigned 64-bit values without
# overflowing XSH's signed Int, including all-ones machine words.
pure unsigned(data: Bytes, big: Bool, base: Int, width: Int) -> Str {
  var limbs = [data.byte_at(if big { i } else { data.len() - i - 1 }) ?? 0 for i in range(data.len())]
  var out = ""
  loop {
    var carry = 0
    var nonzero = false
    for i in range(limbs.len()) {
      let n = carry * 256 + limbs[i]
      limbs[i] = n / base
      carry = n % base
      nonzero = nonzero or limbs[i] != 0
    }
    out = DIGITS.byte_slice(carry, length: 1) + out
    break when ! nonzero
  }
  while out.byte_len() < width { out = "0" + out }
  out
}

pure normalized_float(text: Str) -> Str {
  text.replace("e-0", with: "e-").replace("e+0", with: "e+")
}

# The decoder widens narrow binary formats to Float, so accept only decimals
# inside the source value's rounding interval when selecting a shorter display.
pure float_rounding_limit(value: Float, format: Str) -> Float {
  let magnitude = value.abs()
  let exponent_min = if format == "binary16" { -14 } else { -126 }
  let fraction_bits = if format == "binary16" { 10 } else if format == "bfloat16" { 7 } else { 23 }
  if magnitude < 2.0.pow(exponent_min.float()) { return 2.0.pow((exponent_min - fraction_bits - 1).float()) }
  let exponent = magnitude.log(2.0).floor() ?? exponent_min
  let power = 2.0.pow(exponent.float())
  let lower_exponent = if magnitude == power and exponent > exponent_min { exponent - 1 } else { exponent }
  2.0.pow((lower_exponent - fraction_bits - 1).float())
}

pure float_text(value: Float, format: Str) -> Result[Str] {
  let display = f"{value}"
  return Ok("inf") when display == "Infinity"
  return Ok("-inf") when display == "-Infinity"
  if format in ["binary16", "bfloat16"] {
    return value.format_number("g", 8)
  }
  let narrow = format == "binary32"
  let maximum = if narrow { 9 } else { 17 }
  var best = display
  var precision = 1
  while precision <= maximum {
    let text = normalized_float(value.format_number("g", precision)?)
    let rounded = text.parse_float()?
    if narrow {
      if (rounded - value).abs() <= float_rounding_limit(value, format) and text.byte_len() < best.byte_len() { best = text }
    } else if rounded == value and text.byte_len() < best.byte_len() { best = text }
    precision += 1
  }
  Ok(best)
}

# Extra width before field `index` of a block: the block's padding is shared out across its
# fields with integer division, so the field widths sum to the block width.
pure field_pad_at(fields: Int, index: Int, pad: Int) -> Int {
  pad / fields * index + pad % fields * index / fields
}

pure item(data: Bytes, fmt: Format, big: Bool) -> Result[Str] {
  let byte = data.byte_at(0) ?? 0
  if fmt.kind == "f" {
    let value = bytes.unpack_float(data, 0, fmt.float_format, if big { "big" } else { "little" })?
    # Float display discards a NaN's sign; recover it from the source encoding.
    if f"{value}" == "NaN" {
      let lead = data.byte_at(if big { 0 } else { data.len() - 1 }) ?? 0
      return Ok(if lead >= 128 { "-nan" } else { "nan" })
    }
    return float_text(value, fmt.float_format)
  }
  if fmt.kind == "a" {
    let value = byte % 128
    return Ok(NAMES[value]) when value < 32
    return Ok("sp") when value == 32
    return Ok("del") when value == 127
    return bytes.from_ints([value])?.utf8()
  }
  if fmt.kind == "c" {
    return Ok("\\0") when byte == 0
    return Ok("\\a") when byte == 7
    return Ok("\\b") when byte == 8
    return Ok("\\t") when byte == 9
    return Ok("\\n") when byte == 10
    return Ok("\\v") when byte == 11
    return Ok("\\f") when byte == 12
    return Ok("\\r") when byte == 13
    return data[0..1].utf8() when byte >= 32 and byte < 127
    return Ok(number(byte, 8, 3))
  }
  if fmt.kind == "d" {
    let lead = data.byte_at(if big { 0 } else { data.len() - 1 }) ?? 0
    if lead >= 128 {
      var magnitude: List[Int] = []
      var carry = 1
      for i in range(data.len()) {
        let at = if big { data.len() - i - 1 } else { i }
        let n = 255 - (data.byte_at(at) ?? 0) + carry
        magnitude += [n % 256]
        carry = n / 256
      }
      return Ok("-" + unsigned(bytes.from_ints(magnitude)?, false, 10, 0))
    }
  }
  let base = if fmt.kind == "x" { 16 } else if fmt.kind == "o" { 8 } else { 10 }
  Ok(unsigned(data, big, base, if base == 10 { 0 } else { fmt.chars }))
}

pure address(offset: Int, base: Str, label: Int?, origin: Int) -> Str {
  let text = if base == "n" { "" } else { number(offset, if base == "x" { 16 } else if base == "d" { 10 } else { 8 }, if base == "x" { 6 } else { 7 }) }
  if let value = label {
    # With addresses suppressed, GNU prints the input offset in parentheses.
    let pseudo = if base == "n" { offset } else { value + offset - origin }
    return text + (if text == "" { "" } else { " " }) + "(" + number(pseudo, if base == "x" { 16 } else if base == "d" { 10 } else { 8 }, if base == "x" { 6 } else { 7 }) + ")"
  }
  text
}

pure old_offset(text: Str) -> Int? {
  var raw = if text.starts_with("+") { text.byte_slice(1) } else { text }
  let blocks = ! (raw.starts_with("0x") or raw.starts_with("0X")) and (raw.ends_with("b") or raw.ends_with("B"))
  if blocks { raw = raw.byte_slice(0, raw.byte_len() - 1) }
  let decimal = raw.ends_with(".")
  if decimal { raw = raw.byte_slice(0, raw.byte_len() - 1) }
  let hex = raw.starts_with("0x") or raw.starts_with("0X")
  if hex { raw = raw.byte_slice(2) }
  return null when raw == ""
  let base = if hex { 16 } else if decimal { 10 } else { 8 }
  var value = 0
  for i in range(raw.byte_len()) {
    let digit = DIGITS.find(raw.byte_slice(i, length: 1).lower()) ?? 16
    return null when digit >= base or value > (9223372036854775807 - digit) / base
    value = value * base + digit
  }
  if blocks {
    return null when value > 9223372036854775807 / 512
    value *= 512
  }
  value
}

pure byte_count(text: Str) -> Int? {
  return old_offset(text) when text.starts_with("0x") or text.starts_with("0X")
  tio.parse_count(text)
}

pure legacy_offset_overflow(text: Str) -> Bool {
  var raw = if text.starts_with("+") { text.byte_slice(1) } else { text }
  let blocks = ! (raw.starts_with("0x") or raw.starts_with("0X")) and (raw.ends_with("b") or raw.ends_with("B"))
  if blocks { raw = raw.byte_slice(0, raw.byte_len() - 1) }
  let decimal = raw.ends_with(".")
  if decimal { raw = raw.byte_slice(0, raw.byte_len() - 1) }
  let hex = raw.starts_with("0x") or raw.starts_with("0X")
  if hex { raw = raw.byte_slice(2) }
  return false when raw == ""
  let base = if hex { 16 } else if decimal { 10 } else { 8 }
  var value = 0
  for position in range(raw.byte_len()) {
    let digit = DIGITS.find(raw.byte_slice(position, length: 1).lower()) ?? 16
    return false when digit >= base
    return true when value > (9223372036854775807 - digit) / base
    value = value * base + digit
  }
  if blocks { return value > 9223372036854775807 / 512 }
  false
}

pure byte_count_overflow(text: Str) -> Bool {
  let parts = rx"^([0-9]*)(.*)$".captures(text)
  if parts[1] == "" { return false }
  let number = parts[1].parse_int() ?? -1
  return true when number < 0
  parts[2] != "" and byte_count(text) == 9223372036854775807
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  # Each traditional format option appends at its position, preserving GNU's
  # output order even when short options and explicit type strings mix.
  var args: List[Str] = []
  var requested: List[Str] = []
  var options = true
  var i = 0
  while i < argv.len() {
    let arg = argv[i]
    if arg == "--" { options = false }
    if options and arg == "-t" {
      if i + 1 >= argv.len() { gnu.usage_error("option requires an argument -- 't'") }
      i += 1
      requested += [argv[i]]
    } else if options and (arg.starts_with("-t") and arg.byte_len() > 2) {
      requested += [arg.byte_slice(2)]
    } else if options and arg.starts_with("--format=") {
      requested += [arg.byte_slice(9)]
    } else if options and arg == "--format" {
      if i + 1 >= argv.len() { gnu.usage_error("option '--format' requires an argument") }
      i += 1
      requested += [argv[i]]
    } else if options and arg.starts_with("-") and ! arg.starts_with("--") and arg != "-" {
      var position = 1
      while position < arg.byte_len() {
        let char = arg.byte_slice(position, length: 1)
        if "abcdDfFhHiIlLOosxX".find(char) != null {
          requested += [if char == "a" { "a" } else if char == "c" { "c" } else if char == "b" { "o1" } else if char in ["d", "s"] { if char == "d" { "u2" } else { "d2" } } else if char in ["h", "x"] { "x2" } else if char in ["H", "X"] { "x4" } else if char == "o" { "o2" } else if char == "O" { "o4" } else if char in ["I", "L"] { "d8" } else if char in ["i", "l", "D"] { if char == "D" { "u4" } else { "d4" } } else if char == "f" { "f4" } else { "f8" }]
          position += 1
        } else if char == "t" {
          if position + 1 < arg.byte_len() { requested += [arg.byte_slice(position + 1)] } else {
            if i + 1 >= argv.len() { gnu.usage_error("option requires an argument -- 't'") }
            i += 1
            requested += [argv[i]]
          }
          break
        } else if char == "v" { args += ["-v"]; position += 1 } else {
          if char == "w" and position + 1 == arg.byte_len() and i + 1 < argv.len() and rx"^-[0-9]".matches(argv[i + 1]) {
            i += 1
            args += ["-w" + argv[i]]
            break
          }
          args += ["-" + arg.byte_slice(position)]
          if char in ["A", "j", "N", "w"] and position + 1 == arg.byte_len() and i + 1 < argv.len() { i += 1; args += [argv[i]] }
          break
        }
      }
    } else { args += [arg] }
    i += 1
  }
  var opts = cli.applet(args, {
    gnu: {status: 1},
    address: {form: "-A --address-radix=RADIX", default: "o"},
    skip: {form: "-j --skip-bytes=BYTES", default: "0"},
    count: {form: "-N --read-bytes=BYTES", default: "9223372036854775807"},
    width: {form: "-w --width[=BYTES]", default: "16", optional_default: "32"},
    endian: {form: "--endian=ORDER", default: "little"},
    duplicates: {form: "-v --output-duplicates", default: false},
    strings: {form: "-S --strings[=BYTES]", default: "", optional_default: "3"},
    traditional: {form: "--traditional", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    files: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: od [OPTION]... [FILE]...\nWrite an unambiguous representation of FILE bytes.\n  -A, --address-radix=RADIX  d, o, x or n\n  -t, --format=TYPE         a, c, d, o, u or x with size\n  -j, --skip-bytes=BYTES\n  -N, --read-bytes=BYTES\n  -w, --width[=BYTES]\n  -v, --output-duplicates\n  -S, --strings[=BYTES]\n      --endian=ORDER       little or big\n      --help --version"); return }
  if opts.version { gnu.version("od"); return }
  var operands = opts.files
  var origin: Int? = null
  var label: Int? = null
  if opts.traditional {
    if operands.len() > 3 { gnu.extra_operand(operands[3]) }
    if operands.len() >= 2 and old_offset(operands[-2]) != null and old_offset(operands[-1]) != null {
      origin = old_offset(operands[-2])
      label = old_offset(operands[-1])
      operands = operands[0..operands.len() - 2]
    } else if ! operands.is_empty() and old_offset(operands[-1]) != null {
      origin = old_offset(operands[-1])
      operands = operands[0..operands.len() - 1]
    }
    if operands.len() > 1 { gnu.extra_operand(operands[1]) }
  } else if ! operands.is_empty() and (operands[-1].starts_with("+") or operands.len() == 2) {
    if let value = old_offset(operands[-1]) {
      origin = value
      operands = operands[0..operands.len() - 1]
    }
  }
  if ! operands.is_empty() and legacy_offset_overflow(operands[-1]) {
    gnu.error(f"{operands[-1]}: Numerical result out of range")
    exit 1
  }
  if opts.address == "none" { opts = {...opts, address: "n"} }
  if opts.address == "" { gnu.error("invalid output address radix '\0'; it must be one character from [doxn]"); exit 1 }
  if ! (opts.address in ["d", "o", "x", "n"]) { gnu.usage_error(f"invalid argument {gnu.quote(opts.address)} for 'address radix'") }
  let big = opts.endian != "" and "big".starts_with(opts.endian)
  if ! big and ! (opts.endian != "" and "little".starts_with(opts.endian)) { gnu.usage_error(f"invalid argument {gnu.quote(opts.endian)} for 'endian'") }
  let skip_value: Int? = if origin != null { origin } else { byte_count(opts.skip) }
  let skip_option = if "-j" in argv { "-j" } else { "--skip-bytes" }
  let count_option = if "-N" in argv { "-N" } else { "--read-bytes" }
  guard let skip = skip_value else {
    let parts = rx"^([0-9]*)(.*)$".captures(opts.skip)
    let message = if parts[1] != "" and parts[2] != "" { f"invalid suffix in {skip_option} argument {gnu.quote(opts.skip)}" } else { f"invalid {skip_option} argument {gnu.quote(opts.skip)}" }
    gnu.error(message)
    exit 1
  }
  guard let count = byte_count(opts.count) else {
    let parts = rx"^([0-9]*)(.*)$".captures(opts.count)
    let message = if parts[1] != "" and parts[2] != "" { f"invalid suffix in {count_option} argument {gnu.quote(opts.count)}" } else { f"invalid {count_option} argument {gnu.quote(opts.count)}" }
    gnu.error(message)
    exit 1
  }
  if byte_count_overflow(opts.skip) {
    gnu.error(f"{skip_option} argument {gnu.quote(opts.skip)} too large")
    exit 1
  }
  if byte_count_overflow(opts.count) {
    gnu.error(f"{count_option} argument {gnu.quote(opts.count)} too large")
    exit 1
  }
  var width_option = "--width"
  for arg in argv {
    if arg.starts_with("-w") and ! arg.starts_with("--") { width_option = "-w" }
    if arg == "--width" or arg.starts_with("--width=") { width_option = "--width" }
  }
  let parsed_width = if rx"^[0-9]+$".matches(opts.width) { tio.parse_count(opts.width) } else { null }
  guard let width_value = parsed_width else {
    let parts = rx"^([0-9]*)(.*)$".captures(opts.width)
    let message = if parts[1] != "" and parts[2] != "" { f"invalid suffix in {width_option} argument {gnu.quote(opts.width)}" } else { f"invalid {width_option} argument {gnu.quote(opts.width)}" }
    gnu.error(message)
    exit 1
  }
  if byte_count_overflow(opts.width) {
    gnu.error(f"{width_option} argument {gnu.quote(opts.width)} too large")
    exit 1
  }
  var width = width_value
  if width <= 0 {
    gnu.error(f"invalid {width_option} argument {gnu.quote(opts.width)}")
    exit 1
  }
  var selected: List[Format] = []
  for text in if requested.is_empty() { ["o2"] } else { requested } {
    guard let parsed = formats(text) else { |failure| gnu.usage_error(failure.message); return }
    selected += parsed
  }
  var unit = 1
  for fmt in selected {
    if fmt.size > unit { unit = fmt.size }
  }
  if width % unit != 0 { gnu.error(f"warning: invalid width {width}; using {unit} instead"); width = unit }
  # Every format's columns share the widest block, so narrower formats pad their fields to it.
  var block_width = 0
  for fmt in selected {
    let scaled = (fmt.chars + 1) * (width / fmt.size)
    if scaled > block_width { block_width = scaled }
  }
  # GNU's padding offset calculation squares the field count; reject widths
  # whose intermediate value cannot fit the signed Int representation.
  for fmt in selected {
    let padding = width / fmt.size - 1
    if padding > 0 and padding > 9223372036854775807 / padding {
      gnu.error(f"{width_value} is too large")
      exit 1
    }
  }
  let files = if operands.is_empty() { ["-"] } else { operands }
  var chunks: List[Bytes] = []
  var failed = false
  var remaining = if count > 9223372036854775807 - skip { 9223372036854775807 } else { count + skip }
  var seekable_device: Path? = null
  for name in files {
    if name == "-" {
      while remaining > 0 {
        let want = if remaining < 65536 { remaining } else { 65536 }
        guard let block = io.stdin_read(want) else { |failure| gnu.name_error(name, failure); failed = true; break }
        break when block.is_empty()
        chunks += [block]
        remaining -= block.len()
      }
    } else {
      guard let source = tio.open_source(name) else { |failure| gnu.name_error(name, failure); failed = true; continue }
      if files.len() == 1 and source.mode == "device" { seekable_device = source.path }
      var at = 0
      while remaining > 0 {
        let want = if remaining < 65536 { remaining } else { 65536 }
        guard let block = tio.read_chunk(source, at, want) else { |failure| gnu.name_error(name, failure); failed = true; break }
        break when block.is_empty()
        let part = block.slice(0, length: remaining)
        chunks += [part]
        remaining -= part.len()
        at += part.len()
      }
    }
  }
  if failed and chunks.is_empty() { exit 1 }
  let all = bytes.concat(chunks)
  if skip > all.len() {
    if let device = seekable_device {
      if let Ok(_) = bytes.read_at(device, skip, 0) {
        if opts.address != "n" and opts.strings == "" { gnu.write_text(address(skip, opts.address, label, skip) + "\n") }
        return
      }
    }
    gnu.error("cannot skip past end of combined input")
    exit 1
  }
  let data = all.slice(skip, length: count)
  if opts.strings != "" {
    let minimum = opts.strings
    let min = minimum.parse_int() ?? -1
    if min < 0 { gnu.usage_error(f"invalid minimum string length: {gnu.quote(minimum)}") }
    var start = 0
    for at in range(data.len()) {
      let byte = data.byte_at(at) ?? 0
      if byte == 0 and at - start >= min {
        gnu.write_text(address(skip + start, opts.address, label, skip) + (if opts.address == "n" { "" } else { " " }) + (data[start..at].utf8() ?? "") + "\n")
      }
      if byte < 32 or byte > 126 { start = at + 1 }
    }
    if count <= all.len() - skip and data.len() - start >= min {
      gnu.write_text(address(skip + start, opts.address, label, skip) + (if opts.address == "n" { "" } else { " " }) + (data[start..].utf8() ?? "") + "\n")
    }
  } else {
    var previous = b""
    var compressed = false
    var offset = 0
    for block in data.chunks(width) {
      if ! opts.duplicates and block == previous and offset > 0 {
        if ! compressed { gnu.write_text("*\n"); compressed = true }
        offset += block.len()
        continue
      }
      compressed = false
      previous = block
      for index in range(selected.len()) {
        let fmt = selected[index]
        var line = if index == 0 { address(skip + offset, opts.address, label, skip) } else { spaces(if opts.address == "x" { 6 } else if opts.address != "n" or label != null { 7 } else { 0 }) }
        let fields = width / fmt.size
        let pad = block_width - fmt.chars * fields
        var printed = 0
        var column = 0
        for part in block.chunks(fmt.size) {
          let padded = bytes.concat([part, bytes.zero(fmt.size - part.len())?])
          let value = item(padded, fmt, big)?
          let field = fmt.chars + field_pad_at(fields, fields - printed, pad) - field_pad_at(fields, fields - printed - 1, pad)
          line += spaces(field - value.byte_len()) + value
          column += field
          printed += 1
        }
        if fmt.ascii {
          let text = [if byte >= 32 and byte <= 126 { bytes.from_ints([byte])?.utf8() ?? "." } else { "." } for byte in [block.byte_at(i) ?? 0 for i in range(block.len())]].join("")
          let padding = block_width - column

          if padding > 1048576 {
            gnu.write_text(line)
            write_spaces(padding)
            gnu.write_text("  >" + text + "<\n")
            line = ""
          } else {
            line += spaces(padding) + "  >" + text + "<"
          }
        }
        if ! line.is_empty() { gnu.write_text(line + "\n") }
      }
      offset += block.len()
    }
    if opts.address != "n" or label != null { gnu.write_text(address(skip + data.len(), opts.address, label, skip) + "\n") }
  }
  if let Err(failure) = io.flush_stdout() { gnu.write_failed(failure) }
  if failed { exit 1 }
}
