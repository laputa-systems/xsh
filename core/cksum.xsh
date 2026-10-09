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
  number: Int,
  size: Int,
}

type ChecksumLine = {valid: Bool, path: Str, digest: Str, algorithm: Str, length: Int}

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
    return Ok({algorithm: algorithm, digest: digest.hex(), number: 0, size: data.len()})
  }
  if algorithm == "sha1" {
    let digest = hash.sha1(data)
    return Ok({algorithm: algorithm, digest: digest.hex(), number: 0, size: data.len()})
  }
  if algorithm == "sha224" {
    let digest = hash.sha224(data)
    return Ok({algorithm: algorithm, digest: digest.hex(), number: 0, size: data.len()})
  }
  if algorithm == "sha256" {
    let digest = hash.sha256(data)
    return Ok({algorithm: algorithm, digest: digest.hex(), number: 0, size: data.len()})
  }
  if algorithm == "sha384" {
    let digest = hash.sha384(data)
    return Ok({algorithm: algorithm, digest: digest.hex(), number: 0, size: data.len()})
  }
  if algorithm == "sha512" {
    let digest = hash.sha512(data)
    return Ok({algorithm: algorithm, digest: digest.hex(), number: 0, size: data.len()})
  }
  if algorithm == "blake2b" {
    let digest = hash.blake2b(data, output_length: length / 8)?
    return Ok({algorithm: algorithm, digest: digest.hex(), number: 0, size: data.len()})
  }
  if algorithm == "crc" {
    let value = hash.cksum(data)
    return Ok({algorithm: algorithm, digest: "", number: value.checksum, size: value.bytes})
  }
  if algorithm == "bsd" {
    let value = hash.bsd_sum(data)
    return Ok({algorithm: algorithm, digest: "", number: value.checksum, size: value.blocks})
  }
  if algorithm == "sysv" {
    let value = hash.sysv_sum(data)
    return Ok({algorithm: algorithm, digest: "", number: value.checksum, size: value.blocks})
  }
  let digest = hash.md5(data)
  Ok({algorithm: "md5", digest: digest.hex(), number: 0, size: data.len()})
}

proc digest_path(path_value: Path, algorithm: Str, length: Int) [error] -> Result[ChecksumValue] {
  if algorithm == "md5" {
    match hash.md5(path_value) { Ok(digest) => Ok({algorithm: algorithm, digest: digest.hex(), number: 0, size: 0}); Err(failure) => Err(failure) }
  } else if algorithm == "sha1" {
    match hash.sha1(path_value) { Ok(digest) => Ok({algorithm: algorithm, digest: digest.hex(), number: 0, size: 0}); Err(failure) => Err(failure) }
  } else if algorithm == "sha224" {
    match hash.sha224(path_value) { Ok(digest) => Ok({algorithm: algorithm, digest: digest.hex(), number: 0, size: 0}); Err(failure) => Err(failure) }
  } else if algorithm == "sha256" {
    match hash.sha256(path_value) { Ok(digest) => Ok({algorithm: algorithm, digest: digest.hex(), number: 0, size: 0}); Err(failure) => Err(failure) }
  } else if algorithm == "sha384" {
    match hash.sha384(path_value) { Ok(digest) => Ok({algorithm: algorithm, digest: digest.hex(), number: 0, size: 0}); Err(failure) => Err(failure) }
  } else if algorithm == "sha512" {
    match hash.sha512(path_value) { Ok(digest) => Ok({algorithm: algorithm, digest: digest.hex(), number: 0, size: 0}); Err(failure) => Err(failure) }
  } else if algorithm == "blake2b" {
    match hash.blake2b(path_value, output_length: length / 8) { Ok(digest) => Ok({algorithm: algorithm, digest: digest.hex(), number: 0, size: 0}); Err(failure) => Err(failure) }
  } else if algorithm == "crc" {
    match hash.cksum(path_value) { Ok(value) => Ok({algorithm: algorithm, digest: "", number: value.checksum, size: value.bytes}); Err(failure) => Err(failure) }
  } else if algorithm == "bsd" {
    match hash.bsd_sum(path_value) { Ok(value) => Ok({algorithm: algorithm, digest: "", number: value.checksum, size: value.blocks}); Err(failure) => Err(failure) }
  } else if algorithm == "sysv" {
    match hash.sysv_sum(path_value) { Ok(value) => Ok({algorithm: algorithm, digest: "", number: value.checksum, size: value.blocks}); Err(failure) => Err(failure) }
  } else {
    match hash.md5(path_value) { Ok(digest) => Ok({algorithm: "md5", digest: digest.hex(), number: 0, size: 0}); Err(failure) => Err(failure) }
  }
}

