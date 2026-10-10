#!/bin/xsh
use lib.gnu

# Every tar failure ends the applet with status 1.
error TarError = Invalid(message: Str)

const BLOCK = 512
const NAME_FIELD = 100
const PREFIX_FIELD = 155
const NANOS = 1000000000
const ZERO_BLOCKS = 1024
const TYPE_LONG_NAME = 76
const TYPE_LONG_LINK = 75
const TYPE_PAX = 120
const TYPE_GLOBAL_PAX = 103

const USAGE = """Usage: tar {c|t|x}[vOkozjJ]f ARCHIVE [-C DIR] [-X FILE] [-T FILE] [FILE...]
Create, list or extract a tar archive. A missing ARCHIVE, or '-', uses standard
input for reading and standard output for writing.

  c           create an archive from FILE...
  t           list the archive members
  x           extract the archive members
  -f ARCHIVE  archive name, or '-' for standard input or output
  -C DIR      change to DIR before creating or extracting
  -v          verbose member names (a long listing with t)
  -O          extract regular files to standard output
  -k          keep existing files instead of failing or replacing them
  -o          accepted; owners are never restored
  -z          filter the archive through gzip
  -j          filter the archive through bzip2
  -J          filter the archive through xz
  -X FILE     exclude the members named in FILE, one per line
  -T FILE     read member names from FILE, one per line
      --overwrite                 replace existing files when extracting
      --strip-components=NUMBER   strip NUMBER leading path components
      --help                      display this help and exit
      --version                   output version information and exit
"""

type Options = {
  mode: Str,
  verbose: Bool,
  archive_path: Str?,
  root: Str,
  compression: Str,
  to_stdout: Bool,
  keep: Bool,
  overwrite: Bool,
  strip: Int,
  excludes: List[Str],
  members: List[Str],
  help: Bool,
  version: Bool,
}

# Member kinds: file, dir, symlink, hardlink, fifo, chardev, blockdev, or
# unsupported (any other typeflag). Only file and unsupported members carry data.
type Member = {
  name: Str,
  kind: Str,
  link: Str,
  mode: Int,
  uid: Int,
  gid: Int,
  uname: Str,
  gname: Str,
  size: Int,
  mtime: Int,
  data: Int,
}

type PaxOverrides = {path: Str?, linkpath: Str?}
type Fixup = {path: Path, mode: Int, mtime: Int}
type SplitName = {name: Str, prefix: Str}

# A field ends at its first NUL, or at the end of the field when it fills it.
pure field_end(block: Bytes, start: Int, length: Int) -> Int {
  var position = start
  while position < start + length and (block.byte_at(position) ?? 0) != 0 { position += 1 }
  position
}

pure text_field(block: Bytes, start: Int, length: Int) -> Result[Str] {
  let end = field_end(block, start, length)
  block.slice(start, end - start).utf8()
}

# Numeric fields are octal digits, optionally preceded by spaces and ended by NUL or a space.
pure octal_field(block: Bytes, start: Int, length: Int) -> Result[Int] {
  let end = start + length
  var position = start
  while position < end and (block.byte_at(position) ?? 0) == 32 { position += 1 }
  var value = 0
  while position < end {
    let digit = block.byte_at(position) ?? 0
    if digit == 0 or digit == 32 { break }
    if digit < 48 or digit > 55 { return Err(TarError.Invalid(message: "corrupted octal value in tar header")) }
    value = value * 8 + digit - 48
    position += 1
  }
  Ok(value)
}

pure is_zero_block(block: Bytes) -> Bool {
  for position in range(BLOCK) {
    if (block.byte_at(position) ?? 0) != 0 { return false }
  }
  true
}

# The checksum covers every header byte, with the checksum field itself counted as spaces.
pure header_sum(block: Bytes) -> Int {
  var total = 0
  for position in range(BLOCK) {
    total += if position >= 148 and position < 156 { 32 } else { block.byte_at(position) ?? 0 }
  }
  total
}

# Member data is stored in whole blocks.
pure padded(size: Int) -> Int {
  (size + BLOCK - 1) / BLOCK * BLOCK
}

