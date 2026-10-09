#!/bin/xsh
use lib.gnu

type StatMeta = {
  kind: Str,
  mode: Int,
  size: Int,
  blocks_512: Int,
  blksize: Int,
  uid: Int,
  gid: Int,
  nlink: Int,
  dev: Int,
  ino: Int,
  rdev: Int,
  atime_seconds: Int,
  atime_nanoseconds: Int,
  mtime_seconds: Int,
  mtime_nanoseconds: Int,
  ctime_ns: Int,
  birth_ns: Int?,
}

type StatFs = {
  block_size: Int,
  fragment_size: Int,
  blocks: Int,
  blocks_free: Int,
  blocks_available: Int,
  files: Int,
  files_free: Int,
  fsid: Int,
  name_max: Int,
  type_magic: Int?,
}

type StatOptions = {
  dereference: Bool,
  filesystem: Bool,
  format: Str?,
  printf: Str?,
  terse: Bool,
  help: Bool,
  version: Bool,
  paths: List[Str],
}

pure has_bit(mode: Int, bit: Int) -> Bool {
  mode / bit % 2 == 1
}

pure octal(value: Int) -> Str {
  return "0" when value == 0

  var number = value
  var result = ""
  while number > 0 {
    result = f"{number % 8}{result}"
    number = number / 8
  }
  result
}

pure hexadecimal(value: Int) -> Str {
  const digits = "0123456789abcdef"
  return "0" when value == 0

  var number = value
  var result = ""
  while number > 0 {
    result = f"{digits.byte_slice(number % 16, length: 1)}{result}"
    number = number / 16
  }
  result
}

pure trim_hex_zeroes(text: Str) -> Str {
  var start = 0
  while start < text.byte_len() and text.byte_slice(start, length: 1) == "0" { start += 1 }
  return "0" when start == text.byte_len()
  text.byte_slice(start)
}

pure filesystem_id(value: Int) -> Str {
  return "0" when value == 0
  let raw = hexadecimal(value)
  var padded = raw
  while padded.byte_len() < 16 { padded = f"0{padded}" }
  if padded.byte_len() > 16 { padded = padded.byte_slice(padded.byte_len() - 16) }
  f"{trim_hex_zeroes(padded.byte_slice(8, length: 8))}{padded.byte_slice(0, length: 8)}"
}

pure file_type_name(kind: Str) -> Str {
  match kind {
    "dir" => "directory"
    "file" => "regular file"
    "symlink" => "symbolic link"
    "fifo" => "fifo"
    "socket" => "socket"
    "block" => "block special file"
    "char" => "character special file"
    else => "unknown"
  }
}

pure mode_string(kind: Str, mode: Int) -> Str {
  let file_type = match kind {
    "dir" => "d"
    "symlink" => "l"
    "fifo" => "p"
    "socket" => "s"
    "block" => "b"
    "char" => "c"
    else => "-"
  }
  let ur = if has_bit(mode, 0o400) { "r" } else { "-" }
  let uw = if has_bit(mode, 0o200) { "w" } else { "-" }
  let ux = if has_bit(mode, 0o4000) { if has_bit(mode, 0o100) { "s" } else { "S" } } else { if has_bit(mode, 0o100) { "x" } else { "-" } }
  let gr = if has_bit(mode, 0o40) { "r" } else { "-" }
  let gw = if has_bit(mode, 0o20) { "w" } else { "-" }
  let gx = if has_bit(mode, 0o2000) { if has_bit(mode, 0o10) { "s" } else { "S" } } else { if has_bit(mode, 0o10) { "x" } else { "-" } }
  let other_r = if has_bit(mode, 0o4) { "r" } else { "-" }
  let ow = if has_bit(mode, 0o2) { "w" } else { "-" }
  let ox = if has_bit(mode, 0o1000) { if has_bit(mode, 0o1) { "t" } else { "T" } } else { if has_bit(mode, 0o1) { "x" } else { "-" } }
  f"{file_type}{ur}{uw}{ux}{gr}{gw}{gx}{other_r}{ow}{ox}"
}

proc time_string(seconds: Int, nanoseconds: Int) [time, error] -> Result[Str] {
  time.format(seconds, nanoseconds, "%F %T.%N %z", "local", "gregorian", "locale")
}

