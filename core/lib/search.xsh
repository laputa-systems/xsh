##! Byte-oriented matching and path presentation shared by search applets.

use gnu

error SearchError = Invalid : Usage

## Return an input error so applets can choose their own diagnostic status.
export pure reject(message: Str) -> Result[Unit, Error] {
  Err(SearchError.Invalid(message))
}

## The final path component, preserving undecodable filename bytes.
export pure basename_bytes(raw: Bytes) -> Bytes {
  var end = raw.len()
  while end > 1 and raw.byte_at(end - 1) == 47 { end -= 1 }
  var at = end
  while at > 0 and raw.byte_at(at - 1) != 47 { at -= 1 }
  raw[at..end]
}

## Join a parent with a raw filename without decoding it.
export pure child(parent: Path, name: Bytes) -> Result[Path, Error] {
  let leaf = Path.parse_bytes(name)?
  Ok(if parent.bytes().byte_at(parent.bytes().len() - 1) == 47 { fp"{parent}{leaf}" } else { fp"{parent}/{leaf}" })
}

## ASCII case folding leaves all other bytes unchanged.
export pure fold(byte: Int, insensitive: Bool) -> Int {
  if insensitive and byte >= 65 and byte <= 90 { byte + 32 } else { byte }
}

pure glob_at(pattern: Bytes, text: Bytes, p: Int, t: Int, insensitive: Bool) -> Bool {
  return t == text.len() when p >= pattern.len()
  let char = pattern.byte_at(p) ?? -1
  if char == 42 {
    var next = p + 1
    while pattern.byte_at(next) == 42 { next += 1 }
    return true when next == pattern.len()
    for at in range(t, text.len() + 1) {
      return true when glob_at(pattern, text, next, at, insensitive)
    }
    return false
  }
  return false when t >= text.len()
  let value = fold(text.byte_at(t) ?? -1, insensitive)
  if char == 63 { return glob_at(pattern, text, p + 1, t + 1, insensitive) }
  if char == 91 {
    var at = p + 1
    let negate = pattern.byte_at(at) == 33 or pattern.byte_at(at) == 94
    if negate { at += 1 }
    var matched = false
    var items = 0
    while at < pattern.len() and (pattern.byte_at(at) != 93 or items == 0) {
      let low = fold(pattern.byte_at(at) ?? -1, insensitive)
      if pattern.byte_at(at + 1) == 45 and at + 2 < pattern.len() and pattern.byte_at(at + 2) != 93 {
        let high = fold(pattern.byte_at(at + 2) ?? -1, insensitive)
        matched = matched or (value >= low and value <= high)
        at += 3
      } else {
        matched = matched or value == low
        at += 1
      }
      items += 1
    }
    if pattern.byte_at(at) == 93 {
      return matched != negate and glob_at(pattern, text, at + 1, t + 1, insensitive)
    }
  }
  let escaped = char == 92 and p + 1 < pattern.len()
  let literal = if escaped { pattern.byte_at(p + 1) ?? -1 } else { char }
  value == fold(literal, insensitive) and glob_at(pattern, text, p + (if escaped { 2 } else { 1 }), t + 1, insensitive)
}

## Match shell filename wildcards over bytes, with optional ASCII case folding.
export pure glob(pattern: Str, text: Bytes, insensitive = false) -> Bool {
  glob_at(bytes.from_text(pattern), text, 0, 0, insensitive)
}

## Find a literal byte sequence starting at the requested offset.
export pure find_fixed(text: Bytes, needle: Bytes, offset = 0, insensitive = false) -> Int? {
  return offset when needle.is_empty() and offset <= text.len()
  for at in range(offset, text.len() - needle.len() + 1) {
    var same = true
    for index in range(needle.len()) {
      if fold(text.byte_at(at + index) ?? -1, insensitive) != fold(needle.byte_at(index) ?? -1, insensitive) {
        same = false
        break
      }
    }
    return at when same
  }
  null
}

## Classify the ASCII word characters used in the C locale.
export pure word(byte: Int) -> Bool {
  byte == 95 or (byte >= 48 and byte <= 57) or (byte >= 65 and byte <= 90) or (byte >= 97 and byte <= 122)
}

## Split records without inventing an extra record after a final delimiter.
export pure records(data: Bytes, delimiter: Int) -> List[Bytes] {
  var out: List[Bytes] = []
  var start = 0
  for at in range(data.len()) {
    if data.byte_at(at) == delimiter { out += [data[start..at]]; start = at + 1 }
  }
  if start < data.len() { out += [data[start..]] }
  out
}