# Extended records have the form "LEN KEY=VALUE\n", where LEN counts the whole record.
# Only path and linkpath change a member's name or link; other records are ignored.
pure pax_overrides(body: Bytes) -> Result[PaxOverrides] {
  let text = body.utf8()?
  var overrides: PaxOverrides = {path: null, linkpath: null}
  var position = 0
  while position < text.byte_len() {
    let space = text.find(" ", position) ?? -1
    if space < 0 { return Err(TarError.Invalid(message: "malformed extended header")) }
    let length = text.byte_slice(position, length: space - position).parse_int_decimal()?
    if length <= 0 or position + length > text.byte_len() { return Err(TarError.Invalid(message: "malformed extended header")) }
    let record = text.byte_slice(position, length: length)
    let assignment = record.byte_slice(space + 1 - position, length: length - (space + 1 - position) - 1)
    let equals = assignment.find("=") ?? -1
    if equals < 0 { return Err(TarError.Invalid(message: "malformed extended header")) }
    let key = assignment.byte_slice(0, length: equals)
    let value = assignment.byte_slice(equals + 1)
    if key == "path" {
      overrides = {path: value, linkpath: overrides.linkpath}
    } else if key == "linkpath" {
      overrides = {path: overrides.path, linkpath: value}
    }
    position += length
  }
  Ok(overrides)
}

# The ustar prefix is a directory part of the name; a separator joins it to the
# name unless the prefix already ends with one.
pure join_prefix(prefix: Str, base: Str) -> Str {
  var rest = base
  while rest.starts_with("/") { rest = rest.byte_slice(1) }
  if prefix.ends_with("/") { f"{prefix}{rest}" } else { f"{prefix}/{rest}" }
}

pure body_bytes(data: Bytes, start: Int, size: Int) -> Result[Bytes] {
  if size > data.len() - start { return Err(TarError.Invalid(message: "short read")) }
  Ok(data.slice(start, size))
}

pure body_text(data: Bytes, start: Int, size: Int) -> Result[Str] {
  let body = body_bytes(data, start, size)?
  text_field(body, 0, body.len())
}

# Parses every member of an uncompressed tar stream. An empty input is an error,
# a zero block ends the stream and a second zero block ends it for good; missing
# trailing blocks after the last member are accepted.
pure read_members(data: Bytes) -> Result[List[Member]] {
  var members: List[Member] = []
  var position = 0
  var after_zero = false
  var long_name: Str? = null
  var long_link: Str? = null
  var overrides: PaxOverrides = {path: null, linkpath: null}
  while true {
    let remaining = data.len() - position
    if remaining < 0 { break }
    if remaining == 0 {
      if position == 0 { return Err(TarError.Invalid(message: "short read")) }
      break
    }
    if remaining < BLOCK { return Err(TarError.Invalid(message: "short read")) }
    let block = data.slice(position, BLOCK)
    position += BLOCK
    if is_zero_block(block) {
      if after_zero { break }
      after_zero = true
      continue
    }
    after_zero = false
    if block.slice(257, 5) != b"ustar" { return Err(TarError.Invalid(message: "invalid tar magic")) }
    if octal_field(block, 148, 8)? != header_sum(block) { return Err(TarError.Invalid(message: "invalid tar header checksum")) }
    var flag = block.byte_at(156) ?? 0
    if flag == 0 { flag = 48 }
    let size = octal_field(block, 124, 12)?
    let mtime = octal_field(block, 136, 12)?
    if flag == TYPE_LONG_NAME or flag == TYPE_LONG_LINK or flag == TYPE_PAX or flag == TYPE_GLOBAL_PAX {
      if flag == TYPE_LONG_NAME {
        long_name = body_text(data, position, size)?
      } else if flag == TYPE_LONG_LINK {
        long_link = body_text(data, position, size)?
      } else if flag == TYPE_PAX {
        overrides = pax_overrides(body_bytes(data, position, size)?)?
      }
      position += padded(size)
      continue
    }
    let prefix = if block.slice(257, 5) == b"ustar" { text_field(block, 345, PREFIX_FIELD)? } else { "" }
    let base = text_field(block, 0, NAME_FIELD)?
    let joined = if prefix == "" { base } else { join_prefix(prefix, base) }
    let name = overrides.path ?? long_name ?? joined
    let link = overrides.linkpath ?? long_link ?? text_field(block, 157, NAME_FIELD)?
    let kind = if flag == 48 or flag == 55 {
      "file"
    } else if flag == 49 {
      "hardlink"
    } else if flag == 50 {
      "symlink"
    } else if flag == 51 {
      "chardev"
    } else if flag == 52 {
      "blockdev"
    } else if flag == 53 {
      "dir"
    } else if flag == 54 {
      "fifo"
    } else {
      "unsupported"
    }
    let has_data = kind == "file" or kind == "unsupported"
    let body_size = if has_data { size } else { 0 }
    if body_size > data.len() - position { return Err(TarError.Invalid(message: "short read")) }
    members += [{
      name: name,
      kind: kind,
      link: link,
      mode: octal_field(block, 100, 8)?,
      uid: octal_field(block, 108, 8)?,
      gid: octal_field(block, 116, 8)?,
      uname: text_field(block, 265, 32)?,
      gname: text_field(block, 297, 32)?,
      size: body_size,
      mtime: mtime,
      data: position,
    }]
    position += padded(body_size)
    long_name = null
    long_link = null
    overrides = {path: null, linkpath: null}
  }
  Ok(members)
}