proc file_directive(specifier: Str, target: Path, meta: StatMeta) [fs, error, time, env] -> Result[Str] {
  var owner = f"{meta.uid}"
  var owner_group = f"{meta.gid}"
  if let Ok(found_user) = user.by_uid(meta.uid) { owner = found_user.name }
  if let Ok(found_group) = group.by_gid(meta.gid) { owner_group = found_group.name }

  Ok(match specifier {
    "%a" => octal(meta.mode % 4096)
    "%A" => mode_string(meta.kind, meta.mode)
    "%b" => f"{meta.blocks_512}"
    "%B" => "512"
    "%d" => f"{meta.dev}"
    "%D" => hexadecimal(meta.dev)
    "%f" => hexadecimal(meta.mode)
    "%F" => file_type_name(meta.kind)
    "%g" => f"{meta.gid}"
    "%G" => owner_group
    "%h" => f"{meta.nlink}"
    "%i" => f"{meta.ino}"
    "%m" => {
      let mount = fs.mount_for(target.resolve()?)?
      mount.mounted_on.display()
    }
    "%n" => target.display()
    "%N" => {
      let name = gnu.quote_maybe(target.display())
      if meta.kind == "symlink" {
        let link = target.readlink()?
        f"{gnu.quote_maybe(target.display())} -> {gnu.quote_maybe(link.display())}"
      } else {
        gnu.quote_maybe(target.display())
      }
    }
    "%o" => f"{meta.blksize}"
    "%r" => f"{meta.rdev}"
    "%s" => f"{meta.size}"
    "%t" => hexadecimal(fs.dev_major(meta.rdev))
    "%T" => hexadecimal(fs.dev_minor(meta.rdev))
    "%u" => f"{meta.uid}"
    "%U" => owner
    "%w" => {
      if let birth = meta.birth_ns {
        time_string(birth / 1000000000, birth % 1000000000)?
      } else {
        "-"
      }
    }
    "%W" => f"{(meta.birth_ns ?? 0) / 1000000000}"
    "%x" => time_string(meta.atime_seconds, meta.atime_nanoseconds)?
    "%X" => f"{meta.atime_seconds}"
    "%y" => time_string(meta.mtime_seconds, meta.mtime_nanoseconds)?
    "%Y" => f"{meta.mtime_seconds}"
    "%z" => {
      let seconds = meta.ctime_ns / 1000000000
      let nanoseconds = meta.ctime_ns % 1000000000
      time_string(seconds, nanoseconds)?
    }
    "%Z" => f"{meta.ctime_ns / 1000000000}"
    "%%" => "%"
    else => ""
  })
}

proc render_format(format: Str, target: Path, meta: StatMeta) [fs, error, time, env] -> Result[Str] {
  var output = format
  output = output.replace("%%", "\u{0}XSH_PERCENT\u{0}")
  for specifier in ["%a", "%A", "%b", "%B", "%d", "%D", "%f", "%F", "%g", "%G", "%h", "%i", "%m", "%n", "%N", "%o", "%r", "%s", "%t", "%T", "%u", "%U", "%w", "%W", "%x", "%X", "%y", "%Y", "%z", "%Z"] {
    output = output.replace(specifier, file_directive(specifier, target, meta)?)
  }
  Ok(output.replace("\u{0}XSH_PERCENT\u{0}", "%"))
}

proc terse_format(target: Path, meta: StatMeta) [fs, error, time, env] -> Result[Str] {
  render_format("%n %s %b %B %f %u %g %D %i %h %d %r %t %T %X %Y %Z %W", target, meta)?
}

proc filesystem_format(format: Str, target: Path, info: StatFs, mount: FsMount) [env] -> Str {
  format.replace("%%", "\u{0}XSH_PERCENT\u{0}")
    .replace("%b", f"{info.blocks}")
    .replace("%f", f"{info.blocks_free}")
    .replace("%a", f"{info.blocks_available}")
    .replace("%c", f"{info.files}")
    .replace("%d", f"{info.files_free}")
    .replace("%i", filesystem_id(info.fsid))
    .replace("%l", f"{info.name_max}")
    .replace("%n", gnu.quote_maybe(target.display()))
    .replace("%s", f"{info.block_size}")
    .replace("%S", f"{info.fragment_size}")
    .replace("%t", f"{hexadecimal(info.type_magic ?? 0)}")
    .replace("%T", mount.fstype)
    .replace("\u{0}XSH_PERCENT\u{0}", "%")
}

