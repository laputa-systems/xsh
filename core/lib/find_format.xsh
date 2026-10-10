##! Output formats of the find applet: `-printf` formats, `-ls` lines and the
##! time notations they share.
##!
##! A `-printf` format is compiled once into segments, so escape warnings and
##! format errors appear before any file is visited, as with GNU find.
use gnu
use search

const NANOS = 1000000000

## Metadata of a visited entry as the traversal observed it: through the
## symlink when the traversal follows links, otherwise the link itself.
export type Meta = {kind: Str, size: Int, uid: Int, gid: Int, ino: Int, dev: Int, nlink: Int, mode: Int, atime_ns: Int, mtime_ns: Int, ctime_ns: Int, birth_ns: Int?, blocks_512: Int}

## A visited entry: its path as built from the starting point, the starting
## point it was reached from, and its depth below that point.
export type Entry = {path: Path, start: Path, depth: Int, meta: Meta}

## One piece of a compiled format. `kind` is `text`, `directive` or `stop`
## (the `\c` escape, which ends the output of the whole format).
export type Segment = {kind: Str, text: Bytes, flags: Str, width: Int, precision: Int, spec: Str, sub: Str}

pure spaces(count: Int) -> Bytes {
  var parts: List[Bytes] = []
  for _ in range(count) { parts += [b" "] }
  bytes.concat(parts)
}

# Apply the width, precision and left-justify flag of a `%s` conversion.
pure pad_text(data: Bytes, flags: Str, width: Int, precision: Int) -> Bytes {
  let clipped = if precision >= 0 and precision < data.len() { data[0..precision] } else { data }
  if width <= clipped.len() { return clipped }
  if "-" in flags { return bytes.concat([clipped, spaces(width - clipped.len())]) }
  bytes.concat([spaces(width - clipped.len()), clipped])
}

pure zeros(count: Int) -> Str {
  var out = ""
  for _ in range(count) { out = f"{out}0" }
  out
}

# Apply the flags, width and precision of an integer conversion: the digits are
# padded to the precision first, a sign or leading zero is added, and the zero
# flag only pads when no precision was given.
pure pad_number(digits: Str, negative: Bool, flags: Str, width: Int, precision: Int, prefix: Str) -> Bytes {
  var body = digits
  if precision >= 0 and body.byte_len() < precision { body = f"{zeros(precision - body.byte_len())}{body}" }
  if precision == 0 and digits == "0" { body = "" }
  let sign = if negative { "-" } else if "+" in flags { "+" } else if " " in flags { " " } else { "" }
  let head = f"{sign}{prefix}"
  let used = head.byte_len() + body.byte_len()
  if width <= used { return bytes.from_text(f"{head}{body}") }
  if "-" in flags { return bytes.concat([bytes.from_text(f"{head}{body}"), spaces(width - used)]) }
  if "0" in flags and precision < 0 { return bytes.from_text(f"{head}{zeros(width - used)}{body}") }
  bytes.concat([spaces(width - used), bytes.from_text(f"{head}{body}")])
}

pure octal_digits(value: Int) -> Str {
  if value == 0 { return "0" }
  var out = ""
  var rest = value
  while rest > 0 { out = f"{rest % 8}{out}"; rest = rest / 8 }
  out
}

pure is_octal(byte: Int) -> Bool { byte >= 48 and byte <= 55 }