# Member names compare without leading "/", leading "./" and trailing "/", so
# "./a/b", "a/b/" and "/a/b" name the same member. The empty name is the archive root.
pure canonical(name: Str) -> Str {
  var text = name
  while text.starts_with("/") { text = text.byte_slice(1) }
  while text.starts_with("./") { text = text.byte_slice(2) }
  while text.ends_with("/") { text = text.byte_slice(0, length: text.byte_len() - 1) }
  if text == "." { "" } else { text }
}

# A pattern names a member or a directory whose members it covers.
pure covers(name: Str, pattern: Str) -> Bool {
  let key = canonical(pattern)
  key == "" or name == key or name.starts_with(f"{key}/")
}

pure excluded(name: Str, excludes: List[Str]) -> Bool {
  for pattern in excludes {
    if covers(name, pattern) { return true }
  }
  false
}

pure selected(name: Str, members: List[Str], excludes: List[Str]) -> Bool {
  if ! members.is_empty() {
    var listed = false
    for pattern in members {
      if covers(name, pattern) { listed = true }
    }
    if ! listed { return false }
  }
  ! excluded(name, excludes)
}

# The first operand that names no member of the archive, or null when all are found.
pure first_missing(members: List[Member], operands: List[Str]) -> Str? {
  for operand in operands {
    var seen = false
    for member in members {
      if covers(canonical(member.name), operand) {
        seen = true
        break
      }
    }
    if ! seen { return operand }
  }
  null
}

pure strip_components(name: Str, count: Int) -> Str {
  var text = name
  var left = count
  while left > 0 {
    let slash = text.find("/") ?? -1
    if slash < 0 { return "" }
    text = text.byte_slice(slash + 1)
    left -= 1
  }
  text
}

pure member_path(root: Path, name: Str) -> Result[Path] {
  for part in name.split("/") {
    if part == ".." { return Err(TarError.Invalid(message: f"{name}: refusing to extract a member with '..' in its name")) }
  }
  if name == "" { Ok(root) } else { Ok(fp"{root}/{name}") }
}

pure type_char(kind: Str) -> Str {
  match kind {
    "dir" => "d"
    "symlink" => "l"
    "chardev" => "c"
    "blockdev" => "b"
    "fifo" => "p"
    _ => "-"
  }
}

pure permission(mode: Int, bit: Int, letter: Str) -> Str {
  if mode.bit_and(bit) != 0 { letter } else { "-" }
}

# Execute bits also carry the setuid, setgid and sticky markers.
pure execute_char(mode: Int, execute: Int, special: Int, special_set: Str, special_unset: Str) -> Str {
  let executable = mode.bit_and(execute) != 0
  if mode.bit_and(special) != 0 {
    if executable { special_set } else { special_unset }
  } else if executable {
    "x"
  } else {
    "-"
  }
}

pure mode_text(kind: Str, mode: Int) -> Str {
  let owner_bits = permission(mode, 0o400, "r") + permission(mode, 0o200, "w") + execute_char(mode, 0o100, 0o4000, "s", "S")
  let group_bits = permission(mode, 0o040, "r") + permission(mode, 0o020, "w") + execute_char(mode, 0o010, 0o2000, "s", "S")
  let other_bits = permission(mode, 0o004, "r") + permission(mode, 0o002, "w") + execute_char(mode, 0o001, 0o1000, "t", "T")
  f"{type_char(kind)}{owner_bits}{group_bits}{other_bits}"
}

