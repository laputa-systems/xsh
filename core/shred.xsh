#!/bin/xsh
use lib.gnu
use lib.fs_misc

error ShredError = Invalid : InvalidArgument

type Options = {force: Bool, iterations: Str, size: Str?, exact: Bool, zero: Bool, unlink: Bool, remove: Str?, source: Str?, verbose: Bool, help: Bool, version: Bool, paths: List[Str]}

# Writes bounded chunks in place so hard links refer to the overwritten inode.
proc overwrite(target: Path, length: Int, pattern: Bytes, source: Path, source_offset: Int) [fs, error] -> Result[Int] {
  var offset = 0
  while offset < length {
    let count = if length - offset > 65536 { 65536 } else { length - offset }
    if ! pattern.is_empty() {
      var data = b""
      var chunk = pattern
      while chunk.len() < count + 2 { chunk = bytes.concat([chunk, chunk]) }
      let start = offset % pattern.len()
      data = chunk[start..start + count]
      let _ = bytes.write_at(target, offset, data)?
    } else {
      let data = bytes.read_at(source, source_offset + offset, count)?
      return Err(ShredError.Invalid("end of file")) when data.len() != count
      let _ = bytes.write_at(target, offset, data)?
    }
    offset += count
  }
  fs.fsync(target)
  Ok(source_offset + length)
}

pure pattern_label(data: Bytes) -> Str {
  let digits = "0123456789abcdef"
  var label = ""
  for index in range(data.len()) {
    let byte = data.byte_at(index) ?? 0
    label += digits.byte_slice(byte / 16, length: 1) + digits.byte_slice(byte % 16, length: 1)
  }
  label
}

# Pattern passes are distributed between random passes, with random data at
# both ends. The listed pattern order is stable. For up to three iterations
# every pass uses random data.
pure pass_patterns(passes: Int) -> List[Bytes] {
  var sequence: List[Bytes] = []
  if passes <= 3 { for unused in range(passes) { sequence += [b""] }; return sequence }
  let patterns = [b"\xff\xff\xff", b"\x92\x49\x24", b"\x88\x88\x88", b"\xdb\x6d\xb6", b"\x77\x77\x77", b"\x49\x24\x92", b"\xbb\xbb\xbb", b"\x55\x55\x55", b"\xaa\xaa\xaa", b"\x6d\xb6\xdb", b"\x24\x92\x49", b"\x99\x99\x99", b"\x11\x11\x11", b"\x00\x00\x00", b"\xb6\xdb\x6d", b"\xee\xee\xee", b"\x33\x33\x33", b"\x22\x22\x22", b"\x44\x44\x44", b"\x66\x66\x66", b"\xcc\xcc\xcc", b"\xdd\xdd\xdd"]
  let randoms = if passes / 10 > 3 { passes / 10 } else { 3 }
  let count = passes - randoms
  sequence += [b""]
  var added = 0
  for section in range(randoms - 1) {
    let length = count / (randoms - 1) + (if section < count % (randoms - 1) { 1 } else { 0 })
    for unused in range(length) { sequence += [patterns[added % patterns.len()]]; added += 1 }
    if section < randoms - 2 { sequence += [b""] }
  }
  sequence += [b""]
  sequence
}

proc quote_maybe_bytes(name: Bytes) [env] -> Str {
  match name.utf8() {
    Ok(text) => gnu.quote_maybe(text)
    Err(_) => gnu.quote_bytes(name, always: false)
  }
}

proc name_error(name: Bytes, failure: Error) [process, env] -> Unit {
  gnu.error(f"{quote_maybe_bytes(name)}: {gnu.strerror(failure)}")
}

