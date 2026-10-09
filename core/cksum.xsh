#!/bin/xsh
use lib.gnu

const MD5_USAGE = """Usage: md5sum [OPTION]... [FILE]...
Print or check MD5 (128-bit) checksums.

      --tag             create a BSD-style checksum
  -c, --check           read checksums from the FILEs and check them
      --status          don't output anything, status code shows success
      --quiet           don't print OK for each successfully verified file
      --strict          exit non-zero for improperly formatted checksum lines
  -w, --warn            warn about improperly formatted checksum lines
  -b, --binary          read in binary mode
  -t, --text            read in text mode (default)
  -z, --zero            end each output line with NUL, not newline
      --ignore-missing  don't fail or report status for missing files
      --help            display this help and exit
      --version         output version information and exit
"""
const SHA1_USAGE = """Usage: sha1sum [OPTION]... [FILE]...
Print or check SHA1 (160-bit) checksums.

  -c, --check           read checksums from the FILEs and check them
      --status          don't output anything, status code shows success
      --quiet           don't print OK for each successfully verified file
      --strict          exit non-zero for improperly formatted checksum lines
  -w, --warn            warn about improperly formatted checksum lines
  -b, --binary          read in binary mode
  -t, --text            read in text mode (default)
  -z, --zero            end each output line with NUL, not newline
      --ignore-missing  don't fail or report status for missing files
      --help            display this help and exit
      --version         output version information and exit
"""
const SHA224_USAGE = """Usage: sha224sum [OPTION]... [FILE]...
Print or check SHA224 (224-bit) checksums.

  -c, --check           read checksums from the FILEs and check them
      --status          don't output anything, status code shows success
      --quiet           don't print OK for each successfully verified file
      --strict          exit non-zero for improperly formatted checksum lines
  -w, --warn            warn about improperly formatted checksum lines
  -b, --binary          read in binary mode
  -t, --text            read in text mode (default)
  -z, --zero            end each output line with NUL, not newline
      --ignore-missing  don't fail or report status for missing files
      --help            display this help and exit
      --version         output version information and exit
"""
const SHA256_USAGE = """Usage: sha256sum [OPTION]... [FILE]...
Print or check SHA256 (256-bit) checksums.

  -c, --check           read checksums from the FILEs and check them
      --status          don't output anything, status code shows success
      --quiet           don't print OK for each successfully verified file
      --strict          exit non-zero for improperly formatted checksum lines
  -w, --warn            warn about improperly formatted checksum lines
  -b, --binary          read in binary mode
  -t, --text            read in text mode (default)
  -z, --zero            end each output line with NUL, not newline
      --ignore-missing  don't fail or report status for missing files
      --help            display this help and exit
      --version         output version information and exit
"""
const SHA384_USAGE = """Usage: sha384sum [OPTION]... [FILE]...
Print or check SHA384 (384-bit) checksums.

  -c, --check           read checksums from the FILEs and check them
      --status          don't output anything, status code shows success
      --quiet           don't print OK for each successfully verified file
      --strict          exit non-zero for improperly formatted checksum lines
  -w, --warn            warn about improperly formatted checksum lines
  -b, --binary          read in binary mode
  -t, --text            read in text mode (default)
  -z, --zero            end each output line with NUL, not newline
      --ignore-missing  don't fail or report status for missing files
      --help            display this help and exit
      --version         output version information and exit
"""
const SHA512_USAGE = """Usage: sha512sum [OPTION]... [FILE]...
Print or check SHA512 (512-bit) checksums.

  -c, --check           read checksums from the FILEs and check them
      --status          don't output anything, status code shows success
      --quiet           don't print OK for each successfully verified file
      --strict          exit non-zero for improperly formatted checksum lines
  -w, --warn            warn about improperly formatted checksum lines
  -b, --binary          read in binary mode
  -t, --text            read in text mode (default)
  -z, --zero            end each output line with NUL, not newline
      --ignore-missing  don't fail or report status for missing files
      --help            display this help and exit
      --version         output version information and exit
"""
const B2_USAGE = """Usage: b2sum [OPTION]... [FILE]...
Print or check BLAKE2b (512-bit) checksums.

  -l, --length=BITS     digest length in bits; must be a multiple of 8
  -c, --check           read checksums from the FILEs and check them
      --status          don't output anything, status code shows success
      --quiet           don't print OK for each successfully verified file
      --strict          exit non-zero for improperly formatted checksum lines
  -w, --warn            warn about improperly formatted checksum lines
  -b, --binary          read in binary mode
  -t, --text            read in text mode (default)
  -z, --zero            end each output line with NUL, not newline
      --ignore-missing  don't fail or report status for missing files
      --help            display this help and exit
      --version         output version information and exit
"""
const CKSUM_USAGE = """Usage: cksum [OPTION]... [FILE]...
Print or verify checksums.

  -a, --algorithm=TYPE  select checksum algorithm
  -l, --length=BITS     digest length in bits; supported by BLAKE2b and SHA-2
  -c, --check           read checksums from the FILEs and check them
      --tag             create a BSD-style checksum
      --untagged        create a reversed-style checksum
      --status          don't output anything, status code shows success
      --quiet           don't print OK for each successfully verified file
      --strict          exit non-zero for improperly formatted checksum lines
  -w, --warn            warn about improperly formatted checksum lines
  -b, --binary          read in binary mode
  -t, --text            read in text mode
  -z, --zero            end each output line with NUL, not newline
      --ignore-missing  don't fail or report status for missing files
      --help            display this help and exit
      --version         output version information and exit
"""
const SUM_USAGE = """Usage: sum [OPTION]... [FILE]...
Print checksum and block counts for each FILE.

  -r              use BSD sum algorithm (default)
  -s              use System V sum algorithm
      --help      display this help and exit
      --version   output version information and exit
"""

