#!/bin/xsh
use lib.gnu
error AppletError = Usage : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  f"usage: xsh applets/{applet_name}.xsh -- {summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  AppletError.Usage(usage(applet_name, summary))
}

pure file_type_name(kind: Str) -> Str {
  match kind {
    "dir" => "directory"
    "file" => "regular file"
    "symlink" => "symbolic link"
    else => kind
  }
}

pure stat_file_type(meta: FsStat) -> Str {
  if meta.kind == "file" and meta.size == 0 { "regular empty file" } else if meta.kind == "char" { "character special file" } else if meta.kind == "block" { "block special file" } else { file_type_name(meta.kind) }
}

pure has_bit(mode: Int, bit: Int) -> Bool {
  mode / bit % 2 == 1
}

pure mode_octal(mode: Int) -> Str {
  let bits = mode % 512
  let user_bits = bits / 64
  let group_bits = bits / 8 % 8
  let other_bits = bits % 8
  f"{user_bits}{group_bits}{other_bits}"
}

pure mode_triplet(mode: Int, read_bit: Int, write_bit: Int, exec_bit: Int) -> Str {
  let r = if has_bit(mode, read_bit) { "r" } else { "-" }
  let w = if has_bit(mode, write_bit) { "w" } else { "-" }
  let x = if has_bit(mode, exec_bit) { "x" } else { "-" }
  f"{r}{w}{x}"
}

pure mode_string(kind: Str, mode: Int) -> Str {
  let file_type = if kind == "dir" { "d" } else if kind == "symlink" { "l" } else { "-" }

  f"{file_type}{mode_triplet(mode, 0o400, 0o200, 0o100)}{mode_triplet(mode, 0o40, 0o20, 0o10)}{mode_triplet(mode, 0o4, 0o2, 0o1)}"
}

pure hex(value: Int) -> Str {
  let digits = "0123456789abcdef"
  return "0" when value == 0

  var rest = value
  var out = ""
  while rest > 0 {
    let index = rest % 16
    out = f"{digits[index..index + 1]}{out}"
    rest /= 16
  }

  out
}

type FormatResult = {output: Bytes, invalid: Str?, warning: Str?}
# Format parsing reports the invalid directive text; recover its source position from the selected argv value.
type ValueLocation = {index: Int, offset: Int}

pure diagnostic_spaces(count: Int) -> Str { [" " for _ in range(count)].join("") }

pure argument_source(program: Str, argv: List[Str]) -> Str {
  if argv.is_empty() { program } else { f"{program} {argv.join(" ")}" }
}

pure argument_column(program: Str, argv: List[Str], location: ValueLocation) -> Int {
  var prefix = program
  for index in range(location.index) { prefix = f"{prefix} {argv[index]}" }
  prefix.byte_len() + 2 + location.offset
}

pure format_location(argv: List[Str], printf: Bool) -> ValueLocation? {
  let short = "-c"
  let long = if printf { "--printf" } else { "--format" }
  let long_name = long.byte_slice(2)
  var found: ValueLocation? = null
  var index = 0
  while index < argv.len() {
    let raw = argv[index]
    if raw == "--" { return found }
    if raw == short {
      found = if index + 1 < argv.len() { {index: index + 1, offset: 0} } else { null }
      index += 2
      continue
    }
    if raw.starts_with(short) {
      found = {index: index, offset: short.byte_len()}
      index += 1
      continue
    }
    if raw.starts_with("--") {
      let equal = raw.find("=")
      let equal_at = equal ?? 0
      let name = if equal == null { raw.byte_slice(2) } else { raw.byte_slice(2, length: equal_at - 2) }
      if name != "" and long_name.starts_with(name) {
        if equal != null {
          found = {index: index, offset: equal_at + 1}
          index += 1
        } else {
          found = if index + 1 < argv.len() { {index: index + 1, offset: 0} } else { null }
          index += 2
        }
        continue
      }
    }
    index += 1
  }
  found
}