## Compile a `-printf` format. Unknown escapes and directives warn once and
## are copied through, matching GNU; a lone trailing `%` or `\` is an error.
export proc compile(format: Bytes) [process, env, error] -> Result[List[Segment], Error] {
  var out: List[Segment] = []
  var text: List[Bytes] = []
  var at = 0
  while at < format.len() {
    let byte = format.byte_at(at) ?? 0
    if byte == 92 {
      if at + 1 >= format.len() {
        eprint f"{gnu.prog()}: warning: escape `\\' followed by nothing at all"
        text += [b"\\"]
        at += 1
        continue
      }
      let next = format.byte_at(at + 1) ?? 0
      at += 2
      let simple = match next { 97 => 7; 98 => 8; 102 => 12; 110 => 10; 114 => 13; 116 => 9; 118 => 11; 92 => 92; _ => -1 }
      if simple >= 0 { text += [bytes.from_ints([simple]) ?? b""]; continue }
      if next == 99 {
        if ! text.is_empty() { out += [{kind: "text", text: bytes.concat(text), flags: "", width: 0, precision: -1, spec: "", sub: ""}]; text = [] }
        out += [{kind: "stop", text: b"", flags: "", width: 0, precision: -1, spec: "", sub: ""}]
        return Ok(out)
      }
      if is_octal(next) {
        var value = next - 48
        var digits = 1
        while digits < 3 and at < format.len() and is_octal(format.byte_at(at) ?? 0) {
          value = value * 8 + (format.byte_at(at) ?? 0) - 48; at += 1; digits += 1
        }
        text += [bytes.from_ints([value % 256]) ?? b""]
        continue
      }
      eprint f"{gnu.prog()}: warning: unrecognized escape `\\{(bytes.from_ints([next]) ?? b"").utf8() ?? "?"}'"
      text += [bytes.from_ints([92, next]) ?? b""]
      continue
    }
    if byte != 37 { text += [format[at..at + 1]]; at += 1; continue }
    let start = at
    at += 1
    if at >= format.len() { search.reject("error: % at end of format string")? }
    if format.byte_at(at) == 37 { text += [b"%"]; at += 1; continue }
    var flags = ""
    while at < format.len() and (format.byte_at(at) ?? 0) in [45, 43, 32, 35, 48] {
      flags = f"{flags}{(bytes.from_ints([format.byte_at(at) ?? 0]) ?? b"").utf8() ?? ""}"; at += 1
    }
    var width = 0
    while at < format.len() and (format.byte_at(at) ?? 0) >= 48 and (format.byte_at(at) ?? 0) <= 57 {
      width = width * 10 + (format.byte_at(at) ?? 48) - 48; at += 1
    }
    var precision = -1
    if at < format.len() and format.byte_at(at) == 46 {
      at += 1
      precision = 0
      while at < format.len() and (format.byte_at(at) ?? 0) >= 48 and (format.byte_at(at) ?? 0) <= 57 {
        precision = precision * 10 + (format.byte_at(at) ?? 48) - 48; at += 1
      }
    }
    if at >= format.len() { search.reject("error: % at end of format string")? }
    let spec = (bytes.from_ints([format.byte_at(at) ?? 0]) ?? b"").utf8() ?? "?"
    at += 1
    var sub = ""
    if spec in ["A", "B", "C", "T"] {
      if at >= format.len() { search.reject(f"error: %{spec} at end of format string")? }
      sub = (bytes.from_ints([format.byte_at(at) ?? 0]) ?? b"").utf8() ?? "?"
      at += 1
    }
    if spec not in ["a", "A", "b", "B", "c", "C", "d", "D", "f", "F", "g", "G", "h", "H", "i", "k", "l", "m", "M", "n", "p", "P", "s", "S", "t", "T", "u", "U", "y", "Y"] {
      eprint f"{gnu.prog()}: warning: unrecognized format directive `%{spec}'"
      text += [format[start..at]]
      continue
    }
    if ! text.is_empty() { out += [{kind: "text", text: bytes.concat(text), flags: "", width: 0, precision: -1, spec: "", sub: ""}]; text = [] }
    out += [{kind: "directive", text: b"", flags: flags, width: width, precision: precision, spec: spec, sub: sub}]
  }
  if ! text.is_empty() { out += [{kind: "text", text: bytes.concat(text), flags: "", width: 0, precision: -1, spec: "", sub: ""}] }
  Ok(out)
}

pure basename_of(raw: Bytes) -> Bytes {
  var end = raw.len()
  while end > 1 and raw.byte_at(end - 1) == 47 { end -= 1 }
  if end == 1 and raw.byte_at(0) == 47 { return b"/" }
  var at = end
  while at > 0 and raw.byte_at(at - 1) != 47 { at -= 1 }
  # Trailing slashes belong to the name: `dir/` has base `dir/`.
  raw[at..]
}

# The directory part of a path: everything before the last component, without
# trailing slashes; `.` when the path has no directory part, empty for `/`.
pure dirname_of(raw: Bytes) -> Bytes {
  var end = raw.len()
  while end > 1 and raw.byte_at(end - 1) == 47 { end -= 1 }
  var at = end
  while at > 0 and raw.byte_at(at - 1) != 47 { at -= 1 }
  if end == 1 and raw.byte_at(0) == 47 { return b"" }
  if at == 0 { return b"." }
  var stop = at
  while stop > 1 and raw.byte_at(stop - 1) == 47 { stop -= 1 }
  raw[0..stop]
}