proc grep_write(data: Bytes) {
  let written = try { io.write_stdout_bytes(data)?; io.flush_stdout()? }
  if let Err(failure) = written {
    if gnu.errno(failure) == 32 { exit 141 }
    gnu.error(f"write error: {gnu.strerror(failure)}")
    exit 2
  }
}

type Span = {start: Int, end: Int}
type GrepOptions = {mode: Str, color: Bool, insensitive: Bool, whole: Bool, words: Bool, invert: Bool, numbered: Bool, byte_offset: Bool, null_names: Bool, silent: Bool, names: Int, count: Bool, list: Int, quiet: Bool, only: Bool, delimiter: Int, binary: Str, before: Int, after: Int, limit: Int, recursive: Int, includes: List[Str], excludes: List[Str], exclude_dirs: List[Str], patterns: List[Str]}

proc grep_spans(line: Bytes, opts: GrepOptions) -> Result[List[Span]] {
  var spans: List[Span] = []
  for pattern in opts.patterns {
    var candidates: List[Span] = []
    if opts.mode == "fixed" {
      let needle = bytes.from_text(pattern)
      var at = 0
      while at <= line.len() {
        let found = find_fixed(line, needle, offset: at, insensitive: opts.insensitive)
        if found == null { break }
        let start = found ?? 0
        candidates += [{start: start, end: start + needle.len()}]
        at = start + (if needle.is_empty() { 1 } else { needle.len() })
      }
    } else { candidates = regex.find_bytes(pattern, line, extended: opts.mode == "extended", ignore_case: opts.insensitive)? }
    for span in candidates {
      if opts.whole and (span.start != 0 or span.end != line.len()) { continue }
      if opts.words and (word(line.byte_at(span.start - 1) ?? -1) or word(line.byte_at(span.end) ?? -1)) { continue }
      spans += [span]
    }
  }
  Ok(spans)
}

pure colored(line: Bytes, spans: List[Span], enabled: Bool) -> Bytes {
  if ! enabled { return line }
  var chunks: List[Bytes] = []
  var cursor = 0
  while cursor < line.len() {
    var best: Span? = null
    for span in spans {
      if span.start < cursor or span.end <= span.start { continue }
      if best == null or span.start < (best ?? span).start or (span.start == (best ?? span).start and span.end > (best ?? span).end) { best = span }
    }
    if best == null { break }
    let span = best ?? {start: 0,end: 0}
    chunks += [line[cursor..span.start], b"\x1b[01;31m\x1b[K", line[span.start..span.end], b"\x1b[m\x1b[K"]
    cursor = span.end
  }
  bytes.concat(chunks + [line[cursor..]])
}

type Collected = {paths: List[Path], failed: Bool}

proc grep_paths(target: Path, opts: GrepOptions, root: Bool, ancestors: List[Str]) -> Result[Collected] {
  let meta = fs.stat(target, follow_symlinks: root or opts.recursive == 2)?
  if meta.kind == "symlink" { return Ok({paths: [], failed: false}) }
  if meta.kind != "dir" {
    let name = basename_bytes(target.bytes())
    var include = opts.includes.is_empty()
    for pattern in opts.includes { if glob(pattern,name) { include = true } }
    for pattern in opts.excludes { if glob(pattern,name) { include = false } }
    return Ok({paths: if include { [target] } else { [] }, failed: false})
  }
  for pattern in opts.exclude_dirs { if glob(pattern,basename_bytes(target.bytes())) { return Ok({paths: [], failed: false}) } }
  if opts.recursive == 0 { reject("Is a directory")? }
  let identity = f"{meta.dev}:{meta.ino}"
  if identity in ancestors { reject("recursive directory loop")? }
  var result: List[Path] = []
  var failed = false
  for entry in fs.children(target)? {
    let leaf = child(target, basename_bytes(entry.path.bytes()))?
    let found = grep_paths(leaf, opts, false, ancestors + [identity])
    match found {
      Ok(items) => { result += items.paths; failed = failed or items.failed }
      Err(failure) => { if ! opts.silent { gnu.error(f"{gnu.quote_bytes(leaf.bytes())}: {failure.message}") }; failed = true }
    }
  }
  Ok({paths: result, failed: failed})
}