proc report_format_error(argv: List[Str], fmt: Str, printf: Bool, invalid: Str) [env, process, error] {
  gnu.error(f"'{invalid}': invalid directive")
  return when ! unix.isatty(2)
  guard let location = format_location(argv, printf) else { return }
  let offset = fmt.find(invalid) ?? 0
  let span = {index: location.index, offset: location.offset + offset}
  let column = argument_column("stat", argv, span)
  let source = argument_source("stat", argv)
  let marker = ["─" for _ in range(invalid.byte_len())].join("")
  eprint f"   ╭─[ stat:1:{column} ]\n   │\n 1 │ {source}\n   │{diagnostic_spaces(column)}{marker}\n   │\n   │ Help: a directive is %[FLAGS][WIDTH][.PRECISION]LETTER, as in %-10.2s; a literal % is written %%\n───╯"
}

pure pad_stat(text: Str, width: Int, left: Bool, zero: Bool, numeric: Bool) -> Str {
  let fill = if zero and numeric { "0" } else { " " }
  var padded = text
  while padded.byte_len() < width {
    if left {
      padded = f"{padded}{fill}"
    } else if fill == "0" and padded.starts_with("-") {
      padded = f"-{fill}{padded.byte_slice(1)}"
    } else {
      padded = f"{fill}{padded}"
    }
  }
  padded
}

pure stat_time(value: Int, precision: Int?, width: Int, left: Bool, zero: Bool) -> Str {
  let negative = value < 0
  let absolute = if negative { -value } else { value }
  let seconds = if negative and (precision == null or precision == 0) { (absolute + 999999999) / 1000000000 } else { absolute / 1000000000 }
  let fraction = absolute % 1000000000
  let sign = if negative { "-" } else { "" }
  var rendered = if negative and (precision == null or precision == 0) { f"-{seconds}" } else { f"{sign}{seconds}" }
  if let digits = precision {
    if digits > 0 {
      var fractional = f"{fraction}"
      while fractional.byte_len() < 9 { fractional = f"0{fractional}" }
      if digits <= 9 {
        rendered = f"{rendered}.{fractional.byte_slice(0, length: digits)}"
      } else {
        var extra = ""
        for _ in range(digits - 9) { extra = f"{extra}0" }
        rendered = f"{rendered}.{fractional}{extra}"
      }
    }
  }
  pad_stat(rendered, width, left, zero, true)
}

pure stat_format_byte(data: Bytes, at: Int) -> Str {
  data[at..at + 1].utf8() ?? ""
}

pure stat_c_quote(name: Str) -> Str {
  var out = "\""
  for ch in name {
    let byte = bytes.from_text(ch).byte_at(0) ?? 0
    if ch == "\\" { out += "\\\\" } else if ch == "\"" { out += "\\\"" } else if ch == "\n" { out += "\\n" } else if ch == "\r" { out += "\\r" } else if ch == "\t" { out += "\\t" } else if bytes.from_text(ch).len() == 1 and byte < 32 { out += f"\\{byte / 64}{byte / 8 % 8}{byte % 8}" } else { out += ch }
  }
  f"{out}\""
}

proc stat_quote(name: Str, always = false) [env] -> Str {
  let raw = bytes.from_text(name)
  let style = env.get_or("QUOTING_STYLE", "shell-escape") ?? "shell-escape"
  if style in ["shell-escape", "shell-escape-always"] {
    gnu.quote_bytes(raw, always: always or style.ends_with("-always"))
  } else if style in ["shell", "shell-always"] {
    let quote = always or style == "shell-always" or name.find(" ") != null or name.find("\t") != null or name.find("\n") != null or name.find("'") != null
    return name when ! quote
    if name.find("'") != null and name.find("\"") == null { f"\"{name}\"" } else { f"'{name}'" }
  } else if style == "literal" {
    name
  } else if style in ["c", "clocale"] {
    stat_c_quote(name)
  } else if style == "locale" {
    var escaped = name.replace("\\", with: "\\\\").replace("'", with: "\\'").replace("\t", with: "\\t").replace("\n", with: "\\n").replace("\r", with: "\\r")
    f"'{escaped}'"
  } else if style == "escape" {
    name.replace("\\", with: "\\\\").replace(" ", with: "\\ ").replace("\t", with: "\\t").replace("\n", with: "\\n")
  } else {
    gnu.quote_bytes(raw, always: always)
  }
}