proc checksum_value(name: Str, algorithm: Str, length: Int) [fs, error, io] -> Result[ChecksumValue] {
  if name == "-" {
    let data = io.stdin_bytes()?
    return digest_bytes(data, algorithm, length)
  }
  digest_path(fp"{name}", algorithm, length)
}

pure checksum_line(name: Str, value: ChecksumValue, program: Str, tagged: Bool, binary: Bool, zero: Bool, length: Int) -> Str {
  let end = line_end(zero)
  let label = digest_label(value.algorithm, length)
  let shown_name = checksum_name(name)
  let escaped = shown_name != name and ! zero
  let prefix = if escaped { "\\" } else { "" }
  if value.algorithm == "crc" {
    let file = if name == "-" { "" } else { f" {shown_name}" }
    return f"{value.number} {value.size}{file}{end}"
  }
  if value.algorithm == "bsd" or value.algorithm == "sysv" {
    let number = f"{value.number}"
    let count = f"{value.size}"
    let padded_number = if value.algorithm == "bsd" and value.number < 10000 { f"0{number}" } else { number }
    let shown_count = if value.algorithm == "bsd" and value.size < 10 { f"   {count}" } else if value.algorithm == "bsd" and value.size < 100 { f"  {count}" } else if value.algorithm == "bsd" and value.size < 1000 { f" {count}" } else { count }
    let file = if name == "-" { "" } else { f" {shown_name}" }
    return f"{padded_number} {shown_count}{file}{end}"
  }
  if tagged {
    return f"{prefix}{label} ({shown_name}) = {value.digest}{end}"
  }
  let marker = if binary { " *" } else { "  " }
  f"{prefix}{value.digest}{marker}{shown_name}{end}"
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
  while at < raw.byte_len() and raw.byte_slice(at, length: 1) == " " { at += 1 }
  raw.byte_slice(at)
}