type SumOptions = {
  check: Bool,
  status: Bool,
  quiet: Bool,
  strict: Bool,
  warn: Bool,
  ignore_missing: Bool,
  binary: Bool,
  text: Bool,
  zero: Bool,
  tag: Bool,
  untagged: Bool,
  base64: Bool,
  raw: Bool,
  algorithm: Str?,
  length: Str?,
  bsd: Bool,
  sysv: Bool,
  help: Bool,
  version: Bool,
  files: List[Str],
}

type ChecksumValue = {
  algorithm: Str,
  digest: Str,
  base64: Str,
  number: Int,
  size: Int,
}

type ChecksumLine = {valid: Bool, path: Str, alternate_path: Str?, raw_path: Bytes?, digest: Str, algorithm: Str, length: Int, base64: Bool}
type ChecksumInputLine = {text: Str, raw: Bytes}

pure usage_for(program: Str) -> Str {
  return MD5_USAGE when program == "md5sum"
  return SHA1_USAGE when program == "sha1sum"
  return SHA224_USAGE when program == "sha224sum"
  return SHA256_USAGE when program == "sha256sum"
  return SHA384_USAGE when program == "sha384sum"
  return SHA512_USAGE when program == "sha512sum"
  return B2_USAGE when program == "b2sum"
  return SUM_USAGE when program == "sum"
  CKSUM_USAGE
}

pure default_algorithm(program: Str) -> Str {
  return "md5" when program == "md5sum"
  return "sha1" when program == "sha1sum"
  return "sha224" when program == "sha224sum"
  return "sha256" when program == "sha256sum"
  return "sha384" when program == "sha384sum"
  return "sha512" when program == "sha512sum"
  return "blake2b" when program == "b2sum"
  return "bsd" when program == "sum"
  "crc"
}

pure digest_label(algorithm: Str, length: Int) -> Str {
  return "MD5" when algorithm == "md5"
  return "SHA1" when algorithm == "sha1"
  return "SHA224" when algorithm == "sha224"
  return "SHA256" when algorithm == "sha256"
  return "SHA384" when algorithm == "sha384"
  return "SHA512" when algorithm == "sha512"
  if algorithm == "blake2b" {
    return "BLAKE2b" when length == 512
    return f"BLAKE2b-{length}"
  }
  ""
}

pure digest_hex_length(algorithm: Str, length: Int) -> Int {
  return 32 when algorithm == "md5"
  return 40 when algorithm == "sha1"
  return 56 when algorithm == "sha224"
  return 64 when algorithm == "sha256"
  return 96 when algorithm == "sha384"
  return 128 when algorithm == "sha512"
  return length / 4 when algorithm == "blake2b"
  0
}

pure digest_base64_length(algorithm: Str, length: Int) -> Int {
  let hex_length = digest_hex_length(algorithm, length)
  let bytes_count = hex_length / 2
  let blocks = (bytes_count + 2) / 3
  blocks * 4
}

pure is_hex_digest(value: Str) -> Bool {
  if value == "" or value.byte_len() % 2 != 0 { return false }
  for digit in value.lower() {
    if digit not in "0123456789abcdef" { return false }
  }
  true
}

pure base64_decoded_length(value: Str) -> Int {
  if value == "" or value.byte_len() % 4 != 0 { return -1 }
  var padding = 0
  if value.ends_with("==") { padding = 2 } else if value.ends_with("=") { padding = 1 }
  var at = 0
  while at < value.byte_len() - padding {
    let byte = value.byte_at(at) ?? -1
    let alphabet = (byte >= 65 and byte <= 90) or (byte >= 97 and byte <= 122) or (byte >= 48 and byte <= 57) or byte == 43 or byte == 47
    if ! alphabet { return -1 }
    at += 1
  }
  at = value.byte_len() - padding
  while at < value.byte_len() {
    if value.byte_at(at) != 61 { return -1 }
    at += 1
  }
  value.byte_len() / 4 * 3 - padding
}

# Preserve the structure around invalid UTF-8 bytes so checksum lines with
# non-UTF-8 filenames remain parseable. The original path bytes are retained
# separately for filesystem access and GNU quoting.
pure checksum_lossy(data: Bytes) -> Str {
  if let Ok(text) = data.utf8() { return text }
  var out = ""
  var at = 0
  let total = data.len()
  while at < total {
    let lead = data.byte_at(at) ?? 0
    let need = if lead < 128 { 1 } else if lead >= 194 and lead <= 223 { 2 } else if lead >= 224 and lead <= 239 { 3 } else if lead >= 240 and lead <= 244 { 4 } else { 0 }
    if need > 0 and at + need <= total {
      if let Ok(piece) = data[at..at + need].utf8() {
        out = f"{out}{piece}"
        at += need
        continue
      }
    }
    out = f"{out}\u{fffd}"
    at += 1
  }
  out
}

pure checksum_lines(data: Bytes, delimiter: Int) -> List[ChecksumInputLine] {
  var lines: List[ChecksumInputLine] = []
  var current: List[Int] = []
  for byte in data {
    if byte == delimiter {
      let raw = bytes.from_ints(current) ?? b""
      let line = if current.len() > 0 and current[0] == 35 { "#" } else { checksum_lossy(bytes.from_ints(current) ?? b"") }
      lines += [{text: line, raw: raw}]
      current = []
    } else {
      current += [byte]
    }
  }
  if current.len() > 0 {
    let raw = bytes.from_ints(current) ?? b""
    let line = if current[0] == 35 { "#" } else { checksum_lossy(bytes.from_ints(current) ?? b"") }
    lines += [{text: line, raw: raw}]
  }
  lines
}

pure bsd_padded_number(value: Int) -> Str {
  let raw = f"{value}"
  if value < 10 { return f"0000{raw}" }
  if value < 100 { return f"000{raw}" }
  if value < 1000 { return f"00{raw}" }
  if value < 10000 { return f"0{raw}" }
  raw
}