proc stat_mount_point(target: Path) [fs, error] -> Str {
  let resolved = target.resolve()?
  let mount = fs.mount_for(resolved)?
  mount.mounted_on.display()
}

proc stat_terse(meta: FsStat, name: Str) [env] -> Str {
  f"{stat_quote(name)} {meta.size} {meta.blocks_512} {hex(meta.mode)} {meta.uid} {meta.gid} {meta.dev} {meta.nlink} {meta.ino} {meta.rdev} 0 {meta.atime_ns / 1000000000} {meta.mtime_ns / 1000000000} {meta.ctime_ns / 1000000000} {meta.blksize}"
}

proc render_fs_format(fmt: Str, target: Path, name: Str, printf = false) [fs, env, error] -> FormatResult {
  let stats = fs.statvfs(target)?
  let resolved = target.resolve()?
  let mount = fs.mount_for(resolved)?
  let data = bytes.from_text(fmt)
  let length = data.len()
  var output = b""
  var at = 0
  while at < length {
    if data.byte_at(at) != 37 {
      let start = at
      at += 1
      while at < length and data.byte_at(at) != 37 { at += 1 }
      output = bytes.concat([output, data[start..at]])
      continue
    }
    let origin = at
    at += 1
    if at == length { return {output: bytes.concat([output, b"%"]), invalid: null, warning: null} }
    let code = stat_format_byte(data, at)
    at += 1
    if code == "%" {
      output = bytes.concat([output, b"%"])
      continue
    }
    let specifier = if code == "Q" and at < length {
      let next = stat_format_byte(data, at)
      at += 1
      f"Q{next}"
    } else { code }
    if specifier not in ["a", "b", "c", "d", "f", "i", "l", "n", "s", "S", "t", "T", "Qn"] {
      return {output: output, invalid: data[origin..at].utf8() ?? "", warning: null}
    }
    let value = match specifier {
      "a" => f"{stats.blocks_available}"
      "b" => f"{stats.blocks}"
      "c" => f"{stats.files}"
      "d" => f"{stats.files_free}"
      "f" => f"{stats.blocks_free}"
      "i" => hex(stats.fsid)
      "l" => f"{stats.name_max}"
      "n" => name
      "Qn" => stat_quote(name)
      "s" => f"{stats.block_size}"
      "S" => f"{stats.fragment_size}"
      "t" => hex(stats.type_magic ?? 0)
      "T" => mount.fstype
      else => ""
    }
    output = bytes.concat([output, bytes.from_text(value)])
  }
  {output: output, invalid: null, warning: null}
}