proc list_line(member: Member) [time, error] -> Result[Str] {
  let calendar = time.to_calendar(member.mtime * NANOS, utc: false)?
  let user_name = if member.uname == "" { f"{member.uid}" } else { member.uname }
  let group_name = if member.gname == "" { f"{member.gid}" } else { member.gname }
  let target = if member.kind == "symlink" or member.kind == "hardlink" { f" -> {member.link}" } else { "" }
  Ok(f"{mode_text(member.kind, member.mode)} {user_name}/{group_name} {member.size:>9} {calendar.year:04}-{calendar.month:02}-{calendar.day:02} {calendar.hour:02}:{calendar.minute:02}:{calendar.second:02} {member.name}{target}")
}

# Reads the archive bytes. A compression name comes from -z, -j or -J; without
# one, a file archive is detected from its magic bytes; piped input is never sniffed.
proc archive_data(opts: Options) [fs, io, error, process, env] -> Result[Bytes] {
  if opts.archive_path == null {
    let raw = io.stdin_bytes()?
    if opts.compression == "" { return Ok(raw) }
    tempdir scratch {
      let source = fp"{scratch}/archive"
      source.write(raw)?
      return Ok(archive.decompress_bytes(source, opts.compression)?)
    }
  }
  let name = opts.archive_path ?? ""
  let source = fp"{name}"
  let raw = match source.read_bytes() {
    Ok(value) => value
    Err(failure) => { return Err(TarError.Invalid(message: f"{name}: Cannot open: {gnu.strerror(failure)}")) }
  }
  let format = if opts.compression != "" { opts.compression } else { detected_format(raw) }
  if format == "" { return Ok(raw) }
  Ok(archive.decompress_bytes(source, format)?)
}

pure detected_format(raw: Bytes) -> Str {
  if raw.slice(0, 2) == b"\x1f\x8b" { "gz" } else if raw.slice(0, 3) == b"BZh" { "bz2" } else if raw.slice(0, 6) == b"\xfd7zXZ\x00" { "xz" } else { "" }
}

proc list_members(opts: Options) [fs, io, error, process, env, time] -> Result[Unit] {
  let members = read_members(archive_data(opts)?)?
  for member in members {
    if ! selected(canonical(member.name), opts.members, opts.excludes) { continue }
    if opts.verbose {
      print list_line(member)?
    } else {
      print $member.name
    }
  }
  if let missing = first_missing(members, opts.members) {
    return Err(TarError.Invalid(message: f"{missing}: Not found in archive"))
  }
  Ok()
}

# Path.is_dir fails for a missing path, so existence is checked first.
proc is_directory(target: Path) [fs, error] -> Result[Bool] {
  if ! target.exists()? { return Ok(false) }
  target.is_dir()
}

# Makes the parent directories of an extraction target exist.
proc ensure_parent(target: Path) [fs, error] -> Result[Unit] {
  let parent = target.parent()
  if ! is_directory(parent)? {
    parent.mkdir(parents: true)?
  }
  Ok()
}

# Decides what happens when the target already exists. The default refuses to
# replace it; -k skips it; --overwrite removes it first. Returns false to skip.
proc prepare_destination(target: Path, opts: Options) [fs, error] -> Result[Bool] {
  match fs.stat(target, follow_symlinks: false) {
    Ok(_) => {}
    Err(failure) => {
      if gnu.errno(failure) == 2 { return Ok(true) }
      return Err(TarError.Invalid(message: f"{target.display()}: Cannot stat: {gnu.strerror(failure)}"))
    }
  }
  if opts.keep { return Ok(false) }
  if opts.overwrite {
    target.remove()?
    return Ok(true)
  }
  Err(TarError.Invalid(message: f"{target.display()}: destination exists"))
}