pure line_end(zero: Bool) -> Str {
  if zero { "\0" } else { "\n" }
}

pure checksum_name(name: Str) -> Str {
  if "\n" in name or "\\" in name or "\r" in name {
    return f"\\{name.replace("\\", "\\\\").replace("\n", "\\n").replace("\r", "\\r")}"
  }
  name
}

proc digest_bytes(data: Bytes, algorithm: Str, length: Int) [error] -> Result[ChecksumValue] {
  if algorithm == "md5" {
    let digest = hash.md5(data)
    return Ok({algorithm: algorithm, digest: digest.hex(), base64: digest.base64(), number: 0, size: data.len()})
  }
  if algorithm == "sha1" {
    let digest = hash.sha1(data)
    return Ok({algorithm: algorithm, digest: digest.hex(), base64: digest.base64(), number: 0, size: data.len()})
  }
  if algorithm == "sha224" {
    let digest = hash.sha224(data)
    return Ok({algorithm: algorithm, digest: digest.hex(), base64: digest.base64(), number: 0, size: data.len()})
  }
  if algorithm == "sha256" {
    let digest = hash.sha256(data)
    return Ok({algorithm: algorithm, digest: digest.hex(), base64: digest.base64(), number: 0, size: data.len()})
  }
  if algorithm == "sha384" {
    let digest = hash.sha384(data)
    return Ok({algorithm: algorithm, digest: digest.hex(), base64: digest.base64(), number: 0, size: data.len()})
  }
  if algorithm == "sha512" {
    let digest = hash.sha512(data)
    return Ok({algorithm: algorithm, digest: digest.hex(), base64: digest.base64(), number: 0, size: data.len()})
  }
  if algorithm == "blake2b" {
    let digest = hash.blake2b(data, output_length: length / 8)?
    return Ok({algorithm: algorithm, digest: digest.hex(), base64: digest.base64(), number: 0, size: data.len()})
  }
  if algorithm == "crc" {
    let value = hash.cksum(data)
    return Ok({algorithm: algorithm, digest: "", base64: "", number: value.checksum, size: value.bytes})
  }
  if algorithm == "bsd" {
    let value = hash.bsd_sum(data)
    return Ok({algorithm: algorithm, digest: "", base64: "", number: value.checksum, size: value.blocks})
  }
  if algorithm == "sysv" {
    let value = hash.sysv_sum(data)
    return Ok({algorithm: algorithm, digest: "", base64: "", number: value.checksum, size: (data.len() + 511) / 512})
  }
  if algorithm == "crc32b" {
    return Ok({algorithm: algorithm, digest: "", base64: "", number: hash.crc32(data), size: data.len()})
  }
  let digest = hash.md5(data)
  Ok({algorithm: "md5", digest: digest.hex(), base64: digest.base64(), number: 0, size: data.len()})
}

proc digest_path(path_value: Path, algorithm: Str, length: Int) [fs, error] -> Result[ChecksumValue] {
  if algorithm == "md5" {
    match hash.md5(path_value) { Ok(digest) => Ok({algorithm: algorithm, digest: digest.hex(), base64: digest.base64(), number: 0, size: 0}); Err(failure) => Err(failure) }
  } else if algorithm == "sha1" {
    match hash.sha1(path_value) { Ok(digest) => Ok({algorithm: algorithm, digest: digest.hex(), base64: digest.base64(), number: 0, size: 0}); Err(failure) => Err(failure) }
  } else if algorithm == "sha224" {
    match hash.sha224(path_value) { Ok(digest) => Ok({algorithm: algorithm, digest: digest.hex(), base64: digest.base64(), number: 0, size: 0}); Err(failure) => Err(failure) }
  } else if algorithm == "sha256" {
    match hash.sha256(path_value) { Ok(digest) => Ok({algorithm: algorithm, digest: digest.hex(), base64: digest.base64(), number: 0, size: 0}); Err(failure) => Err(failure) }
  } else if algorithm == "sha384" {
    match hash.sha384(path_value) { Ok(digest) => Ok({algorithm: algorithm, digest: digest.hex(), base64: digest.base64(), number: 0, size: 0}); Err(failure) => Err(failure) }
  } else if algorithm == "sha512" {
    match hash.sha512(path_value) { Ok(digest) => Ok({algorithm: algorithm, digest: digest.hex(), base64: digest.base64(), number: 0, size: 0}); Err(failure) => Err(failure) }
  } else if algorithm == "blake2b" {
    match hash.blake2b(path_value, output_length: length / 8) { Ok(digest) => Ok({algorithm: algorithm, digest: digest.hex(), base64: digest.base64(), number: 0, size: 0}); Err(failure) => Err(failure) }
  } else if algorithm == "crc" {
    match hash.cksum(path_value) { Ok(value) => Ok({algorithm: algorithm, digest: "", base64: "", number: value.checksum, size: value.bytes}); Err(failure) => Err(failure) }
  } else if algorithm == "bsd" {
    match hash.bsd_sum(path_value) { Ok(value) => Ok({algorithm: algorithm, digest: "", base64: "", number: value.checksum, size: value.blocks}); Err(failure) => Err(failure) }
  } else if algorithm == "sysv" {
    match hash.sysv_sum(path_value) { Ok(value) => Ok({algorithm: algorithm, digest: "", base64: "", number: value.checksum, size: (fs.stat(path_value)?.size + 511) / 512}); Err(failure) => Err(failure) }
  } else if algorithm == "crc32b" {
    match path_value.read_bytes() { Ok(data) => digest_bytes(data, algorithm, length); Err(failure) => Err(failure) }
  } else {
    match hash.md5(path_value) { Ok(digest) => Ok({algorithm: "md5", digest: digest.hex(), base64: digest.base64(), number: 0, size: 0}); Err(failure) => Err(failure) }
  }
}

