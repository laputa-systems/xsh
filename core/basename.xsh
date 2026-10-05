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

# The last path component of NAME after trailing slashes are dropped; a NAME of
# only slashes is "/". A SUFFIX is removed unless that would leave nothing.
pure basename_value(name: Str, suffix: Str) -> Str {
  let raw = bytes.from_text(name)
  var end = raw.len()
  while end > 0 and raw.byte_at(end - 1) == 47 {
    end -= 1
  }

  return "/" when end == 0 and name != ""

  let trimmed = name.byte_slice(0, length: end)
  let parts = trimmed.split("/")
  let base = if ! parts.is_empty() { parts[-1] } else { trimmed }

  return base when suffix == "" or base == suffix or ! base.ends_with(suffix)

  base.byte_slice(0, length: base.byte_len() - suffix.byte_len())
}

proc main(...argv: List[Str]) [process, env, error, io] {
  let opts: BasenameOptions = cli.applet(
    argv,
    {
      gnu: {status: 1, permute: false},
      multiple: {form: "-a --multiple", default: false},
      suffix: {form: "-s --suffix SUFFIX"},
      zero: {form: "-z --zero", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
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

  var names = opts.names
  var suffix = opts.suffix ?? ""

  if names.is_empty() {
    gnu.missing_operand()
  }

  if ! (opts.multiple or opts.suffix != null) {
    if names.len() > 2 {
      gnu.extra_operand(names[2])
    }

    if names.len() == 2 {
      suffix = names[1]
      names = [names[0]]
    }
  }

  let ending = if opts.zero { "\0" } else { "\n" }

  for name in names {
    gnu.write_text(basename_value(name, suffix) + ending)
  }
}