proc default_format(target: Path, meta: StatMeta) [fs, error, time, env] -> Result[Str] {
  let name = gnu.quote_maybe(target.display())
  let access = time_string(meta.atime_seconds, meta.atime_nanoseconds)?
  let modify = time_string(meta.mtime_seconds, meta.mtime_nanoseconds)?
  let change = time_string(meta.ctime_ns / 1000000000, meta.ctime_ns % 1000000000)?
  let birth = if let value = meta.birth_ns { time_string(value / 1000000000, value % 1000000000)? } else { "-" }
  let owner = file_directive("%U", target, meta)?
  let owner_group = file_directive("%G", target, meta)?
  Ok(f"  File: {name}\n  Size: {meta.size}\tBlocks: {meta.blocks_512}\tIO Block: {meta.blksize} {file_type_name(meta.kind)}\nDevice: {hexadecimal(meta.dev)}h/{meta.dev}d\tInode: {meta.ino}\tLinks: {meta.nlink}\nAccess: ({octal(meta.mode % 4096)}/{mode_string(meta.kind, meta.mode)})\tUid: ({meta.uid}/{owner})\tGid: ({meta.gid}/{owner_group})\nAccess: {access}\nModify: {modify}\nChange: {change}\n Birth: {birth}")
}

proc main(...argv: List[Str]) [fs, error, io, time, env] {
  let opts: StatOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      dereference: {form: "-L --dereference", default: false},
      filesystem: {form: "-f --file-system", default: false},
      format: {form: "-c --format FORMAT"},
      printf: {form: "--printf FORMAT"},
      terse: {form: "-t --terse", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      paths: {form: "...PATH"},
    },
  )?

  if opts.help {
    gnu.help("""Usage: stat [OPTION]... FILE...
Display file or file system status.

  -L, --dereference   follow links
  -f, --file-system   display file system status instead of file status
  -c, --format=FORMAT use the specified format instead of the default
      --printf=FORMAT like --format, but interpret backslash escapes
  -t, --terse         print the information in terse form
      --help          display this help and exit
      --version       output version information and exit""")
    return
  }

  if opts.version {
    gnu.version("stat")
    return
  }

  if opts.paths.len() == 0 {
    gnu.missing_operand()
  }

  if opts.format != null and opts.printf != null {
    gnu.error("cannot specify both --format and --printf")
    exit 1
  }

  for item in opts.paths {
    let target = fp"{item}"
    if opts.filesystem {
      if item == "-" {
        gnu.error("using '-' to denote standard input does not work in file system mode")
        exit 1
      }
      let statfs: StatFs = fs.statvfs(target)?
      let mount = fs.mount_for(target.resolve()?)?
      if opts.terse {
        print filesystem_format("%n %i %l %t %s %S %b %f %a %c %d", target, statfs, mount)
      } else if opts.format != null or opts.printf != null {
        let format = opts.format ?? opts.printf ?? ""
        let rendered = filesystem_format(format, target, statfs, mount)
        if opts.printf != null { gnu.write_bytes(bytes.from_text(rendered)) } else { print $rendered }
      } else {
        print f"  File: {gnu.quote_maybe(target.display())}"
        print f"    ID: {hexadecimal(statfs.fsid)} Namelen: {statfs.name_max} Type: {mount.fstype} Block size: {statfs.block_size} Fundamental block size: {statfs.fragment_size}"
        print f"  Blocks: Total: {statfs.blocks} Free: {statfs.blocks_free} Available: {statfs.blocks_available}"
        print f"  Inodes: Total: {statfs.files} Free: {statfs.files_free}"
      }
      continue
    }

    let meta: StatMeta = fs.stat(target, follow_symlinks: opts.dereference)?
    if opts.terse {
      print terse_format(target, meta)?
    } else if opts.format != null or opts.printf != null {
      let format = opts.format ?? opts.printf ?? ""
      let rendered = render_format(format, target, meta)?
      if opts.printf != null { gnu.write_bytes(bytes.from_text(rendered)) } else { print $rendered }
    } else {
      print default_format(target, meta)?
    }
  }
}
