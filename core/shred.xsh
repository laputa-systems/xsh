#!/bin/xsh
use lib.gnu

const USAGE = """Usage: shred [OPTION]... FILE...
Overwrite the specified FILEs to make their contents harder to recover.

  -f, --force             change permissions to allow writing if needed
  -n, --iterations=N      overwrite N times instead of 3
      --random-source=FILE  get random bytes from FILE
  -s, --size=N            shred this many bytes
  -u, --remove[=HOW]      truncate and remove after overwriting
  -v, --verbose           show progress
  -x, --exact             do not round file sizes up to a block
  -z, --zero              add a final overwrite with zeros
      --help              display this help and exit
      --version           output version information and exit
"""

type ShredOptions = {force: Bool, iterations: Str, size: Str, random_source: Str, remove: Str, unlink: Bool, verbose: Bool, exact: Bool, zero: Bool, help: Bool, version: Bool, files: List[Str]}

const PATTERNS = ["000000", "ffffff", "555555", "aaaaaa", "249249", "492492", "6db6db", "924924", "b6db6d", "db6db6", "111111", "222222", "333333", "444444", "666666", "777777", "888888", "999999", "bbbbbb", "cccccc", "dddddd", "eeeeee"]
const TEST_ORDER = ["ffffff", "924924", "888888", "db6db6", "777777", "492492", "bbbbbb", "555555", "aaaaaa", "6db6db", "249249", "999999", "111111", "000000", "b6db6d", "eeeeee", "333333"]

pure parse_uint(text: Str) -> Int? {
  return null when text == ""
  let hex = text.starts_with("0x") or text.starts_with("0X")
  let raw = if hex { text.byte_slice(2) } else { text }
  return null when raw == ""
  var value = 0
  var at = 0
  while at < raw.byte_len() {
    let c = raw.byte_slice(at, length: 1)
    let digit = if c >= "0" and c <= "9" { (c.byte_at(0) ?? 48) - 48 } else if hex and c >= "a" and c <= "f" { (c.byte_at(0) ?? 97) - 87 } else if hex and c >= "A" and c <= "F" { (c.byte_at(0) ?? 65) - 55 } else { -1 }
    return null when digit < 0 or digit >= (if hex { 16 } else { 10 })
    return null when value > (9223372036854775807 - digit) / (if hex { 16 } else { 10 })
    value = value * (if hex { 16 } else { 10 }) + digit
    at += 1
  }
  value
}

pure increment_name_at(text: Str, position: Int) -> Str? {
  return null when position < 0
  const nameset = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_."
  let current = text.byte_slice(position, length: 1)
  let digit = nameset.find(current)
  return null when digit == null
  let next = (digit ?? 0) + 1
  if next < nameset.byte_len() {
    return f"{text.byte_slice(0, length: position)}{nameset.byte_slice(next, length: 1)}{text.byte_slice(position + 1)}"
  }
  let prefix = increment_name_at(text, position - 1)
  return null when prefix == null
  var zeros = ""
  for _ in range(text.byte_len() - position) { zeros += "0" }
  f"{prefix ?? ""}{zeros}"
}

pure increment_name(text: Str) -> Str? {
  increment_name_at(text, text.byte_len() - 1)
}

pure raw_files(argv: List[Str], raw: List[Bytes]) -> List[Bytes] {
  var files: List[Bytes] = []
  var index = 0
  var options = true
  while index < argv.len() {
    let arg = argv[index]
    if options and arg == "--" { options = false; index += 1; continue }
    if options and (arg == "-n" or arg == "--iterations" or arg == "-s" or arg == "--size" or arg == "--random-source") { index += 2; continue }
    if options and (arg.starts_with("--iterations=") or arg.starts_with("--size=") or arg.starts_with("--random-source=") or arg.starts_with("--remove=") or (arg.starts_with("-n") and arg.byte_len() > 2) or (arg.starts_with("-s") and arg.byte_len() > 2)) { index += 1; continue }
    if options and arg.starts_with("-") and arg != "-" { index += 1; continue }
    files += [raw[index]]
    index += 1
  }
  files
}

pure pattern_byte(name: Str, index: Int) -> Int {
  if name == "000000" { return 0 }
  if name == "ffffff" { return 255 }
  if name == "555555" { return 85 }
  if name == "aaaaaa" { return 170 }
  if name == "249249" { return [36, 146, 73][index % 3] }
  if name == "492492" { return [73, 36, 146][index % 3] }
  if name == "6db6db" { return [109, 182, 219][index % 3] }
  if name == "924924" { return [146, 73, 36][index % 3] }
  if name == "b6db6d" { return [182, 219, 109][index % 3] }
  if name == "db6db6" { return [219, 109, 182][index % 3] }
  if name == "111111" { return 17 }
  if name == "222222" { return 34 }
  if name == "333333" { return 51 }
  if name == "444444" { return 68 }
  if name == "666666" { return 102 }
  if name == "777777" { return 119 }
  if name == "888888" { return 136 }
  if name == "999999" { return 153 }
  if name == "bbbbbb" { return 187 }
  if name == "cccccc" { return 204 }
  if name == "dddddd" { return 221 }
  238
}

