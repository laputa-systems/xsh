#!/bin/xsh
use lib.gnu

const USAGE = """Usage: basename NAME [SUFFIX]
  or:  basename OPTION... NAME...
Print NAME with any leading directory components removed.
If specified, also remove a trailing SUFFIX.

  -a, --multiple       support multiple arguments and treat each as a NAME
  -s, --suffix=SUFFIX  remove a trailing SUFFIX; implies -a
  -z, --zero           end each output line with NUL, not newline
      --help           display this help and exit
      --version        output version information and exit
"""

type BasenameOptions = {
  multiple: Bool,
  suffix: Str?,
  zero: Bool,
  help: Bool,
  version: Bool,
  names: List[Str],
}

pure long_suffix_option(arg: Str) -> Bool {
  let equal_at = arg.find("=")
  let option = if equal_at == null { arg } else { arg.byte_slice(0, length: equal_at ?? 0) }
  option.byte_len() > 2 and "--suffix".starts_with(option)
}

pure short_suffix_value_start(arg: Str) -> Int? {
  return null when ! arg.starts_with("-") or arg.starts_with("--")

  var at = 1
  while at < arg.byte_len() {
    if arg.byte_slice(at, length: 1) == "s" { return at + 1 }
    at += 1
  }
  null
}

# The last path component of NAME after trailing slashes are dropped; a NAME of
# only slashes is "/". A SUFFIX is removed unless that would leave nothing.
pure raw_names(argv: List[Str], raw: List[Bytes]) -> List[Bytes] {
  var names: List[Bytes] = []
  var index = 0
  var options = true
  while index < argv.len() {
    let arg = argv[index]
    if options and arg == "--" {
      options = false
      index += 1
      continue
    }
    if options and (arg == "-a" or arg == "--multiple" or arg == "-z" or arg == "--zero" or arg == "-h" or arg == "--help" or arg == "-V" or arg == "--version") {
      index += 1
      continue
    }
    if options and long_suffix_option(arg) {
      index += if arg.find("=") == null { 2 } else { 1 }
      continue
    }
    let suffix_start = short_suffix_value_start(arg)
    if options and suffix_start != null {
      index += if (suffix_start ?? 0) >= arg.byte_len() { 2 } else { 1 }
      continue
    }
    if options and arg.starts_with("-") and arg != "-" {
      index += 1
      continue
    }
    names += [raw[index]]
    options = false
    index += 1
  }
  names
}

proc raw_suffix(argv: List[Str], raw: List[Bytes], fallback: Bytes) -> Bytes {
  var index = 0
  var options = true
  var selected: Bytes? = null
  while index < argv.len() {
    let arg = argv[index]
    if options and arg == "--" { options = false; index += 1; continue }
    if options and long_suffix_option(arg) {
      let equal_at = arg.find("=")
      if equal_at == null { selected = raw[index + 1]; index += 2 } else { selected = raw[index].slice((equal_at ?? 0) + 1); index += 1 }
      continue
    }
    let suffix_start = short_suffix_value_start(arg)
    if options and suffix_start != null {
      if (suffix_start ?? 0) >= arg.byte_len() { selected = raw[index + 1]; index += 2 } else { selected = raw[index].slice(suffix_start ?? 0); index += 1 }
      continue
    }
    if options and (! arg.starts_with("-") or arg == "-") { options = false }
    index += 1
  }
  selected ?? fallback
}

pure basename_value(name: Bytes, suffix: Bytes) -> Bytes {
  var end = name.len()
  while end > 0 and (name.byte_at(end - 1) ?? -1) == 47 {
    end -= 1
  }

  return bytes.from_text("/") when end == 0 and name.len() > 0

  let trimmed = name.slice(0, length: end)
  var start = 0
  var index = 0
  while index < trimmed.len() {
    if (trimmed.byte_at(index) ?? -1) == 47 { start = index + 1 }
    index += 1
  }
  let base = trimmed.slice(start)

  return base when suffix.len() == 0 or base == suffix or ! base.ends_with(suffix)

  base.slice(0, length: base.len() - suffix.len())
}

proc main(...argv: List[Str]) [process, env, error, io] {
  let opts: BasenameOptions = cli.applet(
    argv,
    {
      gnu: {status: 1, permute: false},
      multiple: {form: "-a --multiple", default: false},
      suffix: {form: "-s --suffix SUFFIX"},
      zero: {form: "-z --zero", default: false},
      help: {form: "-h --help", default: false, stop: true},
      version: {form: "-V --version", default: false, stop: true},
      names: {form: "...NAME"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("basename")
    return
  }

  var names = raw_names(argv, cli.argv_bytes())
  var suffix = raw_suffix(argv, cli.argv_bytes(), bytes.from_text(opts.suffix ?? ""))

  if names.len() == 0 {
    gnu.missing_operand()
  }

  if ! (opts.multiple or opts.suffix != null) {
    if names.len() > 2 {
      gnu.extra_operand(opts.names[2])
    }

    if names.len() == 2 {
      suffix = names[1]
      names = [names[0]]
    }
  }

  let ending = bytes.from_text(if opts.zero { "\0" } else { "\n" })

  for name in names {
    gnu.write_bytes(bytes.concat([basename_value(name, suffix), ending]))
  }
}