# The path below the starting point, without the separating slash.
pure relative_part(whole: Bytes, start: Bytes) -> Bytes {
  if whole.len() <= start.len() { return b"" }
  var at = start.len()
  while at < whole.len() and whole.byte_at(at) == 47 { at += 1 }
  whole[at..]
}

pure type_letter(kind: Str) -> Str {
  match kind { "file" => "f"; "dir" => "d"; "symlink" => "l"; "fifo" => "p"; "socket" => "s"; "block" => "b"; "char" => "c"; _ => "U" }
}

## The `ls -l` style permission string of a mode.
export pure mode_text(kind: Str, mode: Int) -> Str {
  let lead = match kind { "dir" => "d"; "symlink" => "l"; "fifo" => "p"; "socket" => "s"; "block" => "b"; "char" => "c"; _ => "-" }
  var out = lead
  for shift in [6, 3, 0] {
    let bits = mode / (if shift == 6 { 64 } else if shift == 3 { 8 } else { 1 }) % 8
    out = f"{out}{if bits >= 4 { "r" } else { "-" }}{if bits % 4 >= 2 { "w" } else { "-" }}"
    let execute = bits % 2 == 1
    let special = if shift == 6 { mode.bit_and(0o4000) != 0 } else if shift == 3 { mode.bit_and(0o2000) != 0 } else { mode.bit_and(0o1000) != 0 }
    let letter = if shift == 0 { "t" } else { "s" }
    if special { out = f"{out}{if execute { letter } else { letter.upper() }}" } else { out = f"{out}{if execute { "x" } else { "-" }}" }
  }
  out
}

pure two(value: Int) -> Str { if value < 10 { f"0{value}" } else { f"{value}" } }

# Whole-second part of a nanosecond timestamp, rounding toward negative infinity.
pure seconds_of(ns: Int) -> Int {
  if ns >= 0 { ns / NANOS } else { 0 - (0 - ns + NANOS - 1) / NANOS }
}

pure nanos_of(ns: Int) -> Int {
  ns - seconds_of(ns) * NANOS
}

# Ten fractional digits, like GNU: nine digits of nanoseconds then a zero.
pure fraction(ns: Int) -> Str {
  let value = f"{nanos_of(ns)}"
  f"{zeros(9 - value.byte_len())}{value}0"
}

proc strftime(ns: Int, format: Str) [time, error] -> Result[Str, Error] {
  time.format(ns, format)
}

# `%A`, `%C` and `%T` followed by a one-character time conversion.
proc time_field(ns: Int, sub: Str) [time, error] -> Result[Str, Error] {
  match sub {
    "@" => Ok(f"{seconds_of(ns)}.{fraction(ns)}")
    "S" => Ok(f"{strftime(ns, "%S")?}.{fraction(ns)}")
    "T" | "X" => Ok(f"{strftime(ns, "%H:%M:")?}{strftime(ns, "%S")?}.{fraction(ns)}")
    "+" => Ok(f"{strftime(ns, "%Y-%m-%d+%H:%M:")?}{strftime(ns, "%S")?}.{fraction(ns)}")
    "k" | "l" | "N" => { search.reject("memory exhausted")?; Ok("") }
    _ => strftime(ns, f"%{sub}")
  }
}

# The `%a`, `%c` and `%t` notation: ctime(3) with fractional seconds.
proc ctime_field(ns: Int) [time, error] -> Result[Str, Error] {
  Ok(f"{strftime(ns, "%a %b %e %H:%M:")?}{strftime(ns, "%S")?}.{fraction(ns)} {strftime(ns, "%Y")?}")
}

# Link text of a symlink entry, empty for anything else.
proc link_text(entry: Entry) [fs] -> Bytes {
  if entry.meta.kind != "symlink" { return b"" }
  match entry.path.readlink() { Ok(target) => target.bytes(); Err(_) => b"" }
}

# Type letter of what a symlink points at: N dangling, L loop, ? other error.
proc followed_type(entry: Entry) [fs] -> Str {
  if entry.meta.kind != "symlink" { return type_letter(entry.meta.kind) }
  match fs.stat(entry.path, follow_symlinks: true) {
    Ok(found) => type_letter(found.kind)
    Err(failure) => if gnu.errno(failure) == 2 { "N" } else if gnu.errno(failure) == 40 { "L" } else { "?" }
  }
}