proc checksum_value(name: Str, algorithm: Str, length: Int, raw_path: Bytes?) [fs, error, io] -> Result[ChecksumValue] {
  if name == "-" {
    let data = io.stdin_bytes()?
    return digest_bytes(data, algorithm, length)
  }
  let path_value = if raw_path != null { Path.parse_bytes(raw_path ?? b"")? } else { fp"{name}" }
  digest_path(path_value, algorithm, length)
}

pure checksum_line(name: Str, value: ChecksumValue, program: Str, tagged: Bool, binary: Bool, zero: Bool, length: Int, base64: Bool) -> Str {
  let end = line_end(zero)
  let label = digest_label(value.algorithm, length)
  let shown_name = if zero { name } else { checksum_name(name) }
  let escaped = shown_name != name and ! zero
  let prefix = if escaped { "\\" } else { "" }
  if value.algorithm == "crc" or value.algorithm == "crc32b" {
    let file = if name == "-" { "" } else { f" {shown_name}" }
    return f"{value.number} {value.size}{file}{end}"
  }
  if value.algorithm == "bsd" or value.algorithm == "sysv" {
    let number = f"{value.number}"
    let count = f"{value.size}"
    let padded_number = if value.algorithm == "bsd" { bsd_padded_number(value.number) } else { number }
    let shown_count = if value.algorithm == "bsd" and value.size < 10 { f"    {count}" } else if value.algorithm == "bsd" and value.size < 100 { f"   {count}" } else if value.algorithm == "bsd" and value.size < 1000 { f"  {count}" } else if value.algorithm == "bsd" { f" {count}" } else { count }
    let file = if name == "-" { "" } else { f" {shown_name}" }
    return f"{padded_number} {shown_count}{file}{end}"
  }
  if tagged {
    let digest = if base64 { value.base64 } else { value.digest }
    return f"{prefix}{label} ({shown_name}) = {digest}{end}"
  }
  let marker = if binary { " *" } else { "  " }
  let digest = if base64 { value.base64 } else { value.digest }
  f"{prefix}{digest}{marker}{shown_name}{end}"
}

pure option_was_last(argv: List[Str], first: Str, second: Str, default: Bool) -> Bool {
  var found: Bool? = null
  for item in argv {
    if item == first { found = true }
    if item == second { found = false }
  }
  found ?? default
}

pure binary_mode(argv: List[Str], fallback: Bool) -> Bool {
  var found: Bool? = null
  var options = true
  for item in argv {
    if options and item == "--" { options = false; continue }
    if options and (item == "-b" or item == "--binary") { found = true }
    if options and (item == "-t" or item == "--text") { found = false }
  }
  found ?? fallback
}

pure tagged_mode(argv: List[Str], fallback: Bool) -> Bool {
  var found: Bool? = null
  var options = true
  for item in argv {
    if options and item == "--" { options = false; continue }
    if options and item == "--tag" { found = true }
    if options and item == "--untagged" { found = false }
  }
  found ?? fallback
}

pure sum_mode(argv: List[Str], fallback: Str) -> Str {
  var found = fallback
  var options = true
  for item in argv {
    if options and item == "--" { options = false; continue }
    if options and item == "-r" { found = "bsd" }
    if options and item == "-s" { found = "sysv" }
  }
  found
}

pure raw_sum_names(argv: List[Str], raw: List[Bytes]) -> List[Bytes] {
  var names: List[Bytes] = []
  var options = true
  for index in range(argv.len()) {
    let argument = argv[index]
    if options and argument == "--" { options = false; continue }
    if options and (argument == "-r" or argument == "-s") { continue }
    if options and argument.starts_with("-") and argument != "-" { continue }
    names += [raw[index]]
  }
  names
}

pure unescape_checksum_path(raw: Str) -> Str {
  if ! raw.starts_with("\\") { return raw }
  let text = raw.byte_slice(1)
  var out = ""
  var at = 0
  while at < text.byte_len() {
    let byte = text.byte_slice(at, length: 1)
    if byte == "\\" and at + 1 < text.byte_len() {
      let next = text.byte_slice(at + 1, length: 1)
      if next == "n" { out = f"{out}\n"; at += 2; continue }
      if next == "r" { out = f"{out}\r"; at += 2; continue }
      if next == "\\" { out = f"{out}\\"; at += 2; continue }
    }
    out = f"{out}{byte}"
    at += 1
  }
  out
}

pure trim_leading_space(raw: Str) -> Str {
  var at = 0
  while at < raw.byte_len() and raw.byte_at(at) == 32 { at += 1 }
  raw.byte_slice(at)
}

pure last_find_bytes(haystack: Bytes, pattern: Bytes) -> Int? {
  if pattern.len() == 0 or pattern.len() > haystack.len() { return null }
  var found: Int? = null
  var at = 0
  while at + pattern.len() <= haystack.len() {
    var same = true
    var offset = 0
    while offset < pattern.len() {
      if haystack.byte_at(at + offset) != pattern.byte_at(offset) { same = false }
      offset += 1
    }
    if same { found = at; at += pattern.len() } else { at += 1 }
  }
  found
}

