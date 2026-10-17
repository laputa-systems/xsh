#!/bin/xsh
use lib.gnu as gnu

const MAXSYMLINKS = 20
const USAGE = """Usage:
 namei [options] <pathname>...

Follow a pathname until a terminal point is found.

Options:
 -x, --mountpoints show mount point directories with a 'D'
 -m, --modes show the mode bits of each file
 -o, --owners show owner and group name of each file
 -l, --long use a long listing format (-m -o -v)
 -n, --nosymlinks don't follow symlinks
 -v, --vertical vertical align of modes and owners

 -h, --help display this help
 -V, --version display version

For more details see namei(1).
"""

type Options = {
  mounts: Bool,
  modes: Bool,
  owners: Bool,
  long: Bool,
  nosymlinks: Bool,
  vertical: Bool,
  help: Bool,
  version: Bool,
  paths: List[Str],
}

type Entry = {
  name: Bytes,
  path: Bytes,
  level: Int,
  metadata: FsStat?,
  stat_error: Error?,
  link: Bytes?,
  link_error: Error?,
  mountpoint: Bool,
  mount_error: Error?,
}

type Expansion = {entries: List[Entry], exceeded: Bool}
type OwnerWidths = {user_width: Int, group_width: Int}

pure has_bit(mode: Int, bit: Int) -> Bool {
  mode.bit_and(bit) != 0
}

pure mode_triplet(mode: Int, read_bit: Int, write_bit: Int, exec_bit: Int) -> Str {
  let read = if has_bit(mode, read_bit) { "r" } else { "-" }
  let write = if has_bit(mode, write_bit) { "w" } else { "-" }
  let execute = if has_bit(mode, exec_bit) { "x" } else { "-" }
  f"{read}{write}{execute}"
}

pure mode_string(metadata: FsStat, mountpoint: Bool) -> Str {
  let kind = match metadata.kind {
    "dir" => "d"
    "symlink" => "l"
    "char" => "c"
    "block" => "b"
    "fifo" => "p"
    "socket" => "s"
    else => "-"
  }
  var owner = mode_triplet(metadata.mode, 0o400, 0o200, 0o100)
  var group_bits = mode_triplet(metadata.mode, 0o40, 0o20, 0o10)
  var other = mode_triplet(metadata.mode, 0o4, 0o2, 0o1)
  if has_bit(metadata.mode, 0o4000) { owner = owner.byte_slice(0, length: 2) + (if has_bit(metadata.mode, 0o100) { "s" } else { "S" }) }
  if has_bit(metadata.mode, 0o2000) { group_bits = group_bits.byte_slice(0, length: 2) + (if has_bit(metadata.mode, 0o10) { "s" } else { "S" }) }
  if has_bit(metadata.mode, 0o1000) { other = other.byte_slice(0, length: 2) + (if has_bit(metadata.mode, 0o1) { "t" } else { "T" }) }
  f"{if mountpoint { "D" } else { kind }}{owner}{group_bits}{other}"
}

pure components(pathname: Bytes, start: Int) -> List[Bytes] {
  var begin = start
  let parts = collect {
    for index in range(start, pathname.len()) {
      if pathname.byte_at(index) == 47 {
        yield pathname[begin..index] when index > begin
        begin = index + 1
      }
    }
    yield pathname[begin..] when pathname.len() > begin
  }
  parts
}

pure append_path(prefix: Bytes, component: Bytes) -> Bytes {
  if prefix.is_empty() { return component }
  if prefix == b"/" { return bytes.concat([prefix, component]) }
  bytes.concat([prefix, b"/", component])
}

pure last_slash(pathname: Bytes) -> Int? {
  var found: Int? = null
  for index in range(pathname.len()) {
    if pathname.byte_at(index) == 47 { found = index }
  }
  found
}