# Creates one member below the destination. A directory returns a fixup so its
# mode and mtime are set after its contents, which a read-only mode would block.
proc extract_one(member: Member, target: Path, root: Path, data: Bytes, opts: Options) [fs, io, error, process, env] -> Result[Fixup?] {
  if member.kind == "dir" {
    if ! is_directory(target)? {
      target.mkdir(parents: true)?
    }
    return Ok({path: target, mode: member.mode, mtime: member.mtime})
  }
  if member.kind == "chardev" or member.kind == "blockdev" or member.kind == "fifo" or member.kind == "unsupported" {
    gnu.error(f"skipping {member.kind} member {member.name}")
    return Ok(null)
  }
  ensure_parent(target)?
  if ! prepare_destination(target, opts)? { return Ok(null) }
  match member.kind {
    "file" => {
      target.write(data.slice(member.data, member.size))?
      target.chmod(member.mode.bit_and(0o7777))?
      fs.set_times(target, mtime_ns: member.mtime * NANOS, follow_symlinks: false)?
    }
    "symlink" => fs.symlink(fp"{member.link}", target)?
    "hardlink" => fs.link(member_path(root, strip_components(canonical(member.link), opts.strip))?, target, follow_symlinks: false)?
    _ => {}
  }
  Ok(null)
}

proc extract_members(opts: Options) [fs, io, error, process, env] -> Result[Unit] {
  let data = archive_data(opts)?
  let members = read_members(data)?
  let root = fp"{opts.root}"
  var fixups: List[Fixup] = []
  for member in members {
    let name = canonical(member.name)
    if ! selected(name, opts.members, opts.excludes) { continue }
    let stripped = strip_components(name, opts.strip)
    if stripped == "" { continue }
    if opts.to_stdout {
      if member.kind == "file" { gnu.write_bytes(data.slice(member.data, member.size)) }
      continue
    }
    if opts.verbose { print $member.name }
    let target = member_path(root, stripped)?
    if let fixup = extract_one(member, target, root, data, opts)? {
      fixups += [fixup]
    }
  }
  if let missing = first_missing(members, opts.members) {
    return Err(TarError.Invalid(message: f"{missing}: Not found in archive"))
  }
  for index in range(fixups.len()) {
    let fixup = fixups[fixups.len() - 1 - index]
    fixup.path.chmod(fixup.mode.bit_and(0o7777))?
    fs.set_times(fixup.path, mtime_ns: fixup.mtime * NANOS, follow_symlinks: false)?
  }
  Ok()
}

# The ustar name field holds 100 bytes; a longer name is split at a '/' so the
# prefix holds the leading part, up to 155 bytes, and the name holds the rest.
pure split_name(name: Str) -> Result[SplitName] {
  if name.byte_len() <= NAME_FIELD { return Ok({name: name, prefix: ""}) }
  var position = if name.byte_len() - 1 < PREFIX_FIELD { name.byte_len() - 1 } else { PREFIX_FIELD }
  while position >= 0 and position >= name.byte_len() - NAME_FIELD - 1 {
    if name.byte_slice(position, length: 1) == "/" {
      return Ok({name: name.byte_slice(position + 1), prefix: name.byte_slice(0, length: position)})
    }
    position -= 1
  }
  Err(TarError.Invalid(message: "names longer than 100 chars not supported"))
}

# A numeric field holds width - 1 octal digits and a NUL.
pure octal_text(value: Int, width: Int) -> Result[Str] {
  var digits = ""
  var left = value
  while true {
    digits = f"{"01234567".byte_slice(left % 8, length: 1)}{digits}"
    left = left / 8
    if left == 0 { break }
  }
  if digits.byte_len() > width - 1 { return Err(TarError.Invalid(message: "value too large for tar header")) }
  var padded_digits = digits
  while padded_digits.byte_len() < width - 1 { padded_digits = f"0{padded_digits}" }
  Ok(f"{padded_digits}\0")
}

pure field(text: Str, width: Int) -> Str {
  var result = text
  while result.byte_len() < width { result += "\0" }
  result
}

pure header_text(name: Str, prefix: Str, flag: Str, mode: Int, uid: Int, gid: Int, size: Int, mtime: Int, link: Str, checksum: Str) -> Result[Str] {
  Ok(f"{field(name, NAME_FIELD)}{field(octal_text(mode, 8)?, 8)}{field(octal_text(uid, 8)?, 8)}{field(octal_text(gid, 8)?, 8)}{field(octal_text(size, 12)?, 12)}{field(octal_text(mtime, 12)?, 12)}{checksum}{field(flag, 1)}{field(link, NAME_FIELD)}{field("ustar\0", 6)}{field("00", 2)}{field("", 32)}{field("", 32)}{field("", 8)}{field("", 8)}{field(prefix, PREFIX_FIELD)}{field("", 12)}")
}

