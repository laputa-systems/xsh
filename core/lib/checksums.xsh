##! Shared GNU checksum formatting and verification for the checksum applets.
use gnu

type Options = {
  binary: Bool, text: Bool, check: Bool, tag: Bool, untagged: Bool,
  zero: Bool, quiet: Bool, status: Bool, warn: Bool, strict: Bool,
  ignore_missing: Bool, algorithm: Str, length: Str, base64: Bool, raw: Bool,
  help: Bool, version: Bool, files: List[Str],
}
type Numeric = {checksum: Int, size: Int}
type Check = {hex: Str, path: Str, algorithm: Str, length: Int}

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

pure padded(value: Int, width: Int, fill: Str) -> Str {
  var result = f"{value}"
  while result.byte_len() < width { result = fill + result }
  result
}

pure label(algorithm: Str, length: Int) -> Str {
  return f"BLAKE2b-{length}" when algorithm == "blake2b" and length != 512
  return "BLAKE2b" when algorithm == "blake2b"
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
    _ => 0
  }
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

pure last_separator(line: Str) -> Int? {
  var offset = 0
  var found: Int? = null
  while offset < line.byte_len() {
    let next = line.byte_slice(offset).find(") = ")
    if next == null { break }
    offset += next ?? 0
    found = offset
    offset += 1
  }
  found
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
  let tag_end = line.find(" (") ?? -1
  let split = last_separator(line) ?? -1
  if tag_end > 0 and split > tag_end {
    let tag = line.byte_slice(0, length: tag_end)
    if infer {
      selected = tag.lower()
      if selected.starts_with("blake2b-") {
        bits = selected.byte_slice(8).parse_int() ?? 0
        selected = "blake2b"
      }
    }
    if tag != label(selected, bits) { return null }
    name = line.byte_slice(tag_end + 2, length: split - tag_end - 2)
    expected = line.byte_slice(split + 4)
  } else {
    if infer {
      let width = line.find(" ") ?? 0
      selected = match width { 32 => "md5", 40 => "sha1", 56 => "sha224", 64 => "sha256", 96 => "sha384", 128 => "sha512", _ => "" }
    }
    var width = digest_length(selected)
    if selected == "blake2b" {
      let separator = line.find(" ") ?? -1
      if separator < 0 { return null }
      width = separator
      bits = width * 4
    }
    if line.byte_len() <= width + 1 { return null }
    expected = line.byte_slice(0, length: width)
    if line.byte_slice(width, length: 1) != " " { return null }
    let marker = line.byte_slice(width + 1, length: 1)
    name = line.byte_slice(width + if marker == " " or marker == "*" { 2 } else { 1 })
  }
  let width = if selected == "blake2b" { bits / 4 } else { digest_length(selected) }
  if width < 2 or expected.byte_len() != width or expected.lower().translate("0123456789abcdef", "") != "" or name == "" { return null }
  if escape {
    if let decoded = unescape(name) { name = decoded } else { return null }
  }
  {hex: expected.lower(), path: name, algorithm: selected, length: bits}
}