proc owner_name(uid: Int) [fs] -> Str {
  match user.by_uid(uid) { Ok(found) => found.name; Err(_) => f"{uid}" }
}

proc group_name(gid: Int) [fs] -> Str {
  match group.by_gid(gid) { Ok(found) => found.name; Err(_) => f"{gid}" }
}

proc fstype_name(entry: Entry) [fs] -> Str {
  match fs.mount_for(entry.path) { Ok(found) => found.fstype; Err(_) => "unknown" }
}

# Floating `%S`: allocated bytes over size, in the shortest `%g` form.
pure sparseness(entry: Entry) -> Str {
  if entry.meta.size == 0 { return "1" }
  let ratio = entry.meta.blocks_512 * 512 * 1000000 / entry.meta.size
  let whole = ratio / 1000000
  var fractional = ratio % 1000000
  if fractional == 0 { return f"{whole}" }
  var digits = f"{fractional}"
  digits = f"{zeros(6 - digits.byte_len())}{digits}"
  while digits.ends_with("0") { digits = digits.byte_slice(0, digits.byte_len() - 1) }
  f"{whole}.{digits}"
}

# Expand one directive for an entry.
proc directive_text(entry: Entry, segment: Segment) [fs, time, error] -> Result[Bytes, Error] {
  let meta = entry.meta
  let raw = entry.path.bytes()
  let flags = segment.flags
  let width = segment.width
  let precision = segment.precision
  match segment.spec {
    "d" => Ok(pad_number(f"{entry.depth}", false, flags, width, precision, ""))
    "m" => Ok(pad_number(octal_digits(meta.mode.bit_and(0o7777)), false, flags, width, precision, if "#" in flags and meta.mode.bit_and(0o7777) != 0 { "0" } else { "" }))
    "S" => Ok(pad_text(bytes.from_text(sparseness(entry)), flags, width, precision))
    "p" => Ok(pad_text(raw, flags, width, precision))
    "f" => Ok(pad_text(basename_of(raw), flags, width, precision))
    "h" => Ok(pad_text(dirname_of(raw), flags, width, precision))
    "H" => Ok(pad_text(entry.start.bytes(), flags, width, precision))
    "P" => Ok(pad_text(relative_part(raw, entry.start.bytes()), flags, width, precision))
    "l" => Ok(pad_text(link_text(entry), flags, width, precision))
    "y" => Ok(pad_text(bytes.from_text(type_letter(meta.kind)), flags, width, precision))
    "Y" => Ok(pad_text(bytes.from_text(followed_type(entry)), flags, width, precision))
    "s" => Ok(pad_text(bytes.from_text(f"{meta.size}"), flags, width, precision))
    "M" => Ok(pad_text(bytes.from_text(mode_text(meta.kind, meta.mode)), flags, width, precision))
    "n" => Ok(pad_text(bytes.from_text(f"{meta.nlink}"), flags, width, precision))
    "i" => Ok(pad_text(bytes.from_text(f"{meta.ino}"), flags, width, precision))
    "D" => Ok(pad_text(bytes.from_text(f"{meta.dev}"), flags, width, precision))
    "b" => Ok(pad_text(bytes.from_text(f"{meta.blocks_512}"), flags, width, precision))
    "k" => Ok(pad_text(bytes.from_text(f"{(meta.blocks_512 + 1) / 2}"), flags, width, precision))
    "u" => Ok(pad_text(bytes.from_text(owner_name(meta.uid)), flags, width, precision))
    "U" => Ok(pad_text(bytes.from_text(f"{meta.uid}"), flags, width, precision))
    "g" => Ok(pad_text(bytes.from_text(group_name(meta.gid)), flags, width, precision))
    "G" => Ok(pad_text(bytes.from_text(f"{meta.gid}"), flags, width, precision))
    "F" => Ok(pad_text(bytes.from_text(fstype_name(entry)), flags, width, precision))
    "a" => Ok(pad_text(bytes.from_text(ctime_field(meta.atime_ns)?), flags, width, precision))
    "c" => Ok(pad_text(bytes.from_text(ctime_field(meta.ctime_ns)?), flags, width, precision))
    "t" => Ok(pad_text(bytes.from_text(ctime_field(meta.mtime_ns)?), flags, width, precision))
    "A" => Ok(pad_text(bytes.from_text(time_field(meta.atime_ns, segment.sub)?), flags, width, precision))
    "C" => Ok(pad_text(bytes.from_text(time_field(meta.ctime_ns, segment.sub)?), flags, width, precision))
    "B" => Ok(pad_text(if meta.birth_ns == null { b"" } else { bytes.from_text(time_field(meta.birth_ns ?? 0, segment.sub)?) }, flags, width, precision))
    _ => Ok(pad_text(bytes.from_text(time_field(meta.mtime_ns, segment.sub)?), flags, width, precision))
  }
}

