##! Shared GNU checksum formatting and verification for the checksum applets.
use gnu

type Options = {
  binary: Bool, text: Bool, check: Bool, tag: Bool, untagged: Bool,
  zero: Bool, quiet: Bool, status: Bool, warn: Bool, strict: Bool,
  ignore_missing: Bool, algorithm: Str, length: Str, base64: Bool, raw: Bool,
  help: Bool, version: Bool, files: List[Str],
}
type Numeric = {checksum: Int, size: Int}
type Check = {digest: Str, base64: Bool, path: Bytes, alternate_path: Bytes?, algorithm: Str, length: Int}
type Separator = {close: Int, digest: Int}

# Repeated mode switches obey argument order, including bundled short flags.
pure binary_mode(argv: List[Str]) -> Bool {
  var binary = false
  for arg in argv {
    if arg == "--" { break }
    if arg == "--binary" { binary = true } else if arg == "--text" { binary = false } else if arg.starts_with("-") and !arg.starts_with("--") {
      for index in range(1, arg.byte_len()) {
        let flag = arg.byte_slice(index, length: 1)
        if flag == "b" { binary = true }
        if flag == "t" { binary = false }
      }
    }
  }
  binary
}

pure tagged_mode(argv: List[Str], cksum: Bool) -> Bool {
  var tag = cksum
  for arg in argv {
    if arg == "--" { break }
    if arg == "--tag" { tag = true }
    if arg == "--untagged" { tag = false }
  }
  tag
}

# Preserve GNU's option-argument forms while the shared parser requires the
# equals-separated short form to be split into an option and its value.
pure checksum_args(argv: List[Str]) -> List[Str] {
  var result = []
  var options = true
  for arg in argv {
    if arg == "--" { options = false; result += [arg] } else if options and arg.starts_with("-a=") { result += ["-a", arg.byte_slice(3)] } else if options and arg.starts_with("--algo=") { result += ["--algorithm", arg.byte_slice(7)] } else if options and arg.starts_with("-l=") { result += ["-l", arg.byte_slice(3)] } else { result += [arg] }
  }
  result
}

pure warning_mode(argv: List[Str]) -> Bool {
  var enabled = false
  for arg in argv {
    if arg == "--" { break }
    if arg == "--status" { enabled = false } else if arg == "--warn" { enabled = true } else if arg.starts_with("-") and !arg.starts_with("--") {
      for index in range(1, arg.byte_len()) {
        if arg.byte_slice(index, length: 1) == "w" { enabled = true }
      }
    }
  }
  enabled
}

# The default length is zero, so validation must distinguish an omitted length
# from an explicit zero for SHA2 and SHA3.
pure has_length_option(argv: List[Str]) -> Bool {
  for arg in argv {
    if arg == "--" { break }
    if arg == "--length" or arg.starts_with("--length=") or arg == "-l" or arg.starts_with("-l=") or arg.starts_with("-l") { return true }
  }
  false
}

pure decimal_text(value: Str) -> Bool {
  value.byte_len() > 0 and value.translate("0123456789", "") == ""
}

pure padded(value: Int, width: Int, fill: Str) -> Str {
  var result = f"{value}"
  while result.byte_len() < width { result = fill + result }
  result
}

pure label(algorithm: Str, length: Int) -> Str {
  return f"BLAKE2b-{length}" when algorithm == "blake2b" and length != 512
  return "BLAKE2b" when algorithm == "blake2b"
  return f"BLAKE3-{length}" when algorithm == "blake3"
  return f"SHA3-{length}" when algorithm == "sha3"
  return f"SHAKE128-{length}" when algorithm == "shake128"
  return f"SHAKE256-{length}" when algorithm == "shake256"
  algorithm.upper()
}

pure digest_length(algorithm: Str) -> Int {
  match algorithm {
    "md5" => 32
    "sha1" => 40
    "sha224" => 56
    "sha256" => 64
    "sha384" => 96
    "sha512" => 128
    "sm3" => 64
    _ => 0
  }
}

pure digest_width(algorithm: Str, length: Int) -> Int {
  return (length / 8 + (if length % 8 == 0 { 0 } else { 1 })) * 2 when algorithm == "shake128" or algorithm == "shake256"
  return length / 4 when algorithm == "blake2b" or algorithm == "blake3" or algorithm == "sha3"
  digest_length(algorithm)
}

