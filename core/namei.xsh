#!/bin/xsh
use lib.gnu as gnu
use lib.util_linux_io as ul_io

pure util_strerror(failure: Error) -> Str {
  let text = failure.message
  let at = text.find(" (os error ") ?? -1
  if at < 0 { return text }
  let parts = text.byte_slice(0, at).split(": ")
  parts[-1]
}

const MAXSYMLINKS = 20
const USAGE = """Usage:
 namei [options] <pathname>...

Follow a pathname until a terminal point is found.

Options:
 -x, --mountpoints   show mount point directories with a 'D'
 -m, --modes         show the mode bits of each file
 -o, --owners        show owner and group name of each file
 -l, --long          use a long listing format (-m -o -v)\x20
 -n, --nosymlinks    don't follow symlinks
 -v, --vertical      vertical align of modes and owners
 -h, --help          display this help
 -V, --version       display version

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

type Expansion = {entries: List[Entry], width_entries: List[Entry], exceeded: Bool}
type OwnerWidths = {user_width: Int, group_width: Int}
type Listing = {chunks: List[Bytes], failed: Bool}
type Output = {buffer: Bytes, initialized: Bool, failed: Bool}

proc unrecognized_option(value: Bytes) [process, env, io] -> Unit {
  ul_io.write_diagnostic_bytes(bytes.concat([bytes.from_text(f"{gnu.prog()}: unrecognized option: "), value, b"\n"]))
  gnu.try_help()
  exit 1
}

proc parse_options(argv: List[Str], raw: List[gnu.RawArgument]) [process, env, io] -> Options {
  var options: Options = {mounts: false, modes: false, owners: false, long: false, nosymlinks: false, vertical: false, help: false, version: false, paths: []}
  var at = 0
  while at < argv.len() {
    let word = argv[at]
    at += 1
    if word == "--" {
      return {...options, paths: options.paths.extend(argv[at..])}
    }
    if ! word.starts_with("-") or word == "-" {
      options = {...options, paths: options.paths.extend([word])}
      continue
    }
    var flags: List[Str] = []
    if word.starts_with("--") {
      let body = word.byte_slice(2)
      let typed = body.split("=")[0]
      let names = ["mountpoints", "modes", "owners", "long", "nosymlinks", "vertical", "help", "version"]
      let exact = [name for name in names if name == typed]
      let matches = if exact.is_empty() { [name for name in names if name.starts_with(typed)] } else { exact }
      if matches.is_empty() { unrecognized_option(gnu.argument_bytes(body, raw)) }
      if matches.len() > 1 { gnu.usage_error(f"option is ambiguous: {body}") }
      if "=" in body { gnu.usage_error(f"option does not take an argument: {matches[0]}") }
      flags = matches
    } else {
      let marker_at = word.find("\0") ?? word.byte_len()
      flags = [flag for flag in word.byte_slice(1, length: marker_at - 1)]
      if marker_at < word.byte_len() { flags += [word.byte_slice(marker_at)] }
    }
    for flag in flags {
      match flag {
        "x" | "mountpoints" => options = {...options, mounts: true}
        "m" | "modes" => options = {...options, modes: true}
        "o" | "owners" => options = {...options, owners: true}
        "l" | "long" => options = {...options, long: true}
        "n" | "nosymlinks" => options = {...options, nosymlinks: true}
        "v" | "vertical" => options = {...options, vertical: true}
        "h" | "help" => return {...options, help: true}
        "V" | "version" => return {...options, version: true}
        "Z" => gnu.usage_error("unrecognized option: Z")
        else => {
          let value = gnu.argument_bytes(flag, raw)
          unrecognized_option(if flag.starts_with("\0") { value[..1] } else { value })
        }
      }
    }
  }
  options
}

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
        return Ok({entries: expanded[0..index + 1], width_entries: expanded, exceeded: true})
      }
      let inserted = symlink_entries(item, mounts)?
      expanded = expanded[0..index + 1] + inserted + expanded[index + 1..]
    }
    index += 1
  }
  Ok({entries: expanded, width_entries: expanded, exceeded: false})
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

proc render(entries: List[Entry], operand: Bytes, options: Options, user_width: Int, group_width: Int) [fs] -> Listing {
  if entries.is_empty() { return {chunks: [], failed: false} }
  var chunks: List[Bytes] = [b"f: ", operand, b"\n"]
  var failed = false
  for entry in entries {
    if let failure = entry.stat_error {
      failed = true
      let blanks = 1 + (if options.modes { 9 } else { 0 }) + (if options.owners { user_width + group_width + 2 } else { 0 }) + (if options.vertical { 0 } else { 1 }) + 1 + entry.level * 2
      chunks += [spaces(blanks), b" ", entry.name, b" - ", bytes.from_text(util_strerror(failure)), b"\n"]
      break
    }
    guard let metadata = entry.metadata else { continue }
    let mode = mode_string(metadata, entry.mountpoint)
    if ! options.vertical {
      for _ in range(entry.level) { chunks += [b"  "] }
      chunks += [b" "]
    }
    chunks += [if options.modes { bytes.from_text(mode) } else { bytes.from_text(mode.byte_slice(0, length: 1)) }]
    if options.owners {
      let user_name = username(metadata.uid)
      let group_name = groupname(metadata.gid)
      chunks += [b" ", bytes.from_text(user_name), spaces(user_width - user_name.byte_len()), b" ", bytes.from_text(group_name), spaces(group_width - group_name.byte_len())]
    }
    if options.vertical { for _ in range(entry.level) { chunks += [b"  "] } }
    if let target = entry.link {
      chunks += [b" ", entry.name, b" -> ", target, b"\n"]
    } else {
      chunks += [b" ", entry.name, b"\n"]
    }
  }
  {chunks: chunks, failed: failed}
}

proc write_output(output: Output, data: Bytes) [io] -> Output {
  if output.failed { return output }
  if let Err(_) = io.write_stdout_bytes(data) { return {...output, buffer: b"", initialized: true, failed: true} }
  if let Err(_) = io.flush_stdout() { return {...output, buffer: b"", initialized: true, failed: true} }
  {...output, buffer: b"", initialized: true}
}

proc output_chunks(initial: Output, chunks: List[Bytes], terminal: Bool) [io] -> Output {
  var output = initial
  for chunk in chunks {
    if output.failed { break }
    if chunk.is_empty() { continue }
    var newline = 0
    if terminal or ! output.initialized {
      for index in range(chunk.len()) {
        if chunk.byte_at(index) == 10 { newline = index + 1 }
      }
    }
    if output.buffer.len() + chunk.len() > 1024 {
      output = write_output(output, bytes.concat([output.buffer, chunk]))
    } else if newline > 0 {
      output = write_output(output, bytes.concat([output.buffer, chunk[0..newline]]))
      if ! output.failed { output = {...output, buffer: chunk[newline..]} }
    } else {
      output = {...output, buffer: bytes.concat([output.buffer, chunk])}
    }
  }
  output
}

proc report_fatal(pathname: Bytes, kind: Str, failure: Error, output: Output) [process, env, io, error] {
  ul_io.write_diagnostic_bytes(bytes.concat([
    bytes.from_text(f"{gnu.prog()}: {kind}"), pathname,
    bytes.from_text(f": {util_strerror(failure)}\n"),
  ]))
  let finished = if output.buffer.is_empty() { output } else { write_output(output, output.buffer) }
  if finished.failed { gnu.error("write error") }
  exit 1
}

proc report_loop(pathname: Bytes) [process, env, io] {
  ul_io.write_diagnostic_bytes(bytes.concat([bytes.from_text(f"{gnu.prog()}: "), pathname, b": exceeded limit of symlinks\n"]))
}

proc main(...argv: List[Bytes]) [fs, process, env, error, io] {
  if process.signal_action("PIPE")? == "default" {
    process.set_signal_action("PIPE", "default")
  }

  let prepared = gnu.prepare_arguments(argv)
  let options = parse_options(prepared.text, prepared.raw)

  if options.help { ul_io.help(f"\n{USAGE}"); return }
  if options.version { ul_io.version("namei"); return }
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
  var output: Output = {buffer: b"", initialized: false, failed: false}
  let terminal = unix.isatty(1)
  for token in normalized_options.paths {
    let operand = gnu.argument_bytes(token, prepared.raw)
    let operand_path = Path.parse_bytes(operand)?
    if fs.stat(operand_path, follow_symlinks: true) is Err(_) { failed = true }
    let entries_from_root = path_entries(operand, 0, b"", 0, null, options.mounts)?
    let result = expand(entries_from_root, options.nosymlinks, options.mounts)?
    for entry in result.entries {
      if let failure = entry.link_error { report_fatal(entry.path, "failed to read symlink: ", failure, output) }
      if let failure = entry.mount_error { report_fatal(bytes.concat([entry.path, b"/.. failed"]), "stat of ", failure, output) }
    }
    let widths = update_owner_widths(result.width_entries, normalized_options.owners, user_width, group_width)
    user_width = widths.user_width
    group_width = widths.group_width
    let listing = render(result.entries, operand, normalized_options, user_width, group_width)
    output = output_chunks(output, listing.chunks, terminal)
    failed = listing.failed or failed
    if result.exceeded and ! listing.failed {
      failed = true
      report_loop(operand)
    }
  }
  if ! output.buffer.is_empty() { output = write_output(output, output.buffer) }
  if output.failed { gnu.error("write error"); exit 1 }
  if failed { exit 1 }
}
