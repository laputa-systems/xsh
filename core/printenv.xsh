#!/bin/xsh
use lib.gnu

const USAGE = """Usage: printenv [OPTION]... [VARIABLE]...
Print the values of the specified environment VARIABLE(s).
If no VARIABLE is specified, print name and value pairs for them all.

  -0, --null     end each output line with NUL, not newline
      --help     display this help and exit
      --version  output version information and exit

NOTE: your shell may have its own version of printenv, which usually supersedes
the version described here.  Please refer to your shell's documentation
for details about the options it supports.
"""

type PrintenvOptions = {null: Bool, help: Bool, version: Bool, names: List[Str]}

# The environment block as its NUL-terminated `NAME=VALUE` entries, kept as
# bytes: `env.list` rejects any value that is not valid UTF-8, and GNU printenv
# prints the block unchanged.
pure nul_fields(raw: Bytes) -> List[Bytes] {
  var fields: List[Bytes] = []
  var begin = 0

  for index in range(raw.len()) {
    if raw.byte_at(index) == 0 {
      fields += [raw.slice(begin, length: index - begin)]
      begin = index + 1
    }
  }

  return fields when begin == raw.len()
  fields + [raw.slice(begin, length: raw.len() - begin)]
}

# The value of the first entry named NAME, as getenv finds it; null when unset.
# `env.path` is not used because it yields an empty path for an unset name.
pure env_value(entries: List[Bytes], name: Str) -> Bytes? {
  let prefix = bytes.from_text(f"{name}=")

  for entry in entries {
    if entry.starts_with(prefix) {
      return entry.slice(prefix.len(), length: entry.len() - prefix.len())
    }
  }

  null
}

# Exit statuses: 0 when every named variable is set, 1 when any is not, 2 for
# a usage error.
proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: PrintenvOptions = cli.applet(
    argv,
    {
      gnu: {status: 2},
      null: {form: "-0 --null", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      names: {form: "...VARIABLE"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("printenv")
    return
  }

  let ending = if opts.null { "\0" } else { "\n" }

  var entries: List[Bytes] = []

  match fp"/proc/self/environ".read_bytes() {
    Ok(raw) => {
      entries = nul_fields(raw)
    }
    Err(failure) => {
      gnu.error_reading("/proc/self/environ", failure)
      exit 1
    }
  }

  if opts.names.is_empty() {
    for entry in entries {
      gnu.write_bytes(bytes.concat([entry, bytes.from_text(ending)]))
    }

    return
  }

  var missing = false

  for name in opts.names {
    if "=" in name {
      missing = true
    } else if let value = env_value(entries, name) {
      gnu.write_bytes(bytes.concat([value, bytes.from_text(ending)]))
    } else {
      missing = true
    }
  }

  if missing {
    exit 1
  }
}