pure is_hex(value: Str) -> Bool {
  value.byte_len() > 0 and value.lower().translate("0123456789abcdef", "") == ""
}

pure valid_base64(value: Str, digest_bytes: Int) -> Bool {
  if digest_bytes < 1 or value.byte_len() != (digest_bytes + 2) / 3 * 4 { return false }
  let padding = if value.ends_with("==") { 2 } else if value.ends_with("=") { 1 } else { 0 }
  let expected_padding = if digest_bytes % 3 == 1 { 2 } else if digest_bytes % 3 == 2 { 1 } else { 0 }
  if padding != expected_padding { return false }
  let body_length = value.byte_len() - padding
  let body = value.byte_slice(0, length: body_length)
  body.translate("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/", "") == ""
}

pure escaped(name: Str) -> Str {
  name.replace("\\", with: "\\\\").replace("\n", with: "\\n").replace("\r", with: "\\r")
}

pure needs_escape(name: Str) -> Bool {
  name.find("\\") != null or name.find("\n") != null or name.find("\r") != null
}

pure unescape(name: Str) -> Str? {
  var result = ""
  var at = 0
  while at < name.byte_len() {
    let tail = name.byte_slice(at)
    let slash = tail.find("\\")
    if slash == null { return result + tail }
    let distance = slash ?? 0
    result += tail.byte_slice(0, length: distance)
    at += distance + 1
    if at >= name.byte_len() { return null }
    let escaped_tail = name.byte_slice(at)
    if escaped_tail.starts_with("n") { result += "\n" } else if escaped_tail.starts_with("r") { result += "\r" } else if escaped_tail.starts_with("\\") { result += "\\" } else { return null }
    at += 1
  }
  result
}

pure last_separator(line: Str) -> Separator? {
  var offset = 0
  var found: Separator? = null
  while offset < line.byte_len() {
    let next = line.byte_slice(offset).find(")")
    if next == null { break }
    let close = offset + (next ?? 0)
    var after = close + 1
    while after < line.byte_len() and line.byte_slice(after, length: 1) == " " { after += 1 }
    if after < line.byte_len() and line.byte_slice(after, length: 1) == "=" {
      after += 1
      if after < line.byte_len() and line.byte_slice(after, length: 1) == " " {
        while after < line.byte_len() and line.byte_slice(after, length: 1) == " " { after += 1 }
        found = {close: close, digest: after}
      }
    }
    offset = close + 1
  }
  found
}

pure byte_index(data: Bytes, byte: Int, start: Int) -> Int? {
  var at = start
  while at < data.len() {
    if data.byte_at(at) == byte { return at }
    at += 1
  }
  null
}

pure last_byte_separator(line: Bytes) -> Separator? {
  var offset = 0
  var found: Separator? = null
  while offset < line.len() {
    let next = byte_index(line, 41, offset)
    if next == null { break }
    let close = next ?? 0
    var after = close + 1
    while after < line.len() and line.byte_at(after) == 32 { after += 1 }
    if after < line.len() and line.byte_at(after) == 61 {
      after += 1
      if after < line.len() and line.byte_at(after) == 32 {
        while after < line.len() and line.byte_at(after) == 32 { after += 1 }
        found = {close: close, digest: after}
      }
    }
    offset = close + 1
  }
  found
}

pure byte_lines(data: Bytes) -> List[Bytes] {
  var result = []
  var start = 0
  for at in range(data.len()) {
    if data.byte_at(at) == 10 { result += [data[start..at]]; start = at + 1 }
  }
  if start < data.len() { result += [data[start..]] }
  result
}

pure decoded_digest_bytes(value: Str) -> Int {
  if is_hex(value) { return value.byte_len() / 2 }
  value.byte_len() / 4 * 3 - (if value.ends_with("==") { 2 } else if value.ends_with("=") { 1 } else { 0 })
}

pure malformed_label(line: Bytes, previous: Str) -> Str {
  let value = if line.byte_at(0) == 92 { line[1..] } else { line }
  let open = byte_index(value, 40, 0) ?? -1
  if open > 0 {
    var end = open
    while end > 0 and value.byte_at(end - 1) == 32 { end -= 1 }
    if end > 0 {
      if let Ok(tag) = value[..end].utf8() { return tag }
    }
  }
  previous
}