proc inspect(name: Bytes, path_bytes: Bytes, level: Int, parent: Entry?, mounts: Bool) [fs, error] -> Result[Entry, Error] {
  let path_value = Path.parse_bytes(path_bytes)?
  var metadata: FsStat? = null
  var stat_error: Error? = null
  var link: Bytes? = null
  var link_error: Error? = null
  var mountpoint = false
  var mount_error: Error? = null

  match fs.stat(path_value, follow_symlinks: false) {
    Ok(value) => metadata = value
    Err(failure) => stat_error = failure
  }

  if let value = metadata {
    if value.kind == "symlink" {
      match path_value.readlink() {
        Ok(target) => link = target.bytes()
        Err(failure) => link_error = failure
      }
    }
    if mounts and value.kind == "dir" {
      var compare: FsStat? = null
      if let previous = parent {
        if let parent_metadata = previous.metadata {
          if parent_metadata.kind == "dir" {
            compare = parent_metadata
          } else if parent_metadata.kind == "symlink" {
            let parent_path = Path.parse_bytes(bytes.concat([path_bytes, b"/.."]))?
            match fs.stat(parent_path) {
              Ok(parent_stat) => compare = parent_stat
              Err(failure) => mount_error = failure
            }
          }
        }
      } else {
        let parent_path = Path.parse_bytes(bytes.concat([path_bytes, b"/.."]))?
        match fs.stat(parent_path) {
          Ok(parent_stat) => compare = parent_stat
          Err(failure) => mount_error = failure
        }
      }
      if let previous_metadata = compare {
        mountpoint = previous_metadata.dev != value.dev or previous_metadata.ino == value.ino
      }
    }
  }

  Ok({name: name, path: path_bytes, level: level, metadata: metadata, stat_error: stat_error, link: link, link_error: link_error, mountpoint: mountpoint, mount_error: mount_error})
}

proc path_entries(pathname: Bytes, start: Int, base: Bytes, level: Int, parent: Entry?, mounts: Bool) [fs, error] -> Result[List[Entry], Error] {
  var entries: List[Entry] = []
  var current = base
  var previous = parent
  var offset = start
  if offset < pathname.len() and pathname.byte_at(offset) == 47 {
    while offset < pathname.len() and pathname.byte_at(offset) == 47 { offset += 1 }
    let root = inspect(b"/", b"/", level, parent, mounts)?
    entries += [root]
    previous = root
    current = b"/"
  }
  for component in components(pathname, offset) {
    let full_path = append_path(current, component)
    let item = inspect(component, full_path, level, previous, mounts)?
    entries += [item]
    previous = item
    current = full_path
  }
  Ok(entries)
}

proc symlink_entries(entry: Entry, mounts: Bool) [fs, error] -> Result[List[Entry], Error] {
  guard let target = entry.link else { return Ok([]) }
  if target.starts_with(b"/") {
    return path_entries(target, 0, b"", entry.level + 1, entry, mounts)
  }
  if let slash = last_slash(entry.path) {
    let prefix = entry.path[0..slash]
    let base = if prefix.is_empty() { b"/" } else { prefix }
    let expanded = bytes.concat([base, if base == b"/" { b"" } else { b"/" }, target])
    let begin = base.len() + (if base == b"/" { 0 } else { 1 })
    return path_entries(expanded, begin, base, entry.level + 1, entry, mounts)
  }
  path_entries(target, 0, b"", entry.level + 1, entry, mounts)
}

proc expand(entries: List[Entry], nosymlinks: Bool, mounts: Bool) [fs, error] -> Result[Expansion, Error] {
  var expanded = entries
  var index = 0
  var links = 0
  while index < expanded.len() {
    let item = expanded[index]
    if ! nosymlinks and item.link != null {
      links += 1
      if links > MAXSYMLINKS {
        return Ok({entries: expanded[0..index + 1], exceeded: true})
      }
      let inserted = symlink_entries(item, mounts)?
      expanded = expanded[0..index + 1] + inserted + expanded[index + 1..]
    }
    index += 1
  }
  Ok({entries: expanded, exceeded: false})
}

proc username(uid: Int) [fs] -> Str {
  match user.by_uid(uid) {
    Ok(account) => account.name
    Err(_) => f"{uid}"
  }
}

proc groupname(gid: Int) [fs] -> Str {
  match group.by_gid(gid) {
    Ok(group_entry) => group_entry.name
    Err(_) => f"{gid}"
  }
}

proc update_owner_widths(entries: List[Entry], owners: Bool, user_width: Int, group_width: Int) [fs] -> OwnerWidths {
  var max_user = user_width
  var max_group = group_width
  if owners {
    for entry in entries {
      if let metadata = entry.metadata {
        let user_name = username(metadata.uid)
        let group_name = groupname(metadata.gid)
        if user_name.byte_len() > max_user { max_user = user_name.byte_len() }
        if group_name.byte_len() > max_group { max_group = group_name.byte_len() }
      }
    }
  }
  {user_width: max_user, group_width: max_group}
}

pure spaces(count: Int) -> Bytes {
  bytes.concat([b" " for _ in range(count)])
}