proc main(...argv: List[Bytes]) [fs, process, env, error, io] {
  let prepared = gnu.prepare_arguments(argv)
  let opts: Options = cli.applet(prepared.text, {
    gnu: {status: 1},
    force: {form: "-f --force", default: false},
    iterations: {form: "-n --iterations N", default: "3"},
    size: {form: "-s --size N"},
    exact: {form: "-x --exact", default: false},
    zero: {form: "-z --zero", default: false},
    unlink: {form: "-u", default: false},
    remove: {form: "--remove[=HOW]", optional_default: "wipesync"},
    source: {form: "--random-source FILE"},
    verbose: {form: "-v --verbose", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    paths: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: shred [OPTION]... FILE...\nOverwrite FILE repeatedly.\n  -f, --force\n  -n, --iterations=N\n  -s, --size=N\n  -x, --exact\n  -z, --zero\n  -u, --remove[=HOW]\n  --random-source=FILE\n  -v, --verbose\n"); return }
  if opts.version { gnu.version("shred"); return }
  if opts.paths.is_empty() { gnu.missing_operand() }
  let parsed = opts.iterations.parse_int()
  if parsed is Err(_) or (parsed ?? -1) < 0 { gnu.error(f"invalid number of passes: {gnu.quote_value(opts.iterations)}"); exit 1 }
  let passes = parsed?
  var size: Int? = null
  if opts.size != null {
    size = fs_misc.size_value(opts.size ?? "")
    if size == null { gnu.error(f"invalid file size: {gnu.quote_value(opts.size ?? "")}"); exit 1 }
  }
  var remove = opts.remove ?? (if opts.unlink { "wipesync" } else { "" })
  if remove != "" {
    var matches: List[Str] = []
    if remove in ["unlink", "wipe", "wipesync"] { matches = [remove] } else {
      for method in ["unlink", "wipe", "wipesync"] { if method.starts_with(remove) { matches += [method] } }
    }
    if matches.len() != 1 { gnu.error(f"invalid argument {gnu.quote(remove)} for 'remove'"); exit 1 }
    remove = matches[0]
  }
  let source_bytes = if opts.source == null { b"/dev/urandom" } else { gnu.argument_bytes(opts.source ?? "", prepared.raw) }
  let source = Path.parse_bytes(source_bytes)?
  if opts.source != null {
    let info = fs.stat(source, follow_symlinks: true)
    if let Err(failure) = info { gnu.error(f"{quote_maybe_bytes(source_bytes)}: {gnu.strerror(failure)}"); exit 1 }
    if info?.kind == "dir" { gnu.error(f"{quote_maybe_bytes(source_bytes)}: Is a directory"); exit 1 }
  }
  var failed = false
  var source_offset = 0
  for path_name in opts.paths {
    let name = gnu.argument_bytes(path_name, prepared.raw)
    let target = Path.parse_bytes(name)?
    let metadata = fs.stat(target, follow_symlinks: true)
    if let Err(failure) = metadata { name_error(name, failure); failed = true; continue }
    let info = metadata?
    if info.kind != "file" { gnu.error(f"{quote_maybe_bytes(name)}: {if info.kind == "dir" { "Is a directory" } else { "invalid file type" }}"); failed = true; continue }
    if opts.force {
      if let Err(failure) = target.chmod(info.mode.bit_and(0o777).bit_or(0o200)) { name_error(name, failure); failed = true; continue }
    }
    var length = size ?? info.size
    if ! opts.exact and size == null and length > 0 {
      let block = info.blksize
      if length % block != 0 { length += block - length % block }
    }
    var complete = true
    var schedule = pass_patterns(passes)
    if opts.zero { schedule += [b"\x00\x00\x00"] }
    # Clear the original short extent before overwriting its padded block;
    # otherwise tail packing can leave the old bytes outside the new extent.
    let preliminary = if info.size > 0 and info.size < info.blksize and ((size != null and info.size < length) or (size == null and ! opts.exact)) { info.size } else { 0 }
    if preliminary > 0 {
      for pattern in schedule {
        let result = overwrite(target, preliminary, pattern, source, if opts.source == null { 0 } else { source_offset })
        if let Err(failure) = result { gnu.error(f"{gnu.quote_bytes(name)}: error writing: {gnu.strerror(failure)}"); failed = true; complete = false; break }
        if opts.source != null and pattern.is_empty() { source_offset = result? }
      }
    }
    for pass in range(schedule.len()) {
      break when ! complete
      break when length == 0
      let pattern = schedule[pass]
      let label = if pattern.is_empty() { "random" } else { pattern_label(pattern) }
      if opts.verbose { gnu.error(f"{quote_maybe_bytes(name)}: pass {pass + 1}/{schedule.len()} ({label})...") }
      let result = overwrite(target, length, pattern, source, if opts.source == null { 0 } else { source_offset })
      if let Err(failure) = result { gnu.error(f"{gnu.quote_bytes(name)}: error writing: {gnu.strerror(failure)}"); failed = true; complete = false; break }
      if opts.source != null and pattern.is_empty() { source_offset = result? }
    }
    if ! complete or remove == "" { continue }
    if opts.verbose { gnu.error(f"{quote_maybe_bytes(name)}: removing") }
    if let Err(failure) = target.truncate(0) { name_error(name, failure); failed = true; continue }
    var last = target
    if remove != "unlink" {
      let parent_dir = target.parent()
      let components = target.components()
      let leaf = components[components.len() - 1].bytes()
      for index in range(leaf.len()) {
        let width = leaf.len() - index
        var digits: List[Int] = []
        for unused in range(width) { digits += [0] }
        let alphabet = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"
        while true {
          var renamed_name = ""
          for digit in digits { renamed_name += alphabet.byte_slice(digit, length: 1) }
          let next = Path.parse_bytes(bytes.concat([parent_dir.bytes(), b"/", bytes.from_text(renamed_name)]))?
          let renamed = fs.rename_noreplace(last, next)
          if let Err(failure) = renamed {
            if failure.errno != 17 {
              # Renaming is best effort; unlink reports the final removal error.
              break
            }
            var overflow = true
            for position in range(width) {
              let at = width - position - 1
              digits[at] += 1
              if digits[at] < 62 { overflow = false; break }
              digits[at] = 0
            }
            break when overflow
            continue
          }
          let next_components = next.components()
          if opts.verbose { gnu.error(f"{quote_maybe_bytes(name)}: renamed to {quote_maybe_bytes(next_components[next_components.len() - 1].bytes())}") }
          last = next
          if remove == "wipesync" {
            if let Err(failure) = fs.fsync(parent_dir) { name_error(name, failure); failed = true; complete = false }
          }
          break
        }
        break when ! complete
      }
    }
    if complete {
      if let Err(failure) = last.remove() { gnu.error(f"{quote_maybe_bytes(name)}: failed to remove: {gnu.strerror(failure)}"); failed = true } else {
        if opts.verbose { gnu.error(f"{quote_maybe_bytes(name)}: removed") }
        if remove == "wipesync" {
          if let Err(failure) = fs.fsync(last.parent()) { name_error(name, failure); failed = true }
        }
      }
    }
  }
  if failed { exit 1 }
}
