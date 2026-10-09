#!/bin/xsh
use lib.gnu

const USAGE = """Usage: basenc [OPTION]... [FILE]
basenc encode or decode FILE, or standard input, to standard output.

  --base64             same as base64
  --base64url          base64 with the URL-safe alphabet
  --base32             same as base32
  --base32hex          extended hexadecimal base32
  --base16             hexadecimal encoding
  --base2lsbf          least-significant bit first
  --base2msbf          most-significant bit first
  --base58             Bitcoin-style base58 encoding
  --z85                ZeroMQ Z85 encoding
  -d, --decode          decode data
  -i, --ignore-garbage  when decoding, ignore non-alphabet characters
  -w, --wrap=COLS       wrap encoded lines after COLS characters (default 76)
      --help            display this help and exit
      --version         output version information and exit
"""

const BASE32 = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"
const BASE32HEX = "0123456789ABCDEFGHIJKLMNOPQRSTUV"
const BASE58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
const Z85 = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ.-:+=^!/*?&<>()[]{}@%$#"
const HEX = "0123456789ABCDEF"
const BINARY = "01"

type Options = {
  base64: Bool, base64url: Bool, base32: Bool, base32hex: Bool, base16: Bool,
  base2lsbf: Bool, base2msbf: Bool, base58: Bool, z85: Bool,
  decode: Bool, ignore: Bool, wrap: Str, help: Bool, version: Bool, files: List[Str]
}
type Clean = {text: Str, prefix: Str, valid: Bool}
type Decoded = {data: Bytes, valid: Bool}

pure raw_for(argv: List[Str], raw: List[Bytes], name: Str) -> Bytes {
  for index in range(argv.len()) { if argv[index] == name { return raw[index] } }
  bytes.from_text(name)
}

pure write_error(failure: Error) -> Str {
  let message = gnu.strerror(failure)
  if message.starts_with("write error: ") { message.byte_slice(13) } else { message }
}

pure alpha64(byte: Int, url: Bool) -> Bool {
  (byte >= 65 and byte <= 90) or (byte >= 97 and byte <= 122) or (byte >= 48 and byte <= 57) or (url and (byte == 45 or byte == 95)) or (! url and (byte == 43 or byte == 47))
}
pure alpha32(byte: Int, hex: Bool) -> Bool {
  if hex {
    (byte >= 48 and byte <= 57) or (byte >= 65 and byte <= 86) or (byte >= 97 and byte <= 118)
  } else {
    (byte >= 65 and byte <= 90) or (byte >= 97 and byte <= 122) or (byte >= 50 and byte <= 55)
  }
}
pure alpha16(byte: Int) -> Bool { (byte >= 48 and byte <= 57) or (byte >= 65 and byte <= 70) or (byte >= 97 and byte <= 102) }
pure alpha2(byte: Int) -> Bool { byte == 48 or byte == 49 }
pure alpha58(byte: Int) -> Bool {
  (byte >= 49 and byte <= 57) or (byte >= 65 and byte <= 90 and byte != 73 and byte != 79) or (byte >= 97 and byte <= 122 and byte != 108)
}
pure alpha85(byte: Int) -> Bool {
  byte >= 33 and byte <= 126 and byte not in [34, 39, 44, 59, 92, 95, 96, 124, 126]
}

pure clean(data: Bytes, ignore: Bool, encoding: Str) -> Clean {
  var text = ""
  var prefix = ""
  var prefix_open = true
  var valid = true
  for index in range(data.len()) {
    let byte = data.byte_at(index) ?? 0
    if byte == 10 {
      if encoding == "base64" or encoding == "base64url" {
        text = f"{text}\n"
        if prefix_open { prefix = f"{prefix}\n" }
      }
      continue
    }
    let allowed = if encoding == "base64" or encoding == "base64url" { alpha64(byte, encoding == "base64url") or byte == 61 } else if encoding == "base32" or encoding == "base32hex" { alpha32(byte, encoding == "base32hex") or byte == 61 } else if encoding == "base16" { alpha16(byte) } else if encoding == "base2lsbf" or encoding == "base2msbf" { alpha2(byte) } else if encoding == "base58" { alpha58(byte) } else { alpha85(byte) }
    if allowed {
      let character = data[index..index + 1].utf8() ?? ""
      text = f"{text}{character}"
      if prefix_open { prefix = f"{prefix}{character}" }
    } else if ! ignore {
      valid = false
      prefix_open = false
    }
  }
  {text: text, prefix: prefix, valid: valid}
}

pure wrap(text: Str, width: Int) -> Bytes {
  if text == "" { return b"" }
  if width == 0 { return bytes.from_text(text) }
  var chunks: List[Bytes] = []
  var at = 0
  while at < text.byte_len() {
    let end = if at + width < text.byte_len() { at + width } else { text.byte_len() }
    chunks += [bytes.from_text(text.byte_slice(at, end - at)), b"\n"]
    at = end
  }
  bytes.concat(chunks)
}

pure digit(value: Int, alphabet: Str) -> Str { alphabet.byte_slice(value, 1) }

pure base32hex_encode(data: Bytes) -> Str {
  let encoded = data.base32()
  var out = ""
  for index in range(encoded.byte_len()) {
    let part = encoded.byte_slice(index, 1)
    let found = BASE32.find(part)
    out = f"{out}{if found == null { "=" } else { digit(found ?? 0, BASE32HEX) }}"
  }
  out
}

pure base32_decode_stream(text: Str) -> Decoded {
  var chunks: List[Bytes] = []
  var at = 0
  while at < text.byte_len() {
    let left = text.byte_len() - at
    let count = if left >= 8 { 8 } else { left }
    let piece = text.byte_slice(at, count)
    if count < 8 and count not in [2, 4, 5, 7] {
      let valid_prefix = if count == 3 { 2 } else if count == 6 { 5 } else { 0 }
      if valid_prefix > 0 {
        if let Ok(decoded) = piece.byte_slice(0, valid_prefix).base32_decode() { chunks += [decoded] }
      }
      return {data: bytes.concat(chunks), valid: false}
    }
    if let Ok(decoded) = piece.base32_decode() {
      chunks += [decoded]
      at += count
    } else {
      var valid_prefix = 0
      while valid_prefix < count and alpha32(piece.byte_at(valid_prefix) ?? 0, false) { valid_prefix += 1 }
      if valid_prefix > 0 {
        if let Ok(prefix) = piece.byte_slice(0, valid_prefix).base32_decode() { chunks += [prefix] }
      }
      return {data: bytes.concat(chunks), valid: false}
    }
  }
  {data: bytes.concat(chunks), valid: true}
}

pure base64_decode_stream(text: Str) -> Decoded {
  var chunks: List[Bytes] = []
  var pending = ""
  for index in range(text.byte_len()) {
    let part = text.byte_slice(index, 1)
    if part == "\n" {
      if pending.ends_with("=") {
        match pending.base64_decode() {
          Ok(decoded) => { chunks += [decoded]; pending = "" }
          Err(_) => { return {data: bytes.concat(chunks), valid: false} }
        }
      }
    } else {
      pending = f"{pending}{part}"
    }
  }
  if pending != "" {
    match pending.base64_decode() {
      Ok(decoded) => { chunks += [decoded] }
      Err(_) => { return {data: bytes.concat(chunks), valid: false} }
    }
  }
  {data: bytes.concat(chunks), valid: true}
}

pure base32hex_decode(text: Str) -> Decoded {
  var standard = ""
  for index in range(text.byte_len()) {
    let part = text.byte_slice(index, 1)
    let found = BASE32HEX.find(part.upper())
    standard = f"{standard}{if part == "=" { "=" } else if found == null { "!" } else { digit(found ?? 0, BASE32) }}"
  }
  base32_decode_stream(standard)
}

pure base16_encode(data: Bytes) -> Str {
  var out = ""
  for index in range(data.len()) {
    let byte = data.byte_at(index) ?? 0
    out = f"{out}{digit(byte / 16, HEX)}{digit(byte % 16, HEX)}"
  }
  out
}

pure hex_value(ch: Str) -> Int? {
  let n = HEX.find(ch.upper())
  n
}

proc base16_decode(text: Str) [error] -> Decoded {
  return {data: b"", valid: false} when text.byte_len() % 2 != 0
  var values: List[Int] = []
  var at = 0
  while at < text.byte_len() {
    let hi = hex_value(text.byte_slice(at, 1))
    let lo = hex_value(text.byte_slice(at + 1, 1))
    if hi == null or lo == null { return {data: bytes.from_ints(values)?, valid: false} }
    values += [(hi ?? 0) * 16 + (lo ?? 0)]
    at += 2
  }
  {data: bytes.from_ints(values)?, valid: true}
}

pure base2_encode(data: Bytes, lsb: Bool) -> Str {
  var out = ""
  for byte in data {
    for i in range(8) {
      let factor = [128, 64, 32, 16, 8, 4, 2, 1][if lsb { 7 - i } else { i }]
      out = f"{out}{digit(byte / factor % 2, BINARY)}"
    }
  }
  out
}

proc base2_decode(text: Str, lsb: Bool) [error] -> Decoded {
  if text.byte_len() % 8 != 0 { return {data: b"", valid: false} }
  var values: List[Int] = []
  var at = 0
  while at < text.byte_len() {
    var byte = 0
    for i in range(8) {
      let bit = if text.byte_slice(at + i, 1) == "1" { 1 } else { 0 }
      let factor = [128, 64, 32, 16, 8, 4, 2, 1][if lsb { 7 - i } else { i }]
      byte += bit * factor
    }
    values += [byte]
    at += 8
  }
  {data: bytes.from_ints(values)?, valid: true}
}

pure base58_encode(data: Bytes) -> Str {
  var zeroes = 0
  while zeroes < data.len() and data.byte_at(zeroes) == 0 { zeroes += 1 }
  var digits: List[Int] = []
  for index in range(zeroes, data.len()) {
    var carry = data.byte_at(index) ?? 0
    for at in range(digits.len()) {
      carry += digits[at] * 256
      digits[at] = carry % 58
      carry = carry / 58
    }
    while carry > 0 { digits += [carry % 58]; carry = carry / 58 }
  }
  var out = ""
  for _ in range(zeroes) { out = f"{out}1" }
  for index in range(digits.len()) { out = f"{out}{digit(digits[digits.len() - index - 1], BASE58)}" }
  out
}

proc base58_decode(text: Str) [error] -> Decoded {
  var zeroes = 0
  while zeroes < text.byte_len() and text.byte_slice(zeroes, 1) == "1" { zeroes += 1 }
  var little: List[Int] = []
  for at in range(zeroes, text.byte_len()) {
    let value = BASE58.find(text.byte_slice(at, 1))
    if value == null { return {data: b"", valid: false} }
    var carry = value ?? 0
    for i in range(little.len()) {
      carry += little[i] * 58
      little[i] = carry % 256
      carry = carry / 256
    }
    while carry > 0 { little += [carry % 256]; carry = carry / 256 }
  }
  var values: List[Int] = []
  for _ in range(zeroes) { values += [0] }
  for index in range(little.len()) { values += [little[little.len() - index - 1]] }
  {data: bytes.from_ints(values)?, valid: true}
}

pure z85_encode(data: Bytes) -> Str {
  var out = ""
  var at = 0
  while at < data.len() {
    var value = (data.byte_at(at) ?? 0) * 16777216 + (data.byte_at(at + 1) ?? 0) * 65536 + (data.byte_at(at + 2) ?? 0) * 256 + (data.byte_at(at + 3) ?? 0)
    var word = ""
    for _ in range(5) { word = f"{digit(value % 85, Z85)}{word}"; value = value / 85 }
    out = f"{out}{word}"
    at += 4
  }
  out
}

proc z85_decode(text: Str) [error] -> Decoded {
  if text.byte_len() % 5 != 0 { return {data: b"", valid: false} }
  var values: List[Int] = []
  var at = 0
  while at < text.byte_len() {
    var value = 0
    for i in range(5) {
      let d = Z85.find(text.byte_slice(at + i, 1))
      if d == null { return {data: bytes.from_ints(values)?, valid: false} }
      value = value * 85 + (d ?? 0)
    }
    if value > 4294967295 { return {data: bytes.from_ints(values)?, valid: false} }
    values += [value / 16777216, value / 65536 % 256, value / 256 % 256, value % 256]
    at += 5
  }
  {data: bytes.from_ints(values)?, valid: true}
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: Options = cli.applet(
    argv,
    {
      gnu: {status: 1},
      base64: {form: "--base64", default: false, conflicts: ["base64url", "base32", "base32hex", "base16", "base2lsbf", "base2msbf", "base58", "z85"]},
      base64url: {form: "--base64url", default: false, conflicts: ["base64", "base32", "base32hex", "base16", "base2lsbf", "base2msbf", "base58", "z85"]},
      base32: {form: "--base32", default: false, conflicts: ["base64", "base64url", "base32hex", "base16", "base2lsbf", "base2msbf", "base58", "z85"]},
      base32hex: {form: "--base32hex", default: false, conflicts: ["base64", "base64url", "base32", "base16", "base2lsbf", "base2msbf", "base58", "z85"]},
      base16: {form: "--base16", default: false, conflicts: ["base64", "base64url", "base32", "base32hex", "base2lsbf", "base2msbf", "base58", "z85"]},
      base2lsbf: {form: "--base2lsbf", default: false, conflicts: ["base64", "base64url", "base32", "base32hex", "base16", "base2msbf", "base58", "z85"]},
      base2msbf: {form: "--base2msbf", default: false, conflicts: ["base64", "base64url", "base32", "base32hex", "base16", "base2lsbf", "base58", "z85"]},
      base58: {form: "--base58", default: false, conflicts: ["base64", "base64url", "base32", "base32hex", "base16", "base2lsbf", "base2msbf", "z85"]},
      z85: {form: "--z85", default: false, conflicts: ["base64", "base64url", "base32", "base32hex", "base16", "base2lsbf", "base2msbf", "base58"]},
      decode: {form: "-d -D --decode", default: false},
      ignore: {form: "-i --ignore", default: false},
      wrap: {form: "-w --wrap COLS", default: "76"},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      files: {form: "...FILE"},
    },
  )?
  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.write_text("basenc (uutils coreutils) 0.13.0\n"); return }
  var encoding = ""
  if opts.base64 { encoding = "base64" }
  if opts.base64url { encoding = "base64url" }
  if opts.base32 { encoding = "base32" }
  if opts.base32hex { encoding = "base32hex" }
  if opts.base16 { encoding = "base16" }
  if opts.base2lsbf { encoding = "base2lsbf" }
  if opts.base2msbf { encoding = "base2msbf" }
  if opts.base58 { encoding = "base58" }
  if opts.z85 { encoding = "z85" }
  if encoding == "" { gnu.usage_error("missing encoding type") }
  if opts.files.len() > 1 { gnu.extra_operand(opts.files[1]) }
  let width = match opts.wrap.parse_int() { Ok(value) if value >= 0 => value, _ => { gnu.error(f"invalid wrap size: {gnu.quote_value(opts.wrap)}"); exit 1; 0 } }

  var data = b""
  if opts.files.len() == 0 or opts.files[0] == "-" {
    match io.stdin_bytes() { Ok(value) => data = value, Err(failure) => { gnu.error(f"read error: {gnu.strerror(failure)}"); exit 1 } }
  } else {
    let raw_name = raw_for(argv, cli.argv_bytes(), opts.files[0])
    let target = Path.parse_bytes(raw_name)?
    let is_directory = match fs.stat(target, follow_symlinks: true) { Ok(meta) => meta.kind == "dir", Err(_) => false }
    if is_directory { gnu.error("read error: Is a directory"); exit 1 }
    match target.read_bytes() {
      Ok(value) => data = value,
      Err(failure) => {
        if gnu.errno(failure) in [5, 21] { gnu.error(f"read error: {gnu.strerror(failure)}") } else { gnu.error(f"{gnu.quote_bytes(raw_name, always: false)}: {gnu.strerror(failure)}") }
        exit 1
      },
    }
  }

  if ! opts.decode {
    let encoded = if encoding == "base64" { data.base64() } else if encoding == "base64url" { data.base64().replace("+", "-").replace("/", "_") } else if encoding == "base32" { data.base32() } else if encoding == "base32hex" { base32hex_encode(data) } else if encoding == "base16" { base16_encode(data) } else if encoding == "base2lsbf" { base2_encode(data, true) } else if encoding == "base2msbf" { base2_encode(data, false) } else if encoding == "base58" { base58_encode(data) } else if data.len() % 4 != 0 { gnu.error("error: invalid input (length must be multiple of 4 characters)"); exit 1 } else { z85_encode(data) }
    if let Err(failure) = io.write_stdout_bytes(wrap(encoded, width)) { gnu.error(write_error(failure)); exit 1 }
  } else {
    let cleaned = clean(data, opts.ignore, encoding)
    if ! cleaned.valid {
      if encoding == "base32" or encoding == "base32hex" {
        let partial = if encoding == "base32" { base32_decode_stream(cleaned.prefix) } else { base32hex_decode(cleaned.prefix) }
        if let Err(failure) = io.write_stdout_bytes(partial.data) { gnu.error(write_error(failure)); exit 1 }
        if let Err(failure) = io.flush_stdout() { gnu.error(write_error(failure)); exit 1 }
      }
      gnu.error("error: invalid input"); exit 1
    }
    let decoded = if encoding == "base64" or encoding == "base64url" {
      let standard = cleaned.text.replace("-", "+").replace("_", "/")
      base64_decode_stream(standard)
    } else if encoding == "base32" {
      base32_decode_stream(cleaned.text)
    } else if encoding == "base32hex" { base32hex_decode(cleaned.text) } else if encoding == "base16" { base16_decode(cleaned.text) } else if encoding == "base2lsbf" { base2_decode(cleaned.text, true) } else if encoding == "base2msbf" { base2_decode(cleaned.text, false) } else if encoding == "base58" { base58_decode(cleaned.text) } else { z85_decode(cleaned.text) }
    if ! decoded.valid {
      if encoding == "base32" or encoding == "base32hex" {
        if let Err(failure) = io.write_stdout_bytes(decoded.data) { gnu.error(write_error(failure)); exit 1 }
        if let Err(failure) = io.flush_stdout() { gnu.error(write_error(failure)); exit 1 }
      }
      gnu.error("error: invalid input"); exit 1
    }
    if let Err(failure) = io.write_stdout_bytes(decoded.data) { gnu.error(write_error(failure)); exit 1 }
  }
  if let Err(failure) = io.flush_stdout() { gnu.error(write_error(failure)); exit 1 }
}