pure parse_checksum_line(line: Str, algorithm: Str, length: Int) -> ChecksumLine {
  let text = if line.starts_with("\\") { line.byte_slice(1) } else { line }
  let end_spaced = text.find(") = ")
  let end_compact = text.find(")= ")
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
    if label == "MD5" { line_algorithm = "md5" }
    if label == "SHA1" { line_algorithm = "sha1" }
    if label == "SHA224" { line_algorithm = "sha224" }
    if label == "SHA256" { line_algorithm = "sha256" }
    if label == "SHA384" { line_algorithm = "sha384" }
    if label == "SHA512" { line_algorithm = "sha512" }
    if label == "BLAKE2b" { line_algorithm = "blake2b"; line_length = 512 }
    if label.starts_with("BLAKE2b-") {
      line_algorithm = "blake2b"
      line_length = label.byte_slice(9).parse_int() ?? -1
    }
    if (line_algorithm != algorithm or label == digest_label(algorithm, length)) and line_length >= 0 {
      let filename = text.byte_slice(start + open_width, length: finish - start - open_width)
      let digest = text.byte_slice(finish + end_width)
      return {valid: digest != "" and filename != "", path: unescape_checksum_path(filename), digest: digest.lower(), algorithm: line_algorithm, length: line_length}
    }
  }

  if algorithm == "crc" {
    let first = text.find(" ")
    if first == null { return {valid: false, path: "", digest: "", algorithm: "crc", length: 0} }
    let number = text.byte_slice(0, length: first ?? 0)
    var second_start = first ?? 0
    while second_start < text.byte_len() and text.byte_slice(second_start, length: 1) == " " { second_start += 1 }
    let second = text.byte_slice(second_start).find(" ")
    if second == null { return {valid: false, path: "", digest: "", algorithm: "crc", length: 0} }
    let size = text.byte_slice(second_start, length: second ?? 0)
    var path_start = second_start + (second ?? 0)
    while path_start < text.byte_len() and text.byte_slice(path_start, length: 1) == " " { path_start += 1 }
    let parsed_filename = if path_start == text.byte_len() { "-" } else { unescape_checksum_path(text.byte_slice(path_start)) }
    let number_value: Int? = match number.parse_int() { Ok(value) => value; Err(_) => null }
    let size_value: Int? = match size.parse_int() { Ok(value) => value; Err(_) => null }
    if number_value == null or size_value == null { return {valid: false, path: "", digest: "", algorithm: "crc", length: 0} }
    return {valid: true, path: parsed_filename, digest: f"{number} {size}", algorithm: "crc", length: 0}
  }

  let separator = text.find(" ")
  if separator == null { return {valid: false, path: "", digest: "", algorithm: algorithm, length: length} }
  let digest = text.byte_slice(0, length: separator ?? 0)
  let first_space = (separator ?? 0) + 1
  if first_space >= text.byte_len() { return {valid: false, path: "", digest: "", algorithm: algorithm, length: length} }
  let two_spaces = text.byte_slice(first_space, length: 1) == " "
  let path_start = if two_spaces { first_space + 1 } else { first_space }
  if path_start >= text.byte_len() { return {valid: false, path: "", digest: "", algorithm: algorithm, length: length} }
  var filename = text.byte_slice(path_start)
  if ! two_spaces and filename.starts_with("*") { filename = filename.byte_slice(1) }
  return {valid: false, path: "", digest: "", algorithm: algorithm, length: length} when filename == ""
  match hash.parse_check_line(f"{digest}  {filename}") {
    Ok(parsed) => {valid: true, path: unescape_checksum_path(parsed.path), digest: parsed.hex, algorithm: algorithm, length: length}
    Err(_) => {valid: false, path: "", digest: "", algorithm: algorithm, length: length}
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
  if bits < 8 or bits > 512 or bits % 8 != 0 {
    gnu.error(f"invalid length: {gnu.quote_value(normalized)}")
    if bits % 8 != 0 { gnu.error("length is not a multiple of 8") }
    if bits > 512 { gnu.error("maximum digest length for 'BLAKE2b' is 512 bits") }
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
        gnu.error(f"{name}: {gnu.strerror(failure)}")
      }
      return false when opts.ignore_missing and gnu.errno(failure) == 2
      return true
    }
  }
  let content = data.utf8() ?? ""
  let delimiter = if opts.zero { "\0" } else { "\n" }
  var failed = false
  var malformed = 0
  var valid_lines = 0
  var verified = 0
  var missing = 0
  var mismatches = 0
  var line_number = 0
  for line in content.split(delimiter) {
    line_number += 1
    if line == "" { continue }
    if trim_leading_space(line).starts_with("#") { continue }
    let parsed = parse_checksum_line(line, algorithm, length)
    if ! parsed.valid {
        malformed += 1
        if opts.warn and ! opts.status { gnu.error(f"{name}: {line_number}: improperly formatted {program} checksum line") }
    } else {
      valid_lines += 1
      let target = parsed.path
      let shown_target = gnu.quote_maybe(target)
      let check_algorithm = parsed.algorithm
      let check_length = parsed.length
      let result: Result[ChecksumValue, Error] = if target == "-" {
        match io.stdin_bytes() {
          Ok(stdin_data) => digest_bytes(stdin_data, check_algorithm, check_length)
          Err(failure) => Err(failure)
        }
      } else {
        digest_path(fp"{target}", check_algorithm, check_length)
      }
      match result {
        Err(failure) => {
          if opts.ignore_missing and gnu.errno(failure) == 2 { missing += 1; continue }
          failed = true
          missing += 1
          if ! opts.status {
            gnu.write_text(f"{shown_target}: FAILED open or read\n")
            gnu.error(f"{shown_target}: {gnu.strerror(failure)}")
          }
        }
        Ok(value) => {
          verified += 1
          let actual = if check_algorithm == "crc" { f"{value.number} {value.size}" } else { value.digest.lower() }
          if actual == parsed.digest.lower() {
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
  reject_if(! is_cksum and opts.algorithm != null, "--algorithm")
  reject_if(! (is_cksum or is_b2sum) and opts.length != null, "--length")
  if opts.tag and opts.check { gnu.usage_error("the --tag option is not supported with --check") }
  if opts.tag and opts.text { gnu.usage_error("the --tag option is not supported with --text") }
  if (opts.status or opts.quiet or opts.strict or opts.warn or opts.ignore_missing) and ! opts.check {
    gnu.usage_error("verification options are meaningful only when verifying checksums")
  }

  if is_cksum and opts.text and opts.tag and ! opts.untagged {
    gnu.usage_error("--text is not supported when --tag is specified")
  }

  let raw_algorithm = opts.algorithm ?? "crc"
  let normalized_algorithm = if raw_algorithm.starts_with("=") { raw_algorithm.byte_slice(1) } else { raw_algorithm }
  var algorithm = if is_sum { sum_mode(argv, "bsd") } else if is_cksum { normalized_algorithm } else { default_algorithm(program) }
  let length = parse_length(opts.length, program, algorithm)
  if algorithm == "sha2" {
    if opts.length == null { gnu.usage_error("--algorithm=sha2 requires specifying --length 224, 256, 384, or 512") }
    algorithm = f"sha{length}"
  }
  let supported = algorithm == "crc" or algorithm == "bsd" or algorithm == "sysv" or algorithm == "md5" or algorithm == "sha1" or algorithm == "sha224" or algorithm == "sha256" or algorithm == "sha384" or algorithm == "sha512" or algorithm == "blake2b"
  if ! supported { gnu.usage_error(f"unsupported algorithm {gnu.quote_value(algorithm)}") }
  if opts.length != null and algorithm != "blake2b" and ! (opts.algorithm == "sha2") {
    gnu.usage_error("--length is only supported with --algorithm blake2b, sha2, or sha3")
  }
  if is_cksum and opts.text and algorithm != "crc" and algorithm != "bsd" and algorithm != "sysv" and tagged_mode(argv, true) {
    gnu.usage_error("--text is not supported with tagged output")
  }
  if opts.check and (algorithm == "crc" or algorithm == "bsd" or algorithm == "sysv") {
    gnu.usage_error(f"--check is not supported with --algorithm={{{algorithm}}}")
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
  var failed = false
  for name in names {
    let result = checksum_value(name, algorithm, length)
    match result {
      Ok(value) => {
        let tagged_default = is_cksum and algorithm != "crc" and algorithm != "bsd" and algorithm != "sysv"
        let tagged_option = if opts.tag and opts.untagged { option_was_last(argv, "--tag", "--untagged", false) } else { opts.tag }
        let tagged = if opts.untagged and ! tagged_option { false } else if tagged_option { true } else { tagged_default }
        let binary = option_was_last(argv, "-b", "-t", opts.binary)
        gnu.write_text(checksum_line(name, value, program, tagged, binary, opts.zero, length))
      }
      Err(failure) => {
        failed = true
        gnu.name_error(name, failure)
      }
    }
  }
  if failed { exit 1 }
}