proc digest(name: Str, algorithm: Str, length: Int) [fs, io] -> Result[Digest] {
  if name == "-" { hash.digest_stdin(algorithm, length: length) } else { hash.digest_file(fp"{name}", algorithm, length: length) }
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
  let content = if source == "-" { io.stdin_text() } else { fp"{source}".read_text() }
  let text = match content {
    Ok(value) => value
    Err(failure) => { gnu.name_error(source, failure); return false }
  }
  var malformed = 0
  var mismatched = 0
  var unreadable = 0
  var checked = 0
  var valid = 0
  var line_number = 0
  for line in text.lines() {
    line_number += 1
    if line.starts_with("#") { continue }
    let entry = parse_line(line, algorithm, length, infer)
    if entry == null {
      malformed += 1
      if opts.warn and !opts.status {
        gnu.error(f"{source}: {line_number}: improperly formatted {label(algorithm, length)} checksum line")
      }
      continue
    }
    let item = entry
    valid += 1
    let shown = if needs_escape(item.path) { f"\\{escaped(item.path)}" } else { item.path }
    match digest(item.path, item.algorithm, item.length) {
      Ok(value) => {
        checked += 1
        if value.hex() == item.hex {
          if !opts.quiet and !opts.status { gnu.write_text(f"{shown}: OK\n") }
        } else {
          mismatched += 1
          if !opts.status { gnu.write_text(f"{shown}: FAILED\n") }
        }
      }
      Err(failure) => {
        if opts.ignore_missing and gnu.errno(failure) == 2 { continue }
        unreadable += 1
        if !opts.status {
          gnu.name_error(item.path, failure)
          gnu.write_text(f"{shown}: FAILED open or read\n")
        }
      }
    }
  }
  if valid == 0 {
    gnu.error(f"{if source == "-" { "'standard input'" } else { source }}: no properly formatted checksum lines found")
    return false
  }
  if !opts.status {
    if malformed > 0 { gnu.error(f"WARNING: {malformed} {if malformed == 1 { "line is" } else { "lines are" }} improperly formatted") }
    if unreadable > 0 { gnu.error(f"WARNING: {unreadable} listed {if unreadable == 1 { "file could" } else { "files could" }} not be read") }
    if mismatched > 0 { gnu.error(f"WARNING: {mismatched} computed {if mismatched == 1 { "checksum did" } else { "checksums did" }} NOT match") }
    if checked == 0 and unreadable == 0 { gnu.error(f"{source}: no file was verified") }
  }
  checked > 0 and unreadable == 0 and mismatched == 0 and (!opts.strict or malformed == 0)
}

## Parse conventional checksum options, hash files or stdin, and verify lists.
export proc main(argv: List[Str], default_algorithm: Str, cksum = false) [fs, io, error, process, env] -> Unit {
  let opts: Options = cli.applet(argv, {
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
  })?
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
  if length < 0 { gnu.usage_error(f"invalid length: {gnu.quote(opts.length)}") }
  if algorithm == "sha2" {
    if length != 224 and length != 256 and length != 384 and length != 512 { gnu.usage_error("--algorithm=sha2 requires specifying --length 224, 256, 384, or 512") }
    algorithm = f"sha{length}"
  } else if algorithm != "blake2b" and length != 0 {
    gnu.usage_error("--length is only supported with --algorithm blake2b or sha2")
  }
  if algorithm == "blake2b" and (length > 512 or length % 8 != 0) { gnu.usage_error("invalid BLAKE2b length: must be a multiple of 8 up to 512") }
  if length == 0 { length = 512 }
  let is_numeric = algorithm == "crc" or algorithm == "bsd" or algorithm == "sysv" or algorithm == "crc32b"
  if !is_numeric and algorithm != "blake2b" and digest_length(algorithm) == 0 { gnu.usage_error(f"invalid argument {gnu.quote(algorithm)} for 'checksum algorithm'") }
  if !opts.check {
    for flag in [if opts.quiet { "quiet" } else { "" }, if opts.status { "status" } else { "" }, if opts.warn { "warn" } else { "" }, if opts.strict { "strict" } else { "" }, if opts.ignore_missing { "ignore-missing" } else { "" }] {
      if flag != "" { gnu.usage_error(f"the --{flag} option is meaningful only when verifying checksums") }
    }
  }
  if opts.check and (opts.zero or opts.tag or opts.binary or opts.text or opts.raw or opts.base64) { gnu.usage_error("output format options are meaningless when verifying checksums") }
  if cksum and opts.text and !binary and tagged { gnu.usage_error("--text mode is only supported with --untagged") }
  if opts.tag and opts.text and !binary { gnu.usage_error("--tag does not support --text mode") }
  if opts.check and is_numeric and (!cksum or opts.algorithm != "") { gnu.usage_error("--check is not supported with --algorithm={bsd,sysv,crc,crc32b}") }
  let files = if opts.files.is_empty() { ["-"] } else { opts.files }
  if opts.raw and files.len() > 1 { gnu.usage_error("the --raw option is not supported with multiple files") }
  if opts.raw and opts.base64 { gnu.usage_error("the --base64 and --raw options are mutually exclusive") }
  var failed = false
  for name in files {
    if opts.check {
      if !verify_list(name, opts, algorithm, length, cksum and opts.algorithm == "") { failed = true }
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
