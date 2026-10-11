##! Binary encodings and GNU encoding command argument handling.

use gnu

type Decoded = {data: Bytes, valid: Bool}

const B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
const B32 = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"
const B32HEX = "0123456789ABCDEFGHIJKLMNOPQRSTUV"
const HEX = "0123456789ABCDEF"
const Z85 = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ.-:+=^!/*?&<>()[]{}@%$#"
const B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"

pure reverse_byte(value: Int) -> Int {
  var n = value
  var out = 0
  repeat 8 times {
    out = out * 2 + n % 2
    n /= 2
  }
  out
}

pure alphabet(kind: Str) -> Str {
  return B64 when kind == "base64"
  return "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_" when kind == "base64url"
  return B32 when kind == "base32"
  return B32HEX when kind == "base32hex"
  return HEX when kind == "base16"
  "01"
}

pure radix(kind: Str) -> Int {
  return 64 when kind in ["base64", "base64url"]
  return 32 when kind in ["base32", "base32hex"]
  return 16 when kind == "base16"
  2
}

# The accumulator retains only bits that have not yet been emitted, so it
# stays bounded even for inputs larger than the machine integer range.
pure encode(data: Bytes, kind: Str) -> Result[Str] {
  if kind == "base64" { return Ok(data.base64()) }
  if kind == "base32" { return Ok(data.base32()) }
  if kind == "z85" {
    if data.len() % 4 != 0 { fail "invalid input (length must be multiple of 4 characters)" }
    var out = ""
    for block in data.chunks(4) {
      var n = bytes.unpack_be(block, 4)?
      var word = ""
      repeat 5 times {
        word = Z85.byte_slice(n % 85, length: 1) + word
        n /= 85
      }
      out += word
    }
    return Ok(out)
  }
  if kind == "base58" {
    var digits: List[Int] = []
    var zeros = 0
    var leading = true
    for byte in [data.byte_at(i) ?? 0 for i in range(data.len())] {
      if leading and byte == 0 { zeros += 1 } else { leading = false }
      var carry = byte
      for i in range(digits.len()) {
        carry += digits[i] * 256
        digits[i] = carry % 58
        carry /= 58
      }
      while carry > 0 { digits += [carry % 58]; carry /= 58 }
    }
    var out = ["1" for _ in range(zeros)].join("")
    for i in range(digits.len()) { out += B58.byte_slice(digits[digits.len() - i - 1], length: 1) }
    return Ok(out)
  }
  let chars = alphabet(kind)
  let base = radix(kind)
  var acc = 0
  var scale = 1
  var out = ""
  for raw in [data.byte_at(i) ?? 0 for i in range(data.len())] {
    let byte = if kind == "base2lsbf" { reverse_byte(raw) } else { raw }
    acc = acc * 256 + byte
    scale *= 256
    while scale >= base {
      scale /= base
      out += chars.byte_slice(acc / scale, length: 1)
      acc %= scale
    }
  }
  if scale > 1 { out += chars.byte_slice(acc * base / scale, length: 1) }
  let quantum = if base == 64 { 4 } else if base == 32 { 8 } else { 1 }
  while out.byte_len() % quantum != 0 { out += "=" }
  Ok(out)
}

pure has_any_byte(data: Bytes, targets: List[Int]) -> Bool {
  for position in range(data.len()) {
    return true when (data.byte_at(position) ?? 0) in targets
  }
  false
}

pure decode(data: Bytes, kind: Str, ignore: Bool) -> Result[Decoded] {
  let chars = if kind == "z85" { Z85 } else if kind == "base58" { B58 } else { alphabet(kind) }
  # GNU rejects a base64url input that contains the standard alphabet's '+' or '/'
  # before it decodes any of it. The check covers the whole input, which is GNU's
  # behavior for inputs within one read block. With --ignore-garbage those bytes
  # are dropped first, so they never reach this check.
  if kind == "base64url" and ! ignore and has_any_byte(data, [43, 47]) {
    return Ok({data: b"", valid: false})
  }
  var values: List[Int] = []
  var valid = true
  var padded = false
  var pads = 0
  for position in range(data.len()) {
    let byte = data.byte_at(position) ?? 0
    if byte == 10 or byte == 13 { continue }
    if byte == 61 and kind in ["base64", "base64url", "base32", "base32hex"] {
      padded = true
      pads += 1
      continue
    }
    let char = bytes.from_ints([byte])?.utf8() ?? ""
    let symbol = if kind == "base16" { char.upper() } else { char }
    let value: Int? = if symbol == "" { null } else { chars.find(symbol) }
    if value == null {
      if ignore { continue }
      valid = false
      break
    }
    if padded {
      let prefix = decode(data[0..position], kind, ignore)?
      if ! prefix.valid { return Ok(prefix) }
      let suffix = decode(data[position..], kind, ignore)?
      return Ok({data: bytes.concat([prefix.data, suffix.data]), valid: suffix.valid})
    }
    values += [value ?? 0]
  }
  var out: List[Int] = []
  if kind == "base58" {
    # GNU accumulates base58 input before converting it, so an invalid byte
    # produces no output at all.
    return Ok({data: b"", valid: false}) when ! valid
    var zeros = 0
    var leading = true
    for value in values {
      if leading and value == 0 { zeros += 1 } else { leading = false }
      var carry = value
      for i in range(out.len()) {
        carry += out[i] * 58
        out[i] = carry % 256
        carry /= 256
      }
      while carry > 0 { out += [carry % 256]; carry /= 256 }
    }
    let result = [@[0 for _ in range(zeros)], @[out[out.len() - i - 1] for i in range(out.len())]]
    return Ok({data: bytes.from_ints(result)?, valid: valid})
  }
  if kind == "z85" {
    var acc = 0
    var count = 0
    for value in values {
      acc = acc * 85 + value
      count += 1
      if count == 5 {
        if acc > 4294967295 { valid = false; break }
        out += [acc / 16777216, acc / 65536 % 256, acc / 256 % 256, acc % 256]
        acc = 0
        count = 0
      }
    }
    return Ok({data: bytes.from_ints(out)?, valid: valid and count == 0})
  }
  let base = radix(kind)
  var acc = 0
  var scale = 1
  for value in values {
    acc = acc * base + value
    scale *= base
    if scale >= 256 {
      scale /= 256
      let byte = acc / scale
      out += [if kind == "base2lsbf" { reverse_byte(byte) } else { byte }]
      acc %= scale
    }
  }
  let quantum = if base == 64 { 4 } else if base == 32 { 8 } else if base == 16 { 2 } else { 8 }
  let remainder = values.len() % quantum
  let legal = if base == 64 { remainder in [0, 2, 3] } else if base == 32 { remainder in [0, 2, 4, 5, 7] } else { remainder == 0 }
  let correct_pad = pads == 0 or (remainder > 0 and pads == quantum - remainder)
  # Bits left after the last whole byte must be zero: GNU 9.12 rejects encodings
  # whose padding bits are not zero, with or without padding characters.
  Ok({data: bytes.from_ints(out)?, valid: valid and legal and correct_pad and acc == 0})
}

pure wrap(text: Str, width: Int) -> Str {
  return text when width == 0 or text == ""
  let lines = [text.byte_slice(i * width, length: width) for i in range((text.byte_len() + width - 1) / width)]
  lines.join("\n") + "\n"
}

# Operands stay bytes until they are opened, so a file name that is not UTF-8
# reaches the filesystem unchanged. Stdin is the only operand that is not a path.
proc read_operand(name: Bytes) [fs, error, io] -> Result[Bytes, Error] {
  return io.stdin_bytes() when name == b"-"
  let target = Path.parse_bytes(name)?
  target.read_bytes()
}

proc report_name_error(name: Bytes, failure: Error) [process, env] {
  if let Ok(text) = name.utf8() {
    gnu.name_error(text, failure)
  } else {
    gnu.error(f"{gnu.quote_bytes(name, always: false)}: {gnu.strerror(failure)}")
  }
}

## Run an encoding applet with a fixed alphabet, or selectable basenc alphabets.
export proc execute(raw_argv: List[Bytes], default_kind: Str) [fs, process, env, error, io] {
  let prepared = gnu.prepare_arguments(raw_argv)
  let argv = prepared.text
  # Encoding selectors use ordered argv rather than Boolean fields: GNU
  # permits multiple selectors and the last one chooses the encoding.
  var kind = default_kind
  var args: List[Str] = []
  var options = true
  let selectors = ["--base64", "--base64url", "--base32", "--base32hex", "--base16", "--base2msbf", "--base2lsbf", "--z85", "--base58"]
  for arg in argv {
    if arg == "--" { options = false }
    let candidates = if options and arg.starts_with("--") and arg != "--" and default_kind == "" { [selector for selector in selectors if selector.starts_with(arg)] } else { [] }
    if options and arg in selectors and default_kind == "" {
      kind = arg.byte_slice(2)
    } else if candidates.len() == 1 {
      kind = candidates[0].byte_slice(2)
    } else if candidates.len() > 1 {
      gnu.usage_error(f"option {gnu.quote(arg)} is ambiguous")
    } else { args += [arg] }
  }
  let opts = cli.applet(args, {
    gnu: {status: 1},
    decode: {form: "-d -D --decode", default: false},
    ignore: {form: "-i --ignore-garbage", default: false},
    wrap: {form: "-w --wrap=COLS", default: "76"},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    files: {form: "...FILE"},
  })?
  if opts.help {
    gnu.help(f"Usage: {gnu.prog()} [OPTION]... [FILE]\nEncode or decode FILE, or standard input.\n  -d, --decode          decode data\n  -i, --ignore-garbage  ignore non-alphabet characters\n  -w, --wrap=COLS       wrap encoded lines after COLS (default 76; 0 disables)\n      --base64 --base64url --base32 --base32hex --base16\n      --base2msbf --base2lsbf --z85 --base58 (basenc selectors)\n      --help --version")
    return
  }
  if opts.version { gnu.version(gnu.prog()); return }
  if kind == "" { gnu.usage_error("missing encoding type") }
  if opts.files.len() > 1 {
    let extra = gnu.argument_bytes(opts.files[1], prepared.raw)
    match extra.utf8() {
      Ok(value) => gnu.extra_operand(value)
      Err(_) => gnu.usage_error(f"extra operand {gnu.quote_bytes(extra, always: true)}")
    }
  }
  # GNU reads the wrap width as decimal only, so `0x0` is an invalid size.
  let width = if rx"^[0-9]+$".matches(opts.wrap) { opts.wrap.parse_int() ?? -1 } else { -1 }
  if width < 0 { gnu.error(f"invalid wrap size: {gnu.quote(opts.wrap)}"); exit 1 }
  let name = gnu.argument_bytes(opts.files.get(0) ?? "-", prepared.raw)
  guard let data = read_operand(name) else { |failure|
    if gnu.errno(failure) in [21, 5] { gnu.error(f"read error: {gnu.strerror(failure)}") } else { report_name_error(name, failure) }
    exit 1
  }
  if opts.decode {
    let result = decode(data, kind, opts.ignore)?
    gnu.write_bytes(result.data)
    if ! result.valid { gnu.error("invalid input"); exit 1 }
  } else {
    guard let encoded = encode(data, kind) else { |failure|
      gnu.error(failure.message)
      exit 1
    }
    let text = wrap(encoded, width)
    gnu.write_text(text)
  }
}