# The digest width determines the separator location: spaces and leading stars
# in a filename are data, and tagged lines may carry their own algorithm.
pure parse_line(original: Str, algorithm: Str, length: Int, infer: Bool) -> Check? {
  var line = original
  if line.ends_with("\r") { line = line.byte_slice(0, length: line.byte_len() - 1) }
  let escape = line.starts_with("\\")
  if escape { line = line.byte_slice(1) }
  var selected = algorithm
  var bits = length
  var expected = ""
  var name = ""
  var alternate_path: Str? = null
  let open = line.find("(") ?? -1
  let tag_end = if open > 0 and line.byte_slice(open - 1, length: 1) == " " { open - 1 } else { open }
  let split = last_separator(line)
  let separator = split ?? {close: -1, digest: -1}
  if tag_end > 0 and split != null and separator.close > tag_end {
    let tag = line.byte_slice(0, length: tag_end)
    if infer {
      selected = tag.lower()
      if selected == "blake2b" {
        bits = 512
      } else if selected == "blake3" {
        bits = 256
      } else if selected.starts_with("blake2b-") {
        bits = selected.byte_slice(8).parse_int() ?? 0
        selected = "blake2b"
      } else if selected.starts_with("blake3-") {
        bits = selected.byte_slice(7).parse_int() ?? 0
        selected = "blake3"
      } else if selected.starts_with("sha2-") {
        bits = selected.byte_slice(5).parse_int() ?? 0
        selected = f"sha{bits}"
      } else if selected.starts_with("sha3-") {
        bits = selected.byte_slice(5).parse_int() ?? 0
        selected = "sha3"
      } else if selected.starts_with("shake128-") {
        bits = selected.byte_slice(9).parse_int() ?? 0
        selected = "shake128"
      } else if selected.starts_with("shake256-") {
        bits = selected.byte_slice(9).parse_int() ?? 0
        selected = "shake256"
      } else if selected == "shake128" {
        bits = 256
      } else if selected == "shake256" {
        bits = 512
      }
      if algorithm == "sha2" and selected != "sha224" and selected != "sha256" and selected != "sha384" and selected != "sha512" { return null }
      if algorithm == "sha3" and selected != "sha3" { return null }
    } else if selected == "blake2b" and tag == "BLAKE2b" {
      bits = 512
    } else if selected == "blake2b" and tag.starts_with("BLAKE2b-") {
      bits = tag.byte_slice(8).parse_int() ?? 0
    } else if selected == "blake3" and tag == "BLAKE3" {
      bits = 256
    } else if selected == "blake3" and tag.starts_with("BLAKE3-") {
      bits = tag.byte_slice(7).parse_int() ?? 0
    } else if selected == "shake128" and tag.starts_with("SHAKE128-") {
      bits = tag.byte_slice(9).parse_int() ?? 0
    } else if selected == "shake128" and tag == "SHAKE128" {
      bits = 256
    } else if selected == "shake256" and tag.starts_with("SHAKE256-") {
      bits = tag.byte_slice(9).parse_int() ?? 0
    } else if selected == "shake256" and tag == "SHAKE256" {
      bits = 512
    } else if selected == "sha3" and tag.starts_with("SHA3-") {
      bits = tag.byte_slice(5).parse_int() ?? 0
    }
    if selected == "sha3" and bits != 224 and bits != 256 and bits != 384 and bits != 512 { return null }
    if selected == "blake2b" and (bits < 8 or bits > 512 or bits % 8 != 0) { return null }
    if selected == "blake3" and (bits < 8 or bits % 8 != 0) { return null }
    let sha2_variant = selected == "sha224" or selected == "sha256" or selected == "sha384" or selected == "sha512"
    let shake_default_tag = (selected == "shake128" and bits == 256 and tag == "SHAKE128") or (selected == "shake256" and bits == 512 and tag == "SHAKE256")
    let blake2b_512_alias = selected == "blake2b" and bits == 512 and tag == "BLAKE2b-512"
    let blake3_default_tag = selected == "blake3" and bits == 256 and tag == "BLAKE3"
    if tag != label(selected, bits) and !(sha2_variant and tag == f"SHA2-{selected.byte_slice(3)}") and !shake_default_tag and !blake2b_512_alias and !blake3_default_tag { return null }
    name = line.byte_slice(open + 1, length: separator.close - open - 1)
    expected = line.byte_slice(separator.digest)
  } else {
    let separator = line.find(" ") ?? -1
    if separator <= 0 { return null }
    expected = line.byte_slice(0, length: separator)
    let remainder = line.byte_slice(separator + 1)
    if remainder.starts_with(" ") {
      if !remainder.starts_with(" *") { alternate_path = remainder }
      name = remainder.byte_slice(1)
    } else if remainder.starts_with("*") {
      name = remainder.byte_slice(1)
    } else {
      name = remainder
    }
    if infer and algorithm == "sha2" {
      selected = if is_hex(expected) {
        match expected.byte_len() { 56 => "sha224", 64 => "sha256", 96 => "sha384", 128 => "sha512", _ => "" }
      } else {
        match expected.byte_len() { 40 => "sha224", 44 => "sha256", 64 => "sha384", 88 => "sha512", _ => "" }
      }
    }
    if infer and algorithm == "sha3" {
      let digest_bytes = decoded_digest_bytes(expected)
      if digest_bytes != 28 and digest_bytes != 32 and digest_bytes != 48 and digest_bytes != 64 { return null }
      bits = digest_bytes * 8
    }
    if selected == "blake2b" or selected == "blake3" or selected == "shake128" or selected == "shake256" {
      let digest_bytes = decoded_digest_bytes(expected)
      bits = digest_bytes * 8
      if selected == "blake2b" and (bits < 8 or bits > 512) { return null }
    }
  }
  let width = digest_width(selected, bits)
  let encoded = if width >= 2 and expected.byte_len() == width and is_hex(expected) { false } else if width >= 2 and valid_base64(expected, width / 2) { true } else { return null }
  if name == "" { return null }
  if escape {
    if let decoded = unescape(name) { name = decoded } else { return null }
  }
  let alternate: Bytes? = if let value = alternate_path { bytes.from_text(value) } else { null }
  {digest: expected, base64: encoded, path: bytes.from_text(name), alternate_path: alternate, algorithm: selected, length: bits}
}