## Whether expanding the format needs more than the entry's type and name.
export pure needs_stat(segments: List[Segment]) -> Bool {
  for segment in segments {
    if segment.kind == "directive" and segment.spec not in ["p", "f", "h", "H", "P", "d", "y", "l", "Y"] { return true }
  }
  false
}

## Expand a compiled format for an entry.
export proc render(segments: List[Segment], entry: Entry) [fs, time, error] -> Result[Bytes, Error] {
  var parts: List[Bytes] = []
  for segment in segments {
    if segment.kind == "stop" { break }
    if segment.kind == "text" { parts += [segment.text]; continue }
    parts += [directive_text(entry, segment)?]
  }
  Ok(bytes.concat(parts))
}

# `-ls` names print with C escapes for control bytes and backslash, and as
# octal for bytes outside printable ASCII.
pure escape_name(raw: Bytes) -> Bytes {
  var parts: List[Bytes] = []
  for at in range(raw.len()) {
    let byte = raw.byte_at(at) ?? 0
    let simple = match byte { 7 => "\\a"; 8 => "\\b"; 9 => "\\t"; 10 => "\\n"; 11 => "\\v"; 12 => "\\f"; 13 => "\\r"; 92 => "\\\\"; _ => "" }
    if simple != "" { parts += [bytes.from_text(simple)] } else if byte < 32 or byte >= 127 {
      parts += [bytes.from_text(f"\\{byte / 64}{byte / 8 % 8}{byte % 8}")]
    } else { parts += [raw[at..at + 1]] }
  }
  bytes.concat(parts)
}

pure left_pad(text: Str, width: Int) -> Str {
  if text.byte_len() >= width { text } else { f"{zeros(width - text.byte_len()).replace("0", with: " ")}{text}" }
}

pure right_pad(text: Str, width: Int) -> Str {
  if text.byte_len() >= width { text } else { f"{text}{zeros(width - text.byte_len()).replace("0", with: " ")}" }
}

# Entries older than six months, or dated in the future, show the year.
proc ls_time(mtime_ns: Int, now_ns: Int) [time, error] -> Result[Str, Error] {
  let age = seconds_of(now_ns) - seconds_of(mtime_ns)
  if age > 15778476 or age < -15778476 { return Ok(f"{strftime(mtime_ns, "%b %e")?}  {strftime(mtime_ns, "%Y")?}") }
  strftime(mtime_ns, "%b %e %H:%M")
}

## One `-ls` line, including the newline.
export proc ls_line(entry: Entry, now_ns: Int) [fs, time, error] -> Result[Bytes, Error] {
  let meta = entry.meta
  let size = if meta.kind in ["block", "char"] { f"{left_pad(f"{fs.dev_major(meta.dev)}", 3)}, {left_pad(f"{fs.dev_minor(meta.dev)}", 3)}" } else { left_pad(f"{meta.size}", 8) }
  let head = f"{left_pad(f"{meta.ino}", 9)} {left_pad(f"{(meta.blocks_512 + 1) / 2}", 6)} {mode_text(meta.kind, meta.mode)} {left_pad(f"{meta.nlink}", 3)} {right_pad(owner_name(meta.uid), 8)} {right_pad(group_name(meta.gid), 8)} {size} {ls_time(meta.mtime_ns, now_ns)?} "
  var parts: List[Bytes] = [bytes.from_text(head), escape_name(entry.path.bytes())]
  if meta.kind == "symlink" { parts += [b" -> ", escape_name(link_text(entry))] }
  parts += [b"\n"]
  Ok(bytes.concat(parts))
}