pure parse_checksum_line(line: Str, raw_line: Bytes, algorithm: Str, length: Int, program: Str) -> ChecksumLine {
  let initial = if program == "cksum" { trim_leading_space(line) } else { line }
  let raw_trim = if program == "cksum" { line.byte_len() - initial.byte_len() } else { 0 }
  let raw_initial_with_prefix = raw_line.slice(raw_trim)
  let raw_initial = if initial.starts_with("\\") { raw_initial_with_prefix.slice(1) } else { raw_initial_with_prefix }
  let text = if initial.starts_with("\\") { initial.byte_slice(1) } else { initial }
  let end_spaced = last_find_bytes(raw_initial, b") = ")
  let end_compact = last_find_bytes(raw_initial, b")= ")
  let open_spaced = text.find(" (")
  let open_compact = text.find("(")
  if (end_spaced != null or end_compact != null) and (open_spaced != null or open_compact != null) {
    let start = open_spaced ?? open_compact ?? 0
    let open_width = if open_spaced != null { 2 } else { 1 }
    let finish = end_spaced ?? end_compact ?? 0
    let end_width = if end_spaced != null { 4 } else { 3 }
    let label = text.byte_slice(0, length: start)
    var line_algorithm = algorithm
    var line_length = length
    var recognized = false
    if label == "MD5" { line_algorithm = "md5"; recognized = true }
    if label == "SHA1" { line_algorithm = "sha1"; recognized = true }
    if label == "SHA224" { line_algorithm = "sha224"; recognized = true }
    if label == "SHA256" { line_algorithm = "sha256"; recognized = true }
    if label == "SHA384" { line_algorithm = "sha384"; recognized = true }
    if label == "SHA512" { line_algorithm = "sha512"; recognized = true }
    if label == "SHA2-224" { line_algorithm = "sha224"; recognized = true }
    if label == "SHA2-256" { line_algorithm = "sha256"; recognized = true }
    if label == "SHA2-384" { line_algorithm = "sha384"; recognized = true }
    if label == "SHA2-512" { line_algorithm = "sha512"; recognized = true }
    if label == "BLAKE2b" { line_algorithm = "blake2b"; line_length = 512; recognized = true }
    if label.starts_with("BLAKE2b-") {
      line_algorithm = "blake2b"
      line_length = label.byte_slice(8).parse_int() ?? -1
      recognized = true
    }
    if line_algorithm == "blake2b" and (line_length < 8 or line_length > 512 or line_length % 8 != 0) {
      recognized = false
    }
    if recognized and (algorithm == "crc" or line_algorithm == algorithm or (algorithm == "sha2" and line_algorithm.starts_with("sha"))) and line_length >= 0 {
      let filename_bytes = raw_initial.slice(start + open_width, length: finish - start - open_width)
      let filename = filename_bytes.utf8() ?? "\u{fffd}"
      let digest = raw_initial.slice(finish + end_width).utf8() ?? ""
      let unescaped_filename = unescape_checksum_path(filename)
      let path_bytes = if unescaped_filename != filename { bytes.from_text(unescaped_filename) } else { filename_bytes }
      let hex_length = digest_hex_length(line_algorithm, line_length)
      let digest_is_base64 = digest.byte_len() != hex_length
      let correct_size = if digest_is_base64 { digest.byte_len() == digest_base64_length(line_algorithm, line_length) } else { true }
      let valid_hex = if digest_is_base64 or digest.byte_len() != hex_length { false } else { match hash.parse_check_line(f"{digest}  checksum-validation-target") { Ok(_) => true; Err(_) => false } }
      let valid_encoding = if digest_is_base64 { correct_size } else { valid_hex }
      return {valid: digest != "" and filename != "" and valid_encoding, path: unescape_checksum_path(filename), alternate_path: null, raw_path: path_bytes, digest: if digest_is_base64 { digest } else { digest.lower() }, algorithm: line_algorithm, length: line_length, base64: digest_is_base64}
    }
  }

  if algorithm == "crc" {
    let first = text.find(" ")
    if first == null { return {valid: false, path: "", alternate_path: null, raw_path: null, digest: "", algorithm: "crc", length: 0, base64: false} }
    let number = text.byte_slice(0, length: first ?? 0)
    var second_start = first ?? 0
    if second_start < text.byte_len() and text.byte_slice(second_start, length: 1) == " " { second_start += 1 }
    let second = text.byte_slice(second_start).find(" ")
    if second == null { return {valid: false, path: "", alternate_path: null, raw_path: null, digest: "", algorithm: "crc", length: 0, base64: false} }
    let size = text.byte_slice(second_start, length: second ?? 0)
    var path_start = second_start + (second ?? 0)
    let filename_bytes = raw_initial.slice(path_start)
    let encoded_filename = filename_bytes.utf8() ?? "\u{fffd}"
    let parsed_filename = if path_start == text.byte_len() { "-" } else { unescape_checksum_path(encoded_filename) }
    let parsed_bytes = if parsed_filename != encoded_filename { bytes.from_text(parsed_filename) } else { filename_bytes }
    let number_value: Int? = match number.parse_int() { Ok(value) => value; Err(_) => null }
    let size_value: Int? = match size.parse_int() { Ok(value) => value; Err(_) => null }
    if number_value == null or size_value == null { return {valid: false, path: "", alternate_path: null, raw_path: null, digest: "", algorithm: "crc", length: 0, base64: false} }
    return {valid: true, path: parsed_filename, alternate_path: null, raw_path: parsed_bytes, digest: f"{number} {size}", algorithm: "crc", length: 0, base64: false}
  }

  let separator = text.find(" ")
  if separator == null { return {valid: false, path: "", alternate_path: null, raw_path: null, digest: "", algorithm: algorithm, length: length, base64: false} }
  let digest = text.byte_slice(0, length: separator ?? 0)
  let first_space = (separator ?? 0) + 1
  if first_space >= text.byte_len() { return {valid: false, path: "", alternate_path: null, raw_path: null, digest: "", algorithm: algorithm, length: length, base64: false} }
  let two_spaces = text.byte_slice(first_space, length: 1) == " "
  let path_start = if two_spaces { first_space + 1 } else { first_space }
  if path_start >= text.byte_len() { return {valid: false, path: "", alternate_path: null, raw_path: null, digest: "", algorithm: algorithm, length: length, base64: false} }
  var filename_bytes = raw_initial.slice(path_start)
  if ! two_spaces and filename_bytes.byte_at(0) == 42 { filename_bytes = filename_bytes.slice(1) }
  var filename = filename_bytes.utf8() ?? "\u{fffd}"
  if ! two_spaces and filename.starts_with("*") { filename = filename.byte_slice(1) }
  let unescaped_filename = unescape_checksum_path(filename)
  if unescaped_filename != filename { filename_bytes = bytes.from_text(unescaped_filename); filename = unescaped_filename }
  return {valid: false, path: "", alternate_path: null, raw_path: null, digest: "", algorithm: algorithm, length: length, base64: false} when filename == ""
  var line_algorithm = algorithm
  var line_length = length
  let digest_is_hex = is_hex_digest(digest)
  let digest_bytes = if digest_is_hex { digest.byte_len() / 2 } else { base64_decoded_length(digest) }
  let digest_is_base64 = ! digest_is_hex
  if (algorithm == "blake2b" or program == "b2sum") and digest_bytes >= 1 and digest_bytes <= 64 {
    line_length = digest_bytes * 8
  }
  if algorithm == "sha2" {
    if digest_bytes == 28 { line_algorithm = "sha224"; line_length = 224 }
    if digest_bytes == 32 { line_algorithm = "sha256"; line_length = 256 }
    if digest_bytes == 48 { line_algorithm = "sha384"; line_length = 384 }
    if digest_bytes == 64 { line_algorithm = "sha512"; line_length = 512 }
  }
  let expected_bytes = digest_hex_length(line_algorithm, line_length) / 2
  let correct_size = digest_bytes == expected_bytes
  if digest_is_base64 {
    return {valid: correct_size, path: unescaped_filename, alternate_path: if two_spaces { f" {unescaped_filename}" } else { null }, raw_path: filename_bytes, digest: digest, algorithm: line_algorithm, length: line_length, base64: true}
  }
  return {valid: false, path: "", alternate_path: null, raw_path: null, digest: "", algorithm: line_algorithm, length: line_length, base64: false} when ! correct_size
  match hash.parse_check_line(f"{digest}  checksum-validation-target") {
    Ok(parsed) => {valid: true, path: unescaped_filename, alternate_path: if two_spaces { f" {unescaped_filename}" } else { null }, raw_path: filename_bytes, digest: parsed.hex, algorithm: line_algorithm, length: line_length, base64: false}
    Err(_) => {valid: false, path: "", alternate_path: null, raw_path: null, digest: "", algorithm: line_algorithm, length: line_length, base64: false}
  }
}