pure parse_byte_line(original: Bytes, algorithm: Str, length: Int, infer: Bool) -> Check? {
  var line = original
  if line.len() > 0 and line.byte_at(line.len() - 1) == 13 { line = line[..line.len() - 1] }
  let escape = line.byte_at(0) == 92
  let content = if escape { line[1..] } else { line }
  let open = byte_index(content, 40, 0) ?? -1
  let close = last_byte_separator(content)
  if open > 0 and close != null {
    let separator = close ?? {close: -1, digest: -1}
    if separator.close <= open { return null }
    let placeholder = b"__XSH_RAW_CHECK_PATH__"
    var normalized = bytes.concat([content[..open + 1], placeholder, content[separator.close..]])
    if escape { normalized = bytes.concat([b"\\", normalized]) }
    let parsed = match normalized.utf8() {
      Ok(value) => parse_line(value, algorithm, length, infer)
      Err(_) => null
    }
    if parsed == null { return null }
    let item = parsed ?? {digest: "", base64: false, path: b"", alternate_path: null, algorithm: "", length: 0}
    return {...item, path: content[open + 1..separator.close], alternate_path: null}
  }

  let separator = byte_index(content, 32, 0) ?? -1
  if separator <= 0 { return null }
  var name_start = separator + 1
  let marker = content.byte_at(name_start)
  if marker == 32 or marker == 42 { name_start += 1 }
  let normalized = bytes.concat([content[..name_start], b"__XSH_RAW_CHECK_PATH__"])
  let text = normalized.utf8()
  let parsed = match text {
    Ok(value) => parse_line(value, algorithm, length, infer)
    Err(_) => null
  }
  if parsed == null { return null }
  let item = parsed ?? {digest: "", base64: false, path: b"", alternate_path: null, algorithm: "", length: 0}
  let alternate: Bytes? = if marker == 32 and content.byte_at(name_start) != 42 { content[separator + 1..] } else { null }
  {...item, path: content[name_start..], alternate_path: alternate}
}

proc digest(name: Str, algorithm: Str, length: Int) [fs, io] -> Result[Digest] {
  if name == "-" { hash.digest_stdin(algorithm, length: length) } else { hash.digest_file(fp"{name}", algorithm, length: length) }
}

proc digest_path(name: Bytes, algorithm: Str, length: Int) [fs, io, error] -> Result[Digest] {
  if name == b"-" { hash.digest_stdin(algorithm, length: length) } else { hash.digest_file(Path.parse_bytes(name)?, algorithm, length: length) }
}

proc name_error_bytes(name: Bytes, failure: Error) [process, env] -> Unit {
  gnu.error(f"{gnu.quote_bytes(name, always: false)}: {gnu.strerror(failure)}")
}

