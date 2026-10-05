#!/bin/xsh
use lib.gnu
use lib.textio_a1 as tio

type Format = {kind: Str, size: Int, chars: Int, ascii: Bool}
const DIGITS = "0123456789abcdef"
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
    if kind == "f" { fail "floating point formats require a native binary floating point decoder" }
    var size = if kind in ["a", "c"] { 1 } else { 4 }
    if i < text.byte_len() and kind != "a" and kind != "c" {
      let next = text.byte_slice(i, length: 1)
      if next in ["1", "2", "4", "8", "C", "S", "I", "L"] {
        size = if next in ["1", "C"] { 1 } else if next in ["2", "S"] { 2 } else if next in ["4", "I"] { 4 } else { 8 }
        i += 1
      }
    }
    var ascii = false
    if i < text.byte_len() and text.byte_slice(i, length: 1) == "z" { ascii = true; i += 1 }
    let chars = if kind in ["a", "c"] { 3 } else if kind == "x" { size * 2 } else if kind == "o" { (size * 8 + 2) / 3 } else if kind == "d" { if size == 1 { 4 } else if size == 2 { 6 } else if size == 4 { 11 } else { 20 } } else { if size == 1 { 3 } else if size == 2 { 5 } else if size == 4 { 10 } else { 20 } }
    out += [{kind: kind, size: size, chars: chars, ascii: ascii}]
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

pure item(data: Bytes, fmt: Format, big: Bool) -> Result[Str] {
  let byte = data.byte_at(0) ?? 0
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
    return text + (if text == "" { "" } else { " " }) + "(" + number(value + offset - origin, 8, 7) + ")"
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
          requested += [if char == "a" { "a" } else if char == "c" { "c" } else if char == "b" { "o1" } else if char in ["d", "s"] { if char == "d" { "u2" } else { "d2" } } else if char in ["h", "x"] { "x2" } else if char in ["H", "X"] { "x4" } else if char == "o" { "o2" } else if char == "O" { "o4" } else if char in ["I", "L"] { "d8" } else if char in ["i", "l", "D"] { if char == "D" { "u4" } else { "d4" } } else { "f" }]
          position += 1
        } else if char == "t" {
          if position + 1 < arg.byte_len() { requested += [arg.byte_slice(position + 1)] } else {
            if i + 1 >= argv.len() { gnu.usage_error("option requires an argument -- 't'") }
            i += 1
            requested += [argv[i]]
          }
          break
        } else if char == "v" { args += ["-v"]; position += 1 } else {
          args += ["-" + arg.byte_slice(position)]
          if char in ["A", "j", "N"] and position + 1 == arg.byte_len() and i + 1 < argv.len() { i += 1; args += [argv[i]] }
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
  if opts.address == "none" { opts = {...opts, address: "n"} }
  if ! (opts.address in ["d", "o", "x", "n"]) { gnu.usage_error(f"invalid argument {gnu.quote(opts.address)} for 'address radix'") }
  let big = opts.endian != "" and "big".starts_with(opts.endian)
  if ! big and ! (opts.endian != "" and "little".starts_with(opts.endian)) { gnu.usage_error(f"invalid argument {gnu.quote(opts.endian)} for 'endian'") }
  let skip_value: Int? = if origin != null { origin } else { byte_count(opts.skip) }
  guard let skip = skip_value else { gnu.usage_error(f"invalid number of bytes to skip: {gnu.quote(opts.skip)}"); return }
  guard let count = byte_count(opts.count) else { gnu.usage_error(f"invalid number of bytes: {gnu.quote(opts.count)}"); return }
  var width = opts.width.parse_int() ?? 0
  if width <= 0 { gnu.error(f"invalid -w argument {gnu.quote(opts.width)}"); exit 1 }
  var selected: List[Format] = []
  for text in if requested.is_empty() { ["o2"] } else { requested } {
    guard let parsed = formats(text) else { |failure| gnu.usage_error(failure.message); return }
    selected += parsed
  }
  var alignment = 0
  var unit = 1
  for fmt in selected {
    let scaled = (fmt.chars + 1) * 8 / fmt.size
    if scaled > alignment { alignment = scaled }
    if fmt.size > unit { unit = fmt.size }
  }
  if width % unit != 0 { gnu.error(f"warning: invalid width {width}; using {unit} instead"); width = unit }
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
        var line = if index == 0 { address(skip + offset, opts.address, label, skip) } else { spaces(address(skip + offset, opts.address, label, skip).byte_len()) }
        for part in block.chunks(fmt.size) {
          let padded = bytes.concat([part, bytes.zero(fmt.size - part.len())?])
          let value = item(padded, fmt, big)?
          line += " " + spaces((alignment * fmt.size + 7) / 8 - 1 - value.byte_len()) + value
        }
        if fmt.ascii {
          let text = [if byte >= 32 and byte <= 126 { bytes.from_ints([byte])?.utf8() ?? "." } else { "." } for byte in [block.byte_at(i) ?? 0 for i in range(block.len())]].join("")
          line += spaces((width + fmt.size - 1) / fmt.size * ((alignment * fmt.size + 7) / 8) - (block.len() + fmt.size - 1) / fmt.size * ((alignment * fmt.size + 7) / 8)) + "  >" + text + "<"
        }
        gnu.write_text(line + "\n")
      }
      offset += block.len()
    }
    if opts.address != "n" or label != null { gnu.write_text(address(skip + data.len(), opts.address, label, skip) + "\n") }
  }
  if let Err(failure) = io.flush_stdout() { gnu.write_failed(failure) }
  if failed { exit 1 }
}