# Builds one 512-byte ustar header. The checksum is computed over the header
# with its own field counted as spaces, then written in its place.
pure ustar_header(name: Str, flag: Str, mode: Int, uid: Int, gid: Int, size: Int, mtime: Int, link: Str) -> Result[Bytes] {
  let split = split_name(name)?
  let blank = header_text(split.name, split.prefix, flag, mode, uid, gid, size, mtime, link, "        ")?
  let total = header_sum(bytes.from_text(blank))
  let checksum = f"{octal_text(total, 7)?} "
  Ok(bytes.from_text(header_text(split.name, split.prefix, flag, mode, uid, gid, size, mtime, link, checksum)?))
}

# A member is its header and data; directories recurse in the order the
# filesystem lists them, and excluded names are left out with their subtrees.
proc member_chunks(name: Str, source: Path, opts: Options) [fs, error] -> Result[List[Bytes]] {
  if excluded(canonical(name), opts.excludes) { return Ok([]) }
  let info = match fs.stat(source, follow_symlinks: false) {
    Ok(value) => value
    Err(failure) => { return Err(TarError.Invalid(message: f"{name}: Cannot stat: {gnu.strerror(failure)}")) }
  }
  var chunks: List[Bytes] = []
  if info.kind == "dir" {
    let dir_name = if name.ends_with("/") { name } else { f"{name}/" }
    if opts.verbose { print $dir_name }
    chunks += [ustar_header(dir_name, "5", info.mode.bit_and(0o7777), info.uid, info.gid, 0, info.mtime_ns / NANOS, "")?]
    for child in fs.children(source)?.collect() {
      chunks += member_chunks(f"{dir_name}{child.name}", child.path, opts)?
    }
    return Ok(chunks)
  }
  if opts.verbose { print $name }
  if info.kind == "file" {
    let body = source.read_bytes()?
    chunks += [ustar_header(name, "0", info.mode.bit_and(0o7777), info.uid, info.gid, body.len(), info.mtime_ns / NANOS, "")?]
    chunks += [body]
    chunks += [bytes.zero((BLOCK - body.len() % BLOCK) % BLOCK)?]
  } else if info.kind == "symlink" {
    let target = source.readlink()?
    chunks += [ustar_header(name, "2", info.mode.bit_and(0o7777), info.uid, info.gid, 0, info.mtime_ns / NANOS, target.display())?]
  } else {
    return Err(TarError.Invalid(message: f"{name}: unknown file type"))
  }
  Ok(chunks)
}

# Writes archive bytes to the archive file or standard output, through the
# compression named by -z, -j or -J when one was given.
proc write_archive(opts: Options, data: Bytes) [fs, io, error, process, env] -> Result[Unit] {
  if opts.compression == "" {
    if opts.archive_path == null {
      gnu.write_bytes(data)
    } else {
      fp"{opts.archive_path ?? ""}".write(data)?
    }
    return Ok()
  }
  tempdir scratch {
    let raw = fp"{scratch}/archive.tar"
    let packed = fp"{scratch}/archive.out"
    raw.write(data)?
    if opts.archive_path == null {
      archive.compress(raw, packed, format: opts.compression, overwrite: true)?
      gnu.write_bytes(packed.read_bytes()?)
    } else {
      archive.compress(raw, fp"{opts.archive_path ?? ""}", format: opts.compression, overwrite: true)?
    }
  }
  Ok()
}

proc create_archive(opts: Options) [fs, io, error, process, env] -> Result[Unit] {
  if opts.members.is_empty() { return Err(TarError.Invalid(message: "Cowardly refusing to create an empty archive")) }
  let root = fp"{opts.root}"
  var chunks: List[Bytes] = []
  for operand in opts.members {
    chunks += member_chunks(operand, fp"{root}/{operand}", opts)?
  }
  chunks += [bytes.zero(ZERO_BLOCKS)?]
  write_archive(opts, bytes.concat(chunks))
}

proc execute(opts: Options) [fs, io, error, process, env, time] -> Result[Unit] {
  match opts.mode {
    "create" => create_archive(opts)
    "list" => list_members(opts)
    _ => extract_members(opts)
  }
}

# A mode letter chooses one operation; repeating the same one is accepted.
proc choose_mode(current: Str, next: Str) [process, env] -> Str {
  if current != "" and current != next {
    gnu.usage_error("You may not specify more than one '-Acdtrux' option")
  }
  next
}