proc numeric(name: Str, algorithm: Str) [fs, io] -> Result[Numeric] {
  if name == "-" { hash.checksum_stdin(algorithm) } else { hash.checksum(fp"{name}", algorithm) }
}

pure raw_digest(hex: Str) -> Result[Bytes] {
  let digits = "0123456789abcdef"
  var values = []
  var at = 0
  while at < hex.byte_len() {
    values += [(digits.find(hex.byte_slice(at, length: 1)) ?? 0) * 16 + (digits.find(hex.byte_slice(at + 1, length: 1)) ?? 0)]
    at += 2
  }
  bytes.from_ints(values)
}

proc verify_list(source: Str, opts: Options, algorithm: Str, length: Int, infer: Bool) [fs, io, error, process, env] -> Bool {
  let content = if source == "-" { io.stdin_bytes() } else { fp"{source}".read_bytes() }
  let data = match content {
    Ok(value) => value
    Err(failure) => { gnu.name_error(source, failure); return false }
  }
  var malformed = 0
  var mismatched = 0
  var unreadable = 0
  var checked = 0
  var valid = 0
  var line_number = 0
  var previous_label = label(algorithm, length)
  for line in byte_lines(data) {
    line_number += 1
    if line.len() == 0 or (line.len() == 1 and line.byte_at(0) == 13) { continue }
    if line.byte_at(0) == 35 { continue }
    let entry = match line.utf8() {
      Ok(value) => parse_line(value, algorithm, length, infer)
      Err(_) => parse_byte_line(line, algorithm, length, infer)
    }
    if entry == null {
      malformed += 1
      if opts.warn {
        gnu.error(f"{source}: {line_number}: improperly formatted {malformed_label(line, previous_label)} checksum line")
      }
      continue
    }
    let item = entry
    valid += 1
    previous_label = label(item.algorithm, item.length)
    var checked_path = item.path
    var result = digest_path(checked_path, item.algorithm, item.length)
    if let Err(failure) = result {
      if item.alternate_path != null and gnu.errno(failure) == 2 {
        let alternate = item.alternate_path ?? item.path
        match digest_path(alternate, item.algorithm, item.length) {
          Ok(value) => { checked_path = alternate; result = Ok(value) }
          Err(alternate_failure) => {
            if gnu.errno(alternate_failure) != 2 { checked_path = alternate; result = Err(alternate_failure) }
          }
        }
      }
    }
    let shown = gnu.quote_bytes(checked_path, always: false)
    match result {
      Ok(value) => {
        checked += 1
        let actual = if item.base64 { value.base64() } else { value.hex() }
        if actual == item.digest {
          if !opts.quiet and !opts.status { gnu.write_text(f"{shown}: OK\n") }
        } else {
          mismatched += 1
          if !opts.status { gnu.write_text(f"{shown}: FAILED\n") }
        }
      }
      Err(failure) => {
        if opts.ignore_missing and gnu.errno(failure) == 2 { continue }
        unreadable += 1
        name_error_bytes(checked_path, failure)
        if !opts.status { gnu.write_text(f"{shown}: FAILED open or read\n") }
      }
    }
  }
  if valid == 0 {
    gnu.error(f"{if source == "-" { "'standard input'" } else { source }}: no properly formatted checksum lines found")
    return false
  }
  if !opts.status or opts.warn {
    if malformed > 0 { gnu.error(f"WARNING: {malformed} {if malformed == 1 { "line is" } else { "lines are" }} improperly formatted") }
    if unreadable > 0 { gnu.error(f"WARNING: {unreadable} listed {if unreadable == 1 { "file could" } else { "files could" }} not be read") }
    if mismatched > 0 { gnu.error(f"WARNING: {mismatched} computed {if mismatched == 1 { "checksum did" } else { "checksums did" }} NOT match") }
  }
  if checked == 0 and unreadable == 0 { gnu.error(f"{if source == "-" { "'standard input'" } else { source }}: no file was verified") }
  checked > 0 and unreadable == 0 and mismatched == 0 and (!opts.strict or malformed == 0)
}