# Standard input and regular files are scanned in bounded reads; only one
# record and the requested preceding context stay live between reads. Named
# nonseekable inputs use one read to completion because byte-at-offset reads
# reopen and seek the file, which cannot continue a FIFO or device stream.
proc grep_file(target: Path?, label: Bytes, show_name: Bool, opts: GrepOptions) -> Result[Bool] {
  var source_size = -1
  if target != null {
    let meta = fs.stat(target ?? p"", follow_symlinks: true)?
    source_size = if meta.kind == "file" and meta.size > 0 { meta.size } else { -2 }
  }
  var pending = b""
  var offset = 0
  var line_number = 0
  var record_offset = 0
  var selected = 0
  var previous: List[Bytes] = []
  var last_printed = 0
  var remaining = 0
  var binary = false
  if opts.limit == 0 {
    if opts.count and opts.list == 0 and ! opts.quiet { grep_write(bytes.concat([if show_name { bytes.concat([label,b":"]) } else { b"" },b"0\n"])) }
    if opts.list == -1 { grep_write(bytes.concat([label,if opts.null_names { b"\0" } else { b"\n" }])) }
    return Ok(opts.list == -1)
  }
  while true {
    let chunk = if target == null { io.stdin_read(65536)? } else if source_size == -2 {
      if offset == 0 { (target ?? p"").read_bytes()? } else { b"" }
    } else if offset >= source_size { b"" } else {
      let left = source_size - offset
      bytes.read_at(target ?? p"", offset, if left < 65536 { left } else { 65536 })?
    }
    offset += chunk.len()
    let eof = chunk.is_empty()
    pending = bytes.concat([pending, chunk])
    if opts.delimiter != 0 and find_fixed(pending,b"\0") != null { binary = true }
    if binary and opts.binary == "without-match" { return Ok(false) }
    var start = 0
    var lines: List[Bytes] = []
    for index in range(pending.len()) {
      if pending.byte_at(index) == opts.delimiter { lines += [pending[start..index]]; start = index + 1 }
    }
    if eof and start < pending.len() { lines += [pending[start..]]; start = pending.len() }
    pending = pending[start..]
    for line in lines {
      line_number += 1
      let line_offset = record_offset
      record_offset += line.len() + 1
      if opts.delimiter != 0 and (find_fixed(line, b"\0") != null or line.utf8() is Err(_)) { binary = true }
      if binary and opts.binary == "without-match" { return Ok(false) }
      let spans = grep_spans(line, opts)?
      let matched = ! spans.is_empty() != opts.invert
      if matched {
        selected += 1
        if opts.quiet { return Ok(true) }
        if opts.list == 1 { grep_write(bytes.concat([label,if opts.null_names { b"\0" } else { b"\n" }])); return Ok(true) }
        if binary and opts.binary == "binary" and ! opts.count and opts.list == 0 {
          grep_write(bytes.concat([b"Binary file ",label,b" matches\n"]))
          return Ok(true)
        }
      }
      if ! opts.count and opts.list == 0 and ! opts.quiet {
        if matched or remaining > 0 {
          if matched and opts.before > 0 {
            let first = line_number - previous.len()
            if last_printed > 0 and first > last_printed + 1 { grep_write(b"--\n") }
            for index in range(previous.len()) {
              let number = first + index
              if number > last_printed {
                var prefix = if show_name { bytes.concat([label,if opts.null_names { b"\0" } else { b"-" }]) } else { b"" }
                if opts.numbered { prefix = bytes.concat([prefix,bytes.from_text(f"{number}-")]) }
                if opts.byte_offset {
                  var prior_offset = line_offset
                  for prior in previous[index..] { prior_offset -= prior.len() + 1 }
                  prefix = bytes.concat([prefix,bytes.from_text(f"{prior_offset}-")])
                }
                grep_write(bytes.concat([prefix,previous[index],if opts.delimiter == 0 { b"\0" } else { b"\n" }]))
                last_printed = number
              }
            }
          }
          if matched and opts.after > 0 and opts.before == 0 and last_printed > 0 and line_number > last_printed + 1 { grep_write(b"--\n") }
          let separator = if matched { b":" } else { b"-" }
          var prefix = if show_name { bytes.concat([label,if opts.null_names { b"\0" } else { separator }]) } else { b"" }
          if opts.numbered { prefix = bytes.concat([prefix,bytes.from_text(f"{line_number}"),separator]) }
          if opts.only {
            var cursor = 0
            while cursor <= line.len() {
              var best: Span? = null
              for span in spans {
                if span.start < cursor or span.end <= span.start { continue }
                if best == null or span.start < (best ?? span).start or (span.start == (best ?? span).start and span.end > (best ?? span).end) { best = span }
              }
              if best == null { break }
              let chosen = best ?? {start: 0, end: 0}
              let match_prefix = if opts.byte_offset { bytes.concat([prefix,bytes.from_text(f"{line_offset + chosen.start}:")]) } else { prefix }
              grep_write(bytes.concat([match_prefix,colored(line[chosen.start..chosen.end],[{start:0,end:chosen.end - chosen.start}],opts.color),if opts.delimiter == 0 { b"\0" } else { b"\n" }]))
              cursor = chosen.end
            }
          } else {
            if opts.byte_offset { prefix = bytes.concat([prefix,bytes.from_text(f"{line_offset}"),separator]) }
            grep_write(bytes.concat([prefix,colored(line,spans,opts.color and matched and ! opts.invert),if opts.delimiter == 0 { b"\0" } else { b"\n" }]))
          }
          last_printed = line_number
        }
        if matched { remaining = opts.after } else if remaining > 0 { remaining -= 1 }
        previous += [line]
        if previous.len() > opts.before { previous = previous[previous.len() - opts.before..] }
      }
      if opts.limit >= 0 and selected >= opts.limit { break }
    }
    if eof or (opts.limit >= 0 and selected >= opts.limit) { break }
  }
  if opts.count and opts.list == 0 and ! opts.quiet { grep_write(bytes.concat([if show_name { bytes.concat([label,b":"]) } else { b"" },bytes.from_text(f"{selected}\n")])) }
  if opts.list == -1 and selected == 0 { grep_write(bytes.concat([label,if opts.null_names { b"\0" } else { b"\n" }])) }
  # With -L the status reports whether the file was listed, the inverse of selection.
  Ok(if opts.list == -1 { selected == 0 } else { selected > 0 })
}