proc render(entries: List[Entry], operand: Bytes, options: Options, user_width: Int, group_width: Int) [fs, env, process, io] -> Bool {
  return false when entries.is_empty()
  gnu.write_bytes(bytes.concat([b"f: ", operand, b"\n"]))
  var failed = false
  for entry in entries {
    if let failure = entry.stat_error {
      failed = true
      let blanks = 1 + (if options.modes { 9 } else { 0 }) + (if options.owners { user_width + group_width + 2 } else { 0 }) + (if options.vertical { 0 } else { 1 }) + 1 + entry.level * 2
      gnu.write_bytes(bytes.concat([spaces(blanks), b" ", entry.name, b" - ", bytes.from_text(gnu.strerror(failure)), b"\n"]))
      break
    }
    guard let metadata = entry.metadata else { continue }
    let mode = mode_string(metadata, entry.mountpoint)
    if ! options.vertical { gnu.write_bytes(spaces(entry.level * 2)); gnu.write_bytes(b" ") }
    gnu.write_bytes(if options.modes { bytes.from_text(mode) } else { bytes.from_text(mode.byte_slice(0, length: 1)) })
    if options.owners {
      let user_name = username(metadata.uid)
      let group_name = groupname(metadata.gid)
      gnu.write_bytes(bytes.from_text(f" {right_pad(user_name, user_width)} {right_pad(group_name, group_width)}"))
    }
    if options.vertical { gnu.write_bytes(spaces(entry.level * 2)) }
    if let target = entry.link {
      gnu.write_bytes(bytes.concat([b" ", entry.name, b" -> ", target, b"\n"]))
    } else {
      gnu.write_bytes(bytes.concat([b" ", entry.name, b"\n"]))
    }
  }
  failed
}

pure right_pad(text: Str, width: Int) -> Str {
  var result = text
  while result.byte_len() < width { result += " " }
  result
}

proc write_stderr_bytes(data: Bytes) [process, env, io] {
  if let Err(failure) = io.write_stderr_bytes(data) {
    gnu.error(f"write error: {gnu.strerror(failure)}")
    exit 1
  }
  if let Err(failure) = io.flush_stderr() {
    gnu.error(f"write error: {gnu.strerror(failure)}")
    exit 1
  }
}

proc report_fatal(pathname: Bytes, kind: Str, failure: Error) [process, env, io, error] {
  write_stderr_bytes(bytes.concat([
    bytes.from_text(f"{gnu.prog()}: {kind}"), pathname,
    bytes.from_text(f": {gnu.strerror(failure)}\n"),
  ]))
  exit 1
}

proc report_loop(pathname: Bytes) [process, env, io] {
  write_stderr_bytes(bytes.concat([bytes.from_text(f"{gnu.prog()}: "), pathname, b": exceeded limit of symlinks\n"]))
}

proc main(...argv: List[Bytes]) [fs, process, env, error, io] {
  let prepared = gnu.prepare_arguments(argv)
  let options: Options = cli.applet(prepared.text, {
    gnu: {status: 1},
    mounts: {form: "-x --mountpoints", default: false},
    modes: {form: "-m --modes", default: false},
    owners: {form: "-o --owners", default: false},
    long: {form: "-l --long", default: false},
    nosymlinks: {form: "-n --nosymlinks", default: false},
    vertical: {form: "-v --vertical", default: false},
    help: {form: "-h --help", default: false, stop: true},
    version: {form: "-V --version", default: false, stop: true},
    paths: {form: "...PATH"},
  })?

  if options.help { gnu.help(USAGE); return }
  if options.version { gnu.version("namei"); return }
  if options.paths.is_empty() {
    gnu.error("pathname argument is missing")
    gnu.try_help()
    exit 1
  }

  let long = options.long
  let normalized_options = {...options, modes: options.modes or long, owners: options.owners or long, vertical: options.vertical or long}
  var failed = false
  var user_width = 0
  var group_width = 0
  for token in normalized_options.paths {
    let operand = gnu.argument_bytes(token, prepared.raw)
    let operand_path = Path.parse_bytes(operand)?
    if fs.stat(operand_path, follow_symlinks: true) is Err(_) { failed = true }
    let components = path_entries(operand, 0, b"", 0, null, options.mounts)?
    let result = expand(components, options.nosymlinks, options.mounts)?
    for entry in result.entries {
      if let failure = entry.link_error { report_fatal(entry.path, "failed to read symlink: ", failure) }
      if let failure = entry.mount_error { report_fatal(bytes.concat([entry.path, b"/.."]), "stat of ", failure) }
    }
    let widths = update_owner_widths(result.entries, normalized_options.owners, user_width, group_width)
    user_width = widths.user_width
    group_width = widths.group_width
    failed = render(result.entries, operand, normalized_options, user_width, group_width) or failed
    if result.exceeded {
      failed = true
      report_loop(operand)
    }
  }
  if failed { exit 1 }
}