proc reject_if(condition: Bool, option: Str) [process, env] {
  if condition {
    gnu.usage_error(f"option '{option}' is not supported")
  }
}

proc parse_length(raw: Str?, program: Str, algorithm: Str) [process, env] -> Int {
  if raw == null { return 512 when program == "b2sum" or algorithm == "blake2b"; return 256 when algorithm == "sha2"; return 0 }
  let text = raw ?? ""
  let normalized = if text.starts_with("=") { text.byte_slice(1) } else { text }
  let bits = normalized.parse_int() ?? -1
  if bits == 0 and (program == "b2sum" or algorithm == "blake2b") { return 512 }
  if bits == 0 { return 0 }
  var decimal = normalized != ""
  for digit in normalized {
    if digit < "0" or digit > "9" { decimal = false }
  }
  let too_large = bits > 512 or (bits == -1 and decimal and normalized.byte_len() > 3)
  if bits < 8 or too_large or bits % 8 != 0 {
    gnu.error(f"invalid length: {gnu.quote_value(normalized)}")
    if ! too_large and bits % 8 != 0 { gnu.error("length is not a multiple of 8") }
    if too_large { gnu.error("maximum digest length for 'BLAKE2b' is 512 bits") }
    exit 1
  }
  bits
}

proc check_file(name: Str, algorithm: Str, opts: SumOptions, length: Int, program: Str) [fs, process, env, error, io] -> Bool {
  let data_result: Result[Bytes, Error] = if name == "-" { io.stdin_bytes() } else { fp"{name}".read_bytes() }
  let data = match data_result {
    Ok(contents) => contents
    Err(failure) => {
      if ! opts.status and ! (opts.ignore_missing and gnu.errno(failure) == 2) {
        let reason = if gnu.errno(failure) == 5 { f"read error: {gnu.strerror(failure)}" } else { gnu.strerror(failure) }
        gnu.error(f"{name}: {reason}")
      }
      return false when opts.ignore_missing and gnu.errno(failure) == 2
      return true
    }
  }
  let delimiter_byte = if opts.zero { 0 } else { 10 }
  var failed = false
  var malformed = 0
  var valid_lines = 0
  var verified = 0
  var missing = 0
  var mismatches = 0
  var line_number = 0
  for input_line in checksum_lines(data, delimiter_byte) {
    let line = input_line.text
    line_number += 1
    if line == "" { continue }
    if line.starts_with("#") { continue }
    let parsed = parse_checksum_line(line, input_line.raw, algorithm, length, program)
    if ! parsed.valid {
        malformed += 1
        if opts.warn and ! opts.status {
          let label = if program == "cksum" { "cksum" } else { digest_label(default_algorithm(program), length) }
          gnu.error(f"{name}: {line_number}: improperly formatted {label} checksum line")
        }
    } else {
      valid_lines += 1
      var target = parsed.path
      var target_bytes = parsed.raw_path ?? bytes.from_text(target)
      var target_path = Path.parse_bytes(target_bytes)?
      let check_algorithm = parsed.algorithm
      let check_length = parsed.length
      if target != "-" {
        if let Ok(metadata) = fs.stat(target_path) {
          if metadata.kind == "dir" {
            failed = true
            missing += 1
            if ! opts.status {
              let shown = gnu.quote_bytes(target_bytes, always: false)
              gnu.write_text(f"{shown}: FAILED open or read\n")
              gnu.error(f"{shown}: Is a directory")
            }
            continue
          }
        }
      }
      var result: Result[ChecksumValue, Error] = if target == "-" {
        match io.stdin_bytes() {
          Ok(stdin_data) => digest_bytes(stdin_data, check_algorithm, check_length)
          Err(failure) => Err(failure)
        }
      } else {
        digest_path(target_path, check_algorithm, check_length)
      }
      if target != "-" and parsed.alternate_path != null {
        let alternate = parsed.alternate_path ?? target
        if let Ok(_) = fs.stat(fp"{alternate}") {
          target = alternate
          target_bytes = bytes.from_text(target)
          target_path = fp"{target}"
          result = digest_path(target_path, check_algorithm, check_length)
        }
      }
      let shown_target = gnu.quote_bytes(target_bytes, always: false)
      match result {
        Err(failure) => {
          if opts.ignore_missing and gnu.errno(failure) == 2 { continue }
          failed = true
          missing += 1
          if ! opts.status {
            gnu.write_text(f"{shown_target}: FAILED open or read\n")
            let reason = if gnu.errno(failure) == 5 { f"read error: {gnu.strerror(failure)}" } else { gnu.strerror(failure) }
            gnu.error(f"{shown_target}: {reason}")
          }
        }
        Ok(value) => {
          verified += 1
          let actual = if check_algorithm == "crc" { f"{value.number} {value.size}" } else if parsed.base64 { value.base64 } else { value.digest.lower() }
          let expected = if parsed.base64 { parsed.digest } else { parsed.digest.lower() }
          if actual == expected {
            if ! opts.status and ! opts.quiet { gnu.write_text(f"{shown_target}: OK\n") }
          } else {
            failed = true
            mismatches += 1
            if ! opts.status { gnu.write_text(f"{shown_target}: FAILED\n") }
          }
        }
      }
    }
  }
  if valid_lines == 0 {
    let shown = if name == "-" { "'standard input'" } else { gnu.quote_maybe(name) }
    gnu.error(f"{shown}: no properly formatted checksum lines found")
    return true
  }
  if malformed > 0 {
    let line_word = if malformed == 1 { "line" } else { "lines" }
    let verb = if malformed == 1 { "is" } else { "are" }
    if ! opts.status { gnu.error(f"WARNING: {malformed} {line_word} {verb} improperly formatted") }
    failed = failed or opts.strict
  }
  if opts.ignore_missing and verified == 0 {
    let shown = if name == "-" { "'standard input'" } else { gnu.quote_maybe(name) }
    gnu.error(f"{shown}: no file was verified")
    failed = true
  }
  if missing > 0 and ! opts.status {
    let file_word = if missing == 1 { "file" } else { "files" }
    gnu.error(f"WARNING: {missing} listed {file_word} could not be read")
  }
  if mismatches > 0 and ! opts.status {
    let checksum_word = if mismatches == 1 { "checksum" } else { "checksums" }
    gnu.error(f"WARNING: {mismatches} computed {checksum_word} did NOT match")
  }
  failed
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let program = gnu.prog()
  let opts: SumOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      check: {form: "-c --check", default: false},
      status: {form: "--status", default: false},
      quiet: {form: "--quiet", default: false},
      strict: {form: "--strict", default: false},
      warn: {form: "-w --warn", default: false},
      ignore_missing: {form: "--ignore-missing", default: false},
      binary: {form: "-b --binary", default: false},
      text: {form: "-t --text", default: false},
      zero: {form: "-z --zero", default: false},
      tag: {form: "--tag", default: false},
      untagged: {form: "--untagged", default: false},
      base64: {form: "--base64", default: false},
      raw: {form: "--raw", default: false},
      algorithm: {form: "-a --algorithm=TYPE --algo=TYPE"},
      length: {form: "-l --length=BITS"},
      bsd: {form: "-r", default: false},
      sysv: {form: "-s", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      files: {form: "...FILE"},
    },
  )?

  if opts.help { gnu.help(usage_for(program)); return }
  if opts.version { gnu.version(program); return }

  let is_sum = program == "sum"
  let is_cksum = program == "cksum"
  let is_b2sum = program == "b2sum"
  reject_if(is_sum and (opts.check or opts.status or opts.quiet or opts.strict or opts.warn or opts.ignore_missing or opts.binary or opts.text or opts.zero or opts.tag or opts.untagged or opts.algorithm != null or opts.length != null), "checksum options")
  reject_if(! is_sum and (opts.bsd or opts.sysv), "-r/-s")
  reject_if(! is_cksum and opts.untagged, "--untagged")
  reject_if(! is_cksum and opts.base64, "--base64")
  reject_if(! is_cksum and opts.raw, "--raw")
  reject_if(! is_cksum and opts.algorithm != null, "--algorithm")
  reject_if(! (is_cksum or is_b2sum) and opts.length != null, "--length")
  let binary_option = binary_mode(argv, opts.binary)
  let text_option = opts.text and ! binary_option
  if opts.tag and opts.check { gnu.usage_error("the --tag option is meaningless when verifying checksums") }
  if opts.check and (opts.binary or opts.text) { gnu.usage_error("the --binary and --text options are meaningless when verifying checksums") }
  if opts.tag and text_option { gnu.usage_error("the --tag option is not supported with --text") }
  if opts.status and ! opts.check { gnu.usage_error("the --status option is meaningful only when verifying checksums") }
  if opts.quiet and ! opts.check { gnu.usage_error("the --quiet option is meaningful only when verifying checksums") }
  if opts.strict and ! opts.check { gnu.usage_error("the --strict option is meaningful only when verifying checksums") }
  if opts.warn and ! opts.check { gnu.usage_error("the --warn option is meaningful only when verifying checksums") }
  if opts.ignore_missing and ! opts.check { gnu.usage_error("the --ignore-missing option is meaningful only when verifying checksums") }
  if opts.base64 and opts.raw { gnu.usage_error("the --base64 option cannot be used with --raw") }
  if opts.raw and opts.check { gnu.usage_error("the --raw option is not supported with --check") }

  if is_cksum and text_option and ! opts.untagged {
    gnu.usage_error("--text mode is only supported with --untagged")
  }

  let raw_algorithm = opts.algorithm ?? "crc"
  let normalized_algorithm = if raw_algorithm.starts_with("=") { raw_algorithm.byte_slice(1) } else { raw_algorithm }
  var algorithm = if is_sum { sum_mode(argv, "bsd") } else if is_cksum { normalized_algorithm } else { default_algorithm(program) }
  let raw_length = if (opts.length ?? "").starts_with("=") { (opts.length ?? "").byte_slice(1) } else { opts.length ?? "" }
  if opts.length != null and raw_length != "0" and algorithm != "blake2b" and algorithm != "sha2" and algorithm != "sha3" {
    gnu.usage_error("--length is only supported with --algorithm blake2b, sha2, or sha3")
  }
  if opts.length != null and (algorithm == "sha2" or algorithm == "sha3") {
    let parsed_length: Int? = match raw_length.parse_int() { Ok(value) => value; Err(_) => null }
    let valid_length = parsed_length == 224 or parsed_length == 256 or parsed_length == 384 or parsed_length == 512
    if ! valid_length {
      gnu.error(f"invalid length: {gnu.quote_value(raw_length)}")
      let label = if algorithm == "sha2" { "SHA2" } else { "SHA3" }
      gnu.error(f"digest length for '{label}' must be 224, 256, 384, or 512")
      exit 1
    }
  }
  let length = parse_length(opts.length, program, algorithm)
  if algorithm == "sha2" or algorithm == "sha3" {
    if opts.length == null and ! opts.check { gnu.usage_error(f"--algorithm={algorithm} requires specifying --length 224, 256, 384, or 512") }
    if algorithm == "sha2" and opts.length != null { algorithm = f"sha{length}" }
  }
  let supported = (algorithm == "sha2" and opts.check) or algorithm == "crc" or algorithm == "crc32b" or algorithm == "bsd" or algorithm == "sysv" or algorithm == "md5" or algorithm == "sha1" or algorithm == "sha224" or algorithm == "sha256" or algorithm == "sha384" or algorithm == "sha512" or algorithm == "blake2b"
  if ! supported and ! opts.check and (algorithm == "blake3" or algorithm == "sm3") {
    var all_directories = opts.files.len() > 0
    for name in opts.files {
      if let Ok(metadata) = fs.stat(fp"{name}") {
        if metadata.kind != "dir" { all_directories = false }
      } else {
        all_directories = false
      }
    }
    if all_directories {
      for name in opts.files {
        gnu.error(f"{gnu.quote_maybe(name)}: Is a directory")
      }
      exit 1
    }
  }
  if ! supported and ! (opts.check and algorithm == "sm3") {
    gnu.usage_error(f"unsupported algorithm {gnu.quote_value(algorithm)}")
  }
  if opts.length != null and length != 0 and algorithm != "blake2b" and ! (opts.algorithm == "sha2") {
    gnu.usage_error("--length is only supported with --algorithm blake2b, sha2, or sha3")
  }
  if is_cksum and text_option and algorithm != "crc" and algorithm != "bsd" and algorithm != "sysv" and tagged_mode(argv, true) {
    gnu.usage_error("--text is not supported with tagged output")
  }
  if opts.check and opts.algorithm != null and (algorithm == "crc" or algorithm == "crc32b" or algorithm == "bsd" or algorithm == "sysv") {
    gnu.error("--check is not supported with --algorithm={bsd,sysv,crc,crc32b}")
    exit 1
  }

  if opts.check {
    let check_files = if opts.files.len() == 0 { ["-"] } else { opts.files }
    var failed = false
    for name in check_files {
      failed = check_file(name, algorithm, opts, length, program) or failed
    }
    if failed { exit 1 }
    return
  }

  let names = if opts.files.len() == 0 { ["-"] } else { opts.files }
  if opts.raw and names.len() > 1 { gnu.usage_error("the --raw option is not supported with multiple files") }
  let raw_arguments = cli.argv_bytes()
  let provided_names = if is_sum { raw_sum_names(argv, raw_arguments) } else { [bytes.from_text(name) for name in names] }
  let raw_names = if opts.files.len() == 0 { [b"-"] } else { provided_names }
  var failed = false
  for index in range(names.len()) {
    let name = names[index]
    let raw_name = raw_names[index]
    let result = checksum_value(name, algorithm, length, if is_sum { raw_name } else { null })
    match result {
      Ok(value) => {
        let tagged_default = is_cksum and algorithm != "crc" and algorithm != "bsd" and algorithm != "sysv"
        let tagged_option = if opts.tag and opts.untagged { option_was_last(argv, "--tag", "--untagged", false) } else { opts.tag }
        let tagged = if opts.untagged and ! tagged_option { false } else if tagged_option { true } else { tagged_default }
        let binary = binary_mode(argv, opts.binary)
        if opts.raw {
          let raw = if value.base64 != "" { value.base64.base64_decode()? } else if value.algorithm == "bsd" or value.algorithm == "sysv" { bytes.pack_be(value.number, 2)? } else { bytes.pack_be(value.number, 4)? }
          gnu.write_bytes(raw)
        } else {
          gnu.write_text(checksum_line(name, value, program, tagged, binary, opts.zero, length, opts.base64))
        }
      }
      Err(failure) => {
        failed = true
        gnu.name_error(name, failure)
      }
    }
  }
  if failed { exit 1 }
}