# Member names listed in a file, one per line. A file that cannot be read ends the applet.
proc read_names(path_text: Str) [fs, error, process, env] -> List[Str] {
  match fp"{path_text}".read_lines() {
    Ok(lines) => {
      var names: List[Str] = []
      for line in lines {
        if line != "" { names += [line] }
      }
      names
    }
    Err(failure) => {
      gnu.cannot_open(path_text, failure)
      exit 1
    }
  }
}

# Accepts the classic bundled form (`tar xvf ARCHIVE`) by treating a first word
# without a dash as option letters, then parses long options and operands.
proc parse_options(argv: List[Str]) [fs, error, process, env] -> Options {
  var opts: Options = {mode: "", verbose: false, archive_path: null, root: ".", compression: "", to_stdout: false, keep: false, overwrite: false, strip: 0, excludes: [], members: [], help: false, version: false}
  var words = argv
  if ! words.is_empty() and ! words[0].starts_with("-") {
    words = [f"-{words[0]}"] + words[1..]
  }
  var index = 0
  var operands_only = false
  while index < words.len() {
    let word = words[index]
    index += 1
    if operands_only or word == "-" or ! word.starts_with("-") {
      opts.members += [word]
      continue
    }
    if word == "--" {
      operands_only = true
      continue
    }
    if word.starts_with("--") {
      let body = word.byte_slice(2)
      let equals = body.find("=") ?? -1
      let long_name = if equals < 0 { body } else { body.byte_slice(0, length: equals) }
      var inline_value: Str? = null
      if equals >= 0 { inline_value = body.byte_slice(equals + 1) }
      match long_name {
        "overwrite" => opts.overwrite = true
        "help" => opts.help = true
        "version" => opts.version = true
        "strip-components" => {
          var value = inline_value ?? ""
          if inline_value == null {
            if index >= words.len() { gnu.usage_error("option '--strip-components' requires an argument") }
            value = words[index]
            index += 1
          }
          match value.parse_int_decimal() {
            Ok(count) => opts.strip = count
            Err(_) => gnu.usage_error(f"invalid number of strip components {gnu.quote_value(value)}")
          }
        }
        _ => gnu.usage_error(f"unrecognized option '--{long_name}'")
      }
      continue
    }
    var position = 1
    while position < word.byte_len() {
      let letter = word.byte_slice(position, length: 1)
      position += 1
      if letter in ["f", "C", "X", "T"] {
        var value = word.byte_slice(position)
        position = word.byte_len()
        if value == "" {
          if index >= words.len() { gnu.usage_error(f"option requires an argument -- '{letter}'") }
          value = words[index]
          index += 1
        }
        match letter {
          "f" => opts.archive_path = if value == "-" { null } else { value }
          "C" => opts.root = value
          "X" => opts.excludes += read_names(value)
          _ => opts.members += read_names(value)
        }
      } else {
        match letter {
          "c" => opts.mode = choose_mode(opts.mode, "create")
          "t" => opts.mode = choose_mode(opts.mode, "list")
          "x" => opts.mode = choose_mode(opts.mode, "extract")
          "v" => opts.verbose = true
          "O" => opts.to_stdout = true
          "k" => opts.keep = true
          "o" => {}
          "z" => opts.compression = "gz"
          "j" => opts.compression = "bz2"
          "J" => opts.compression = "xz"
          "a" | "h" | "m" | "p" | "Z" => gnu.usage_error(f"unsupported option -- '{letter}'")
          _ => gnu.usage_error(f"invalid option -- '{letter}'")
        }
      }
    }
  }
  if ! opts.help and ! opts.version and opts.mode == "" {
    gnu.usage_error("You must specify one of the '-Acdtrux' options")
  }
  opts
}

proc main(...argv: List[Str]) [fs, io, error, process, env, time] {
  let opts = parse_options(argv)
  if opts.help {
    gnu.help(USAGE)
    return
  }
  if opts.version {
    gnu.version("tar")
    return
  }
  match execute(opts) {
    Ok(_) => {}
    Err(failure) => {
      match failure {
        TarError.Invalid {message} => gnu.error(message)
        _ => gnu.error(gnu.strerror(failure))
      }
      exit 1
    }
  }
}