## Parse conventional checksum options, hash files or stdin, and verify lists.
export proc execute(argv: List[Str], default_algorithm: Str, cksum = false) [fs, io, error, process, env] -> Unit {
  let parsed = cli.applet(checksum_args(argv), {
    gnu: {status: 1},
    binary: {form: "-b --binary", default: false},
    text: {form: "-t --text", default: false},
    check: {form: "-c --check", default: false},
    tag: {form: "--tag", default: false},
    untagged: {form: "--untagged", default: false},
    zero: {form: "-z --zero", default: false},
    quiet: {form: "--quiet", default: false},
    status: {form: "--status", default: false},
    warn: {form: "-w --warn", default: false},
    strict: {form: "--strict", default: false},
    ignore_missing: {form: "--ignore-missing", default: false},
    algorithm: {form: "-a --algorithm ALGORITHM", default: ""},
    length: {form: "-l --length BITS", default: "0"},
    base64: {form: "--base64", default: false},
    raw: {form: "--raw", default: false},
    debug: {form: "--debug", unsupported: true},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    files: {form: "...FILE"},
  })
  var opts: Options = match parsed {
    Ok(value) => value
    Err(failure) => {
      let prefix = f"{gnu.prog()}: "
      let diagnostic = if failure.message.starts_with(prefix) { failure.message.byte_slice(prefix.byte_len()) } else { failure.message }
      gnu.error(diagnostic)
      exit 1
    }
  }
  opts = {...opts, warn: warning_mode(argv)}
  if opts.help {
    gnu.help(f"Usage: {gnu.prog()} [OPTION]... [FILE]...\nPrint or check checksums. With no FILE, or when FILE is -, read standard input.\n  -b, --binary       read in binary mode\n  -c, --check        read checksums and verify files\n  -t, --text         read in text mode\n      --tag          create a BSD-style checksum\n  -z, --zero         end each output line with NUL\n      --quiet        omit OK when checking\n      --status       suppress check output\n      --strict       fail on malformed check lines\n  -w, --warn         warn about malformed check lines\n      --ignore-missing  ignore missing files when checking\n      --help         display this help\n      --version      display version")
    return
  }
  if opts.version { gnu.version(gnu.prog()); return }
  if !cksum and opts.algorithm != "" { gnu.usage_error("the --algorithm option is supported only by cksum") }
  if !cksum and (opts.untagged or opts.base64 or opts.raw) { gnu.usage_error("--untagged, --base64 and --raw are supported only by cksum") }
  let binary = binary_mode(argv)
  let tagged = tagged_mode(argv, cksum)
  var algorithm = if opts.algorithm == "" { default_algorithm } else { opts.algorithm }
  var length = opts.length.parse_int() ?? -1
  let length_given = has_length_option(argv)
  if algorithm == "sha2" or algorithm == "sha3" {
    if !length_given {
      if !opts.check { gnu.usage_error(f"--algorithm={algorithm} requires specifying --length 224, 256, 384, or 512") }
    } else {
      if length < 0 {
        if decimal_text(opts.length) {
          gnu.error(f"invalid length: {gnu.quote(opts.length)}")
          gnu.usage_error(f"digest length for '{algorithm.upper()}' must be 224, 256, 384, or 512")
        }
        gnu.usage_error(f"invalid length: {gnu.quote(opts.length)}")
      }
      if length != 224 and length != 256 and length != 384 and length != 512 {
        gnu.error(f"invalid length: {gnu.quote(opts.length)}")
        gnu.usage_error(f"digest length for '{algorithm.upper()}' must be 224, 256, 384, or 512")
      }
      if algorithm == "sha2" { algorithm = f"sha{length}" }
    }
  } else if algorithm == "blake2b" or algorithm == "blake3" {
    if length < 0 {
      if decimal_text(opts.length) and algorithm == "blake2b" {
        gnu.error(f"invalid length: {gnu.quote(opts.length)}")
        gnu.usage_error(f"maximum digest length for '{label(algorithm, 512)}' is 512 bits")
      }
      gnu.usage_error(f"invalid length: {gnu.quote(opts.length)}")
    }
    if length == 0 { length = if algorithm == "blake2b" { 512 } else { 256 } }
    if algorithm == "blake2b" and length > 512 {
      gnu.error(f"invalid length: {gnu.quote(opts.length)}")
      gnu.usage_error(f"maximum digest length for '{label(algorithm, 512)}' is 512 bits")
    }
    if length % 8 != 0 {
      gnu.error(f"invalid length: {gnu.quote(opts.length)}")
      gnu.usage_error("length is not a multiple of 8")
    }
  } else if algorithm == "shake128" or algorithm == "shake256" {
    if length < 0 { gnu.usage_error(f"invalid length: {gnu.quote(opts.length)}") }
    if length == 0 { length = if algorithm == "shake128" { 256 } else { 512 } }
  } else if length != 0 {
    gnu.usage_error("--length is only supported with --algorithm blake2b, sha2, or sha3")
  }
  let is_numeric = algorithm == "crc" or algorithm == "bsd" or algorithm == "sysv" or algorithm == "crc32b"
  if !is_numeric and algorithm != "blake2b" and algorithm != "sha2" and algorithm != "sha3" and algorithm != "blake3" and algorithm != "shake128" and algorithm != "shake256" and digest_length(algorithm) == 0 { gnu.usage_error(f"invalid argument {gnu.quote(algorithm)} for 'checksum algorithm'") }
  if !opts.check {
    for flag in [if opts.quiet { "quiet" } else { "" }, if opts.status { "status" } else { "" }, if opts.warn { "warn" } else { "" }, if opts.strict { "strict" } else { "" }, if opts.ignore_missing { "ignore-missing" } else { "" }] {
      if flag != "" { gnu.usage_error(f"the --{flag} option is meaningful only when verifying checksums") }
    }
  }
  if opts.tag and opts.text and !binary { gnu.usage_error("--tag does not support --text mode") }
  if cksum and opts.text and !binary and tagged { gnu.usage_error("--text mode is only supported with --untagged") }
  if opts.check and opts.tag { gnu.usage_error("the --tag option is meaningless when verifying checksums") }
  if opts.check and (opts.binary or opts.text) { gnu.usage_error("the --binary and --text options are meaningless when verifying checksums") }
  if opts.check and is_numeric and (!cksum or opts.algorithm != "") {
    gnu.error("--check is not supported with --algorithm={bsd,sysv,crc,crc32b}")
    exit 1
  }
  let files = if opts.files.is_empty() { ["-"] } else { opts.files }
  if opts.raw and files.len() > 1 { gnu.usage_error("the --raw option is not supported with multiple files") }
  if opts.raw and opts.base64 { gnu.usage_error("--base64 cannot be used with --raw") }
  var failed = false
  for name in files {
    if opts.check {
      let check_algorithm = if cksum and opts.algorithm == "sha2" { "sha2" } else { algorithm }
      let infer = cksum and (opts.algorithm == "" or opts.algorithm == "sha2" or opts.algorithm == "sha3")
      if !verify_list(name, opts, check_algorithm, length, infer) { failed = true }
    } else if is_numeric {
      match numeric(name, algorithm) {
        Ok(value) => {
          if opts.raw {
            let number = value.checksum
            let octets = if algorithm == "bsd" or algorithm == "sysv" { [number / 256 % 256, number % 256] } else { [number / 16777216 % 256, number / 65536 % 256, number / 256 % 256, number % 256] }
            gnu.write_bytes(bytes.from_ints(octets)?)
            continue
          }
          let suffix = if name == "-" and opts.files.is_empty() { "" } else { f" {name}" }
          let ending = if opts.zero { "\0" } else { "\n" }
          let size = if algorithm == "bsd" { (value.size + 1023) / 1024 } else if algorithm == "sysv" { (value.size + 511) / 512 } else { value.size }
          let checksum = if algorithm == "bsd" { padded(value.checksum, 5, "0") } else { f"{value.checksum}" }
          let blocks = if algorithm == "bsd" { padded(size, 5, " ") } else { f"{size}" }
          gnu.write_text(f"{checksum} {blocks}{suffix}{ending}")
        }
        Err(failure) => { gnu.name_error(name, failure); failed = true }
      }
    } else {
      match digest(name, algorithm, length) {
        Ok(value) => {
          if opts.raw { gnu.write_bytes(raw_digest(value.hex())?); continue }
          let text = if opts.base64 { value.base64() } else { value.hex() }
          let escape = !opts.zero and needs_escape(name)
          let shown = if escape { escaped(name) } else { name }
          let prefix = if escape { "\\" } else { "" }
          let ending = if opts.zero { "\0" } else { "\n" }
          let output = if tagged { f"{prefix}{label(algorithm, length)} ({shown}) = {text}{ending}" } else { f"{prefix}{text} {if binary { "*" } else { " " }}{shown}{ending}" }
          gnu.write_text(output)
        }
        Err(failure) => { gnu.name_error(name, failure); failed = true }
      }
    }
  }
  if failed { exit 1 }
}