proc render_format(fmt: Str, target: Path, name: Str, meta: FsStat, printf = false) [fs, env, time, error] -> FormatResult {
  var owner = f"{meta.uid}"
  var owner_group = f"{meta.gid}"

  if let Ok(found_user) = user.by_uid(meta.uid) {
    owner = found_user.name
  }

  if let Ok(found_group) = group.by_gid(meta.gid) {
    owner_group = found_group.name
  }

  var at = 0
  var output = b""
  var warning: Str? = null
  let data = bytes.from_text(fmt)
  let length = data.len()
  while at < length {
    let byte = data.byte_at(at) ?? 0
    if byte != 37 and (! printf or byte != 92) {
      let start = at
      at += 1
      while at < length and data.byte_at(at) != 37 and (! printf or data.byte_at(at) != 92) { at += 1 }
      output = bytes.concat([output, data[start..at]])
      continue
    }
    let ch = stat_format_byte(data, at)
    if ch == "\\" {
      at += 1
      if at >= length {
        output = bytes.concat([output, b"\\"])
        break
      }
      let escape = stat_format_byte(data, at)
      at += 1
      if escape in ["a", "b", "f", "n", "r", "t", "v", "\\"] {
        let byte = if escape == "a" { 7 } else if escape == "b" { 8 } else if escape == "f" { 12 } else if escape == "n" { 10 } else if escape == "r" { 13 } else if escape == "t" { 9 } else if escape == "v" { 11 } else { 92 }
        output = bytes.concat([output, bytes.from_ints([byte])?])
      } else if escape == "x" {
        var digits = 0
        var value = 0
        while at < length and digits < 2 {
          let nibble = "0123456789abcdef".find(stat_format_byte(data, at).lower())
          if nibble == null { break }
          value = value * 16 + (nibble ?? 0)
          at += 1
          digits += 1
        }
        if digits == 0 { warning = warning ?? "incomplete hex escape" } else { output = bytes.concat([output, bytes.from_ints([value])?]) }
      } else if escape in ["0", "1", "2", "3", "4", "5", "6", "7"] {
        var value = escape.parse_int() ?? 0
        var digits = 1
        while at < length and digits < (if escape == "0" { 4 } else { 3 }) {
          let digit = stat_format_byte(data, at)
          if "01234567".find(digit) == null { break }
          value = value * 8 + (digit.parse_int() ?? 0)
          at += 1
          digits += 1
        }
        output = bytes.concat([output, bytes.from_ints([value % 256])?])
      } else {
        output = bytes.concat([output, bytes.from_text(escape)])
      }
      continue
    }
    let origin = at
    at += 1
    if at == length { return {output: bytes.concat([output, b"%"]), invalid: null, warning: warning} }
    if stat_format_byte(data, at) == "%" {
      output = bytes.concat([output, b"%"])
      at += 1
      continue
    }
    var modifier = ""
    if stat_format_byte(data, at) in ["H", "L", "I"] {
      modifier = stat_format_byte(data, at)
      at += 1
    }
    var left = false
    var zero = false
    while at < length and stat_format_byte(data, at) in ["-", "0"] {
      let flag = stat_format_byte(data, at)
      left = left or flag == "-"
      zero = zero or flag == "0"
      at += 1
    }
    var width = 0
    while at < length and "0123456789".find(stat_format_byte(data, at)) != null {
      let next_width = width * 10 + ("0123456789".find(stat_format_byte(data, at)) ?? 0)
      width = if next_width > 1000000 { 1000000 } else { next_width }
      at += 1
    }
    var precision: Int? = null
    if at < length and stat_format_byte(data, at) == "." {
      at += 1
      var digits = 0
      var parsed = 0
      while at < length and "0123456789".find(stat_format_byte(data, at)) != null {
        let next_parsed = parsed * 10 + ("0123456789".find(stat_format_byte(data, at)) ?? 0)
        parsed = if next_parsed > 1000000 { 1000000 } else { next_parsed }
        at += 1
        digits += 1
      }
      precision = if digits == 0 { 9 } else { parsed }
    }
    if stat_format_byte(data, at) == "Q" {
      modifier = "Q"
      at += 1
    }
    if at >= length { return {output: output, invalid: data[origin..].utf8() ?? "", warning: warning} }
    let code = stat_format_byte(data, at)
    at += 1
    let specifier = if modifier == "I" { code } else { f"{modifier}{code}" }
    if specifier not in ["Hd", "Ld", "Hr", "Lr", "s", "b", "B", "f", "a", "A", "u", "g", "U", "G", "h", "i", "d", "D", "o", "r", "R", "t", "T", "X", "Y", "Z", "w", "W", "x", "y", "z", "m", "F", "n", "N", "Qn"] {
      return {output: output, invalid: data[origin..at].utf8() ?? "", warning: warning}
    }
    let numeric = code in ["s", "b", "B", "f", "a", "u", "g", "h", "i", "d", "D", "o", "r", "R", "t", "T", "X", "Y", "Z", "w", "W"]
    let value = match specifier {
      "Hd" => f"{fs.dev_major(meta.dev)}"
      "Ld" => f"{fs.dev_minor(meta.dev)}"
      "Hr" => f"{fs.dev_major(meta.rdev)}"
      "Lr" => f"{fs.dev_minor(meta.rdev)}"
      "s" => f"{meta.size}"
      "b" => f"{meta.blocks_512}"
      "B" => "512"
      "f" => hex(meta.mode)
      "a" => mode_octal(meta.mode)
      "A" => mode_string(meta.kind, meta.mode)
      "u" => f"{meta.uid}"
      "g" => f"{meta.gid}"
      "U" => owner
      "G" => owner_group
      "h" => f"{meta.nlink}"
      "i" => f"{meta.ino}"
      "d" => f"{meta.dev}"
      "D" => hex(meta.dev)
      "o" => f"{meta.blksize}"
      "r" => f"{meta.rdev}"
      "R" => hex(meta.rdev)
      "t" => hex(fs.dev_major(meta.rdev))
      "T" => hex(fs.dev_minor(meta.rdev))
      "X" => stat_time(meta.atime_ns, precision, width, left, zero)
      "Y" => stat_time(meta.mtime_ns, precision, width, left, zero)
      "Z" => stat_time(meta.ctime_ns, precision, width, left, zero)
      "w" => if meta.birth_ns == null { "-" } else { time.format(meta.birth_ns ?? 0, "%Y-%m-%d %H:%M:%S.%N %z")? }
      "W" => if meta.birth_ns == null { "0" } else { f"{(meta.birth_ns ?? 0) / 1000000000}" }
      "x" => time.format(meta.atime_ns, "%Y-%m-%d %H:%M:%S.%N %z")?
      "y" => time.format(meta.mtime_ns, "%Y-%m-%d %H:%M:%S.%N %z")?
      "z" => time.format(meta.ctime_ns, "%Y-%m-%d %H:%M:%S.%N %z")?
      "m" => stat_mount_point(target)
      "F" => file_type_name(meta.kind)
      "n" => name
      "N" => {
        let quoted_name = stat_quote(name)
        if meta.kind == "symlink" { f"{quoted_name} -> {stat_quote(target.readlink()?.display())}" } else { quoted_name }
      }
      "Qn" => stat_quote(name)
      else => ""
    }
    if code in ["X", "Y", "Z"] {
      output = bytes.concat([output, bytes.from_text(value)])
    } else if code == "n" and precision != null {
      let size = precision ?? 0
      output = bytes.concat([output, bytes.from_text(value)[..size]])
    } else {
      output = bytes.concat([output, bytes.from_text(pad_stat(value, width, left, zero, numeric))])
    }
  }
  {output: output, invalid: null, warning: warning}
}