## Search byte records using POSIX basic/extended or literal matching.
export proc grep(args: List[Str], mode: Str) [fs, io, process, env, error] -> Unit {
  var opts: GrepOptions = {mode: mode, color: false, insensitive: false, whole: false, words: false, invert: false, numbered: false, byte_offset: false, null_names: false, silent: false, names: 0, count: false, list: 0, quiet: false, only: false, delimiter: 10, binary: "binary", before: 0, after: 0, limit: -1, recursive: 0, includes: [], excludes: [], exclude_dirs: [], patterns: []}
  var operands: List[Str] = []
  var explicit = false
  var positional = false
  var at = 0
  while at < args.len() {
    let arg = args[at]; at += 1
    if arg == "--" and ! positional { positional = true; continue }
    if positional or ! arg.starts_with("-") or arg == "-" { operands += [arg]; continue }
    var flags: List[Str] = []
    var value: Str? = null
    if arg.starts_with("--") {
      let parts = arg.byte_slice(2).split("="); flags = [parts[0]]
      if parts.len() > 1 { value = parts[1..].join("=") }
    } else {
      var index = 1
      while index < arg.byte_len() {
        let flag = arg.byte_slice(index,1); index += 1; flags += [flag]
        if flag in ["e","f","A","B","C","m"] { if index < arg.byte_len() { value = arg.byte_slice(index) }; break }
      }
    }
    for flag in flags {
      if arg.starts_with("--") and value != null and flag not in ["regexp","file","after-context","before-context","context","max-count","binary-files","color","colour","include","exclude","exclude-dir","exclude-from"] { gnu.error(f"option {gnu.quote(arg)} does not take an argument"); exit 2 }
      if flag in ["e","regexp","f","file","A","after-context","B","before-context","C","context","m","max-count","binary-files","color","colour","include","exclude","exclude-dir","exclude-from"] and value == null {
        if flag in ["color","colour"] { value = "auto" } else {
          if at >= args.len() { gnu.error("option requires an argument"); exit 2 }; value = args[at]; at += 1
        }
      }
      let val = value ?? ""
      match flag {
        "E" | "extended-regexp" => opts.mode = "extended"
        "G" | "basic-regexp" => opts.mode = "basic"
        "F" | "fixed-strings" => opts.mode = "fixed"
        "i" | "ignore-case" => opts.insensitive = true
        "no-ignore-case" => opts.insensitive = false
        "v" | "invert-match" => opts.invert = true
        "w" | "word-regexp" => opts.words = true
        "x" | "line-regexp" => opts.whole = true
        "n" | "line-number" => opts.numbered = true
        "b" | "byte-offset" => opts.byte_offset = true
        "Z" | "null" => opts.null_names = true
        "s" | "no-messages" => opts.silent = true
        "H" | "with-filename" => opts.names = 1
        "h" | "no-filename" => opts.names = -1
        "c" | "count" => opts.count = true
        "l" | "files-with-matches" => opts.list = 1
        "L" | "files-without-match" => opts.list = -1
        "q" | "quiet" | "silent" => opts.quiet = true
        "o" | "only-matching" => opts.only = true
        "z" | "null-data" => opts.delimiter = 0
        "a" | "text" => opts.binary = "text"
        "I" => opts.binary = "without-match"
        "r" | "recursive" => opts.recursive = 1
        "R" | "dereference-recursive" => opts.recursive = 2
        "e" | "regexp" => { explicit = true; opts.patterns += val.split("\n") }
        "f" | "file" => {
          explicit = true
          let loaded = try { if val == "-" { io.stdin_bytes()? } else { fp"{val}".read_bytes()? } }
          if let Err(failure) = loaded { gnu.error(failure.message); exit 2 }
          for record in records(loaded ?? b"", 10) { let decoded = record.utf8()
            if let Err(failure) = decoded { gnu.error(failure.message); exit 2 }
            opts.patterns += [decoded ?? ""] }
        }
        "A" | "after-context" | "B" | "before-context" | "C" | "context" | "m" | "max-count" => {
          let number = val.parse_int() ?? -1
          if number < 0 { gnu.error("invalid numeric argument"); exit 2 }
          if flag in ["A","after-context","C","context"] { opts.after = number }
          if flag in ["B","before-context","C","context"] { opts.before = number }
          if flag in ["m","max-count"] { opts.limit = number }
        }
        "include" => opts.includes += [val]
        "exclude" => opts.excludes += [val]
        "exclude-dir" => opts.exclude_dirs += [val]
        "exclude-from" => {
          let loaded = try { fp"{val}".read_bytes()? }
          if let Err(failure) = loaded { gnu.error(failure.message); exit 2 }
          for record in records(loaded ?? b"",10) {
            let decoded = record.utf8()
            if let Err(failure) = decoded { gnu.error(failure.message); exit 2 }
            opts.excludes += [decoded ?? ""]
          }
        }
        "binary-files" => { if val not in ["text","binary","without-match"] { gnu.error("invalid binary-files value"); exit 2 }; opts.binary = val }
        "color" | "colour" => { if val not in ["never","auto","always"] { gnu.error("invalid color value"); exit 2 }; opts.color = val == "always" or (val == "auto" and unix.isatty(1)) }
        "help" => { gnu.help("Usage: grep [OPTION]... PATTERNS [FILE]...\nSearch for patterns in byte records."); return }
        "version" => { gnu.version(gnu.prog()); return }
        _ => { gnu.error(f"unsupported option {gnu.quote(arg)}"); exit 2 }
      }
    }
  }
  if ! explicit {
    if operands.is_empty() { gnu.error("missing pattern"); exit 2 }
    opts.patterns = operands[0].split("\n"); operands = operands[1..]
  }
  if opts.mode != "fixed" {
    for pattern in opts.patterns {
      if let Err(failure) = regex.find_bytes(pattern, b"", extended: opts.mode == "extended", ignore_case: opts.insensitive) { gnu.error(failure.message); exit 2 }
    }
  }
  if operands.is_empty() { operands = [if opts.recursive > 0 { "." } else { "-" }] }
  var paths: List[Path?] = []
  var failed = false
  for name in operands {
    if name == "-" { paths += [null]; continue }
    let found = grep_paths(fp"{name}", opts, true, [])
    match found { Ok(items) => { for item in items.paths { paths += [item] }; failed = failed or items.failed }; Err(failure) => { if ! opts.silent { gnu.error(f"{name}: {failure.message}") }; failed = true } }
  }
  var matched = false
  for target in paths {
    let label = if target == null { b"(standard input)" } else { (target ?? p"").bytes() }
    let result = grep_file(target, label, opts.names == 1 or (opts.names == 0 and (operands.len() > 1 or opts.recursive > 0)), opts)
    match result { Ok(selected) => matched = matched or selected; Err(failure) => { if ! opts.silent { gnu.error(f"{gnu.quote_bytes(label)}: {failure.message}") }; failed = true } }
    if matched and opts.quiet { exit 0 }
  }
  if let Err(failure) = io.flush_stdout() { gnu.error(failure.message); exit 2 }
  exit if failed { 2 } else if matched { 0 } else { 1 }
}