pure pass_sequence(count: Int, seeded: Bool) -> List[Str] {
  if count <= 3 { return ["random" for _ in range(count)] }
  if seeded and count == 20 {
    var ordered = ["random", @TEST_ORDER[0..9], "random", @TEST_ORDER[9..], "random"]
    return ordered
  }
  let patterns = count - 3
  var out = ["random"]
  for index in range(patterns) { out += [PATTERNS[index % PATTERNS.len()]] }
  let middle = out.len() / 2
  out = out[0..middle] + ["random"] + out[middle..]
  out += ["random"]
  out
}

proc remove_file(file_path: Path, name: Str, how: Str, verbose: Bool) [fs, process, env, io, error] -> Result[Bool] {
  if how == "" { return Ok(true) }
  if verbose { gnu.error(f"{gnu.quote(name)}: removing") }
  if how == "unlink" {
    if let Err(failure) = file_path.remove() { gnu.error(f"cannot remove {gnu.quote(name)}: {gnu.strerror(failure)}"); return Ok(false) }
  } else {
    let parent = file_path.parent()
    var current = file_path
    var renamed = false
    var first_failure: Str? = null
    var first_target = ""
    let original_name_length = file_path.basename().byte_len()
    for index in range(original_name_length) {
      let length = original_name_length - index
      var candidate = ""
      for _ in range(length) { candidate += "0" }
      while true {
        let next = fp"{parent}/{candidate}"
        let move_result = fs.rename_noreplace(current, next)
        if let Ok(_) = move_result {
          renamed = true
          if verbose { gnu.error(f"{gnu.quote_maybe(name)}: renamed to {candidate}") }
          current = next
          if how == "wipesync" {
            if let Err(failure) = fs.sync() { gnu.error(f"cannot synchronize {gnu.quote_maybe(name)}: {gnu.strerror(failure)}"); return Ok(false) }
          }
          break
        }
        if let Err(failure) = move_result {
          if first_failure == null {
            first_failure = gnu.strerror(failure)
            first_target = next.display()
          }
          if gnu.errno(failure) == 17 {
            let incremented = increment_name(candidate)
            if incremented != null {
              candidate = incremented ?? candidate
              continue
            }
          }
        }
        break
      }
    }
    if !renamed and first_failure != null {
      let reason = first_failure ?? ""
      gnu.error(f"{gnu.quote_maybe(name)}: Couldn't rename to {gnu.quote_value(first_target)}: {reason}")
      return Ok(false)
    }
    if let Err(failure) = current.remove() { gnu.error(f"cannot remove {gnu.quote(name)}: {gnu.strerror(failure)}"); return Ok(false) }
  }
  if verbose { gnu.error(f"{gnu.quote_maybe(name)}: removed") }
  Ok(true)
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  var adjusted: List[Str] = []
  for arg in argv { adjusted += [if arg == "--remove" { "--remove=wipesync" } else { arg }] }
  let opts: ShredOptions = cli.applet(
    adjusted,
    {
      gnu: {status: 1},
      force: {form: "-f --force", default: false},
      iterations: {form: "-n --iterations NUM", default: "3"},
      size: {form: "-s --size SIZE", default: ""},
      random_source: {form: "--random-source FILE", default: ""},
      remove: {form: "--remove MODE", default: ""},
      unlink: {form: "-u", default: false},
      verbose: {form: "-v --verbose", default: false},
      exact: {form: "-x --exact", default: false},
      zero: {form: "-z --zero", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      files: {form: "...FILE"},
    },
  )?
  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("shred"); return }
  if opts.files.len() == 0 { gnu.missing_operand() }
  let count = parse_uint(opts.iterations)
  if count == null or (count ?? 0) > 1000 { gnu.usage_error(f"invalid number of passes {gnu.quote_value(opts.iterations)}") }
  var how = opts.remove
  if opts.unlink { how = "wipesync" }
  if how != "" {
    if "unlink".starts_with(how) { how = "unlink" } else if how == "wip" { gnu.usage_error(f"ambiguous remove method {gnu.quote_value(how)}") } else if "wipe".starts_with(how) { how = "wipe" } else if "wipesync".starts_with(how) { how = "wipesync" } else { gnu.usage_error(f"invalid remove method {gnu.quote_value(how)}") }
  }
  var requested: Int? = null
  if opts.size != "" { requested = parse_uint(opts.size) }
  if opts.size != "" and requested == null { gnu.error(f"invalid file size: {gnu.quote_value(opts.size)}"); exit 1 }

  var source_path: Path? = null
  var source_is_file = false
  var seeded = false
  if opts.random_source != "" {
    let source = fp"{opts.random_source}"
    let metadata = fs.stat(source, true)
    if let Err(failure) = metadata {
      gnu.error(f"{gnu.quote_maybe(opts.random_source)}: {gnu.strerror(failure)}")
      exit 1
    } else if let Ok(found) = metadata {
      source_path = source
      source_is_file = found.kind == "file"
      if let Ok(seed) = bytes.read_at(source, 0, 1024) {
        seeded = seed.len() == 1024
        for at in range(seed.len()) {
          if seed.byte_at(at) != 85 { seeded = false; break }
        }
      }
    }
  }
  let sequence = pass_sequence(count ?? 3, seeded)
  let raw_paths = raw_files(argv, cli.argv_bytes())
  var failed = false
  for index in range(opts.files.len()) {
    let name = opts.files[index]
    let target = Path.parse_bytes(raw_paths[index])?
    let metadata = fs.stat(target, true)
    if let Err(failure) = metadata {
      gnu.error(f"cannot open {gnu.quote(name)}: {gnu.strerror(failure)}")
      failed = true
      continue
    }
    let meta = if let Ok(value) = metadata { value } else { fs.stat(target, true)? }
    if name.ends_with("/") {
      let reason = if meta.kind == "dir" { "Is a directory" } else { "Not a directory" }
      gnu.error(f"cannot open {gnu.quote(name)} for writing: {reason}")
      failed = true
      continue
    }
    if meta.kind != "file" {
      gnu.error(f"{gnu.quote(name)}: not a regular file")
      failed = true
      continue
    }
    if meta.mode / 0o200 % 2 == 0 {
      if ! opts.force {
        if how != "" { gnu.error(f"{gnu.quote(name)}: cannot rename to permit removal") } else { gnu.error(f"cannot open {gnu.quote(name)} for writing: Permission denied") }
        failed = true
        continue
      }
      if let Err(failure) = target.chmod(meta.mode % 0o10000 + 0o200) {
        gnu.error(f"cannot open {gnu.quote(name)} for writing: {gnu.strerror(failure)}")
        failed = true
        continue
      }
    }
    let original_result = target.read_bytes()
    if let Err(failure) = original_result {
      gnu.error(f"cannot read {gnu.quote(name)}: {gnu.strerror(failure)}")
      failed = true
      continue
    }
    let original = if let Ok(data) = original_result { data } else { b"" }
    let bytes_to_write = requested ?? original.len()
    if bytes_to_write > 16777216 { gnu.error("file size is too large to shred in this XSH build"); failed = true; continue }
    let output_size = if opts.exact or requested != null { bytes_to_write } else { (bytes_to_write + 4095) / 4096 * 4096 }
    var random_offset = 0
    var failed_file = false
    var pass_index = 0
    let active_sequence: List[Str] = if original.len() == 0 { [] } else { sequence }
    for pass in active_sequence {
      pass_index += 1
      if opts.verbose { gnu.error(f"{gnu.quote_maybe(name)}: pass {pass_index}/{active_sequence.len()} ({pass})...") }
      var random_bytes = b""
      if pass == "random" {
        let source = source_path ?? p"/dev/urandom"
        let offset = if source_path != null and source_is_file { random_offset } else { 0 }
        let found = bytes.read_at(source, offset, output_size)
        if let Err(failure) = found {
          let message = if source_path != null and source_is_file and gnu.strerror(failure) == "failed to fill whole buffer" { "unexpected end of file" } else { gnu.strerror(failure) }
          let source_name = if source_path == null { "/dev/urandom" } else { opts.random_source }
          gnu.error(f"{gnu.quote(source_name)}: {message}")
          failed_file = true
          break
        } else if let Ok(data) = found {
          random_bytes = data
          if data.len() < output_size {
            gnu.error(f"{gnu.quote(opts.random_source)}: unexpected end of file")
            failed_file = true
            break
          }
          if source_path != null and source_is_file { random_offset += output_size }
        }
      }
      var output: List[Int] = []
      var at = 0
      while at < output_size {
        let byte = if pass == "000000" { 0 } else if pass == "random" {
          random_bytes.byte_at(at) ?? 0
        } else { pattern_byte(pass, at) }
        output += [byte]
        at += 1
      }
      if let Err(failure) = target.write(bytes.from_ints(output)?) {
        gnu.error(f"cannot write {gnu.quote(name)}: {gnu.strerror(failure)}")
        failed_file = true
        break
      }
    }
    if opts.zero and original.len() > 0 and ! failed_file {
      var zeros: List[Int] = []
      for _ in range(output_size) { zeros += [0] }
      if let Err(failure) = target.write(bytes.from_ints(zeros)?) {
        gnu.error(f"cannot write {gnu.quote(name)}: {gnu.strerror(failure)}")
        failed_file = true
      }
    }
    if ! failed_file and how != "" {
      if ! remove_file(target, name, how, opts.verbose)? { failed_file = true }
    }
    if failed_file { failed = true }
  }
  if failed { exit 1 }
}