type StatOptions = {format: Str, printf: Str?, file_system: Bool, terse: Bool, dereference: Bool, help: Bool, version: Bool, paths: List[Str]}

pure valid_stat_quote_style(style: Str) -> Bool {
  style in ["literal", "shell", "shell-always", "shell-escape", "shell-escape-always", "c", "escape", "locale", "clocale"]
}

proc main(...argv: List[Str]) [fs, env, io, time, error] {
  let opts: StatOptions = cli.applet(argv, {
    gnu: {status: 1},
    format: {form: "-c --format FORMAT", default: ""},
    printf: {form: "--printf FORMAT"},
    file_system: {form: "-f --file-system", default: false},
    terse: {form: "-t --terse", default: false},
    dereference: {form: "-L --dereference", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    paths: {form: "...PATH"},
  })?
  let {format, printf, file_system, terse, dereference, help, version, paths, ..} = opts

  if help { gnu.help("Usage: stat [OPTION]... FILE...\nDisplay file or filesystem status.\n  -L, --dereference  follow links\n  -f, --file-system  display filesystem status\n  -c, --format=FORMAT  use the specified format\n      --printf=FORMAT  use the specified format without a trailing newline\n  -t, --terse  print information in terse form\n"); return }
  if version { gnu.version("stat"); return }
  if paths.is_empty() {
    gnu.error("the following required arguments were not provided: <file>")
    exit 1
  }

  let fmt = printf ?? format
  if fmt.find("%N") != null or fmt.find("%Qn") != null {
    if let Ok(style) = env.get("QUOTING_STYLE") {
      if ! valid_stat_quote_style(style) {
        gnu.error(f"ignoring invalid value of environment variable QUOTING_STYLE: '{style}'")
      }
    }
  }

  for item in paths {
    if file_system and item == "-" {
      gnu.error("using '-' to denote standard input does not work in file system mode")
      exit 1
    }
    let name = item
    let target = if item == "-" { fp"/dev/stdin" } else { fp"{item}" }
    if file_system {
      let stats = fs.statvfs(target)?
      let mount = fs.mount_for(target.resolve()?)?
      if terse {
        let selected = if format != "" or printf != null { render_fs_format(fmt, target, name, printf != null) } else {
          {output: bytes.from_text(f"{stat_quote(name)} {stats.blocks} {stats.files} {hex(stats.fsid)} {stats.name_max} {stats.block_size} {stats.fragment_size} {hex(stats.type_magic ?? 0)} {mount.fstype}"), invalid: null, warning: null}
        }
        gnu.write_bytes(selected.output)
        if printf == null and selected.invalid == null { gnu.write_bytes(b"\n") }
        if let invalid = selected.invalid { report_format_error(argv, fmt, printf != null, invalid); exit 1 }
      } else if format != "" or printf != null {
        let rendered = render_fs_format(fmt, target, name, printf != null)
        gnu.write_bytes(rendered.output)
        if printf == null and rendered.invalid == null { gnu.write_bytes(b"\n") }
        if let invalid = rendered.invalid { report_format_error(argv, fmt, printf != null, invalid); exit 1 }
      } else {
        print f"  File: {stat_quote(name)}"
        print f"    ID: {hex(stats.fsid)} Namelen: {stats.name_max} Type: {mount.fstype}"
        print f"Block size: {stats.block_size}"
        print f"Blocks: Total: {stats.blocks} Free: {stats.blocks_free} Available: {stats.blocks_available}"
        print f"Inodes: Total: {stats.files} Free: {stats.files_free}"
      }
      continue
    }
    guard let meta = fs.stat(target, follow_symlinks: dereference or item == "-") else { |failure|
      gnu.error(f"cannot statx {gnu.quote(name)}: {gnu.strerror(failure)}")
      exit 1
    }

    if terse {
      gnu.write_text(f"{stat_terse(meta, name)}\n")
    } else if printf != null {
      let rendered = render_format(fmt, target, name, meta, printf: true)
      gnu.write_bytes(rendered.output)
      if let warning = rendered.warning { gnu.error(f"warning: {warning}") }
      if let invalid = rendered.invalid { report_format_error(argv, fmt, true, invalid); exit 1 }
    } else if format != "" {
      let rendered = render_format(fmt, target, name, meta)
      let ending = if rendered.invalid == null { b"\n" } else { b"" }
      gnu.write_bytes(bytes.concat([rendered.output, ending]))
      if let invalid = rendered.invalid { report_format_error(argv, fmt, false, invalid); exit 1 }
    } else {
      print f"  File: {stat_quote(name)}"
      print f"  Size: {meta.size} Blocks: {meta.blocks_512} IO Block: {meta.blksize} {stat_file_type(meta)}"
      print f"kind {meta.kind}"
      print f"size {meta.size}"
      print f"mode {meta.mode}"
      print f"uid {meta.uid}"
      print f"gid {meta.gid}"
      print f"path {name}"
    }
  }
}
