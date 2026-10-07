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
type RawArgument = {marker: Str, value: Bytes}
type PreparedArguments = {text: List[Str], raw: List[RawArgument]}

# The shared option parser takes text; NUL-marked operands preserve Unix argv bytes.
pure prepare_arguments(argv: List[Bytes]) -> PreparedArguments {
  var text: List[Str] = []
  var raw: List[RawArgument] = []
  for index in range(argv.len()) {
    let argument = argv[index]
    match argument.utf8() {
      Ok(value) => text += [value]
      Err(_) => {
        let marker = f"\0basename-raw-argument-{index}\0"
        text += [marker]
        raw += [{marker: marker, value: argument}]
      }
    }
  }
  {text: text, raw: raw}
}

pure argument_bytes(value: Str, raw: List[RawArgument]) -> Bytes {
  for argument in raw {
    if argument.marker == value { return argument.value }
  }
  bytes.from_text(value)
}

# The last path component of NAME after trailing slashes are dropped; a NAME of
# only slashes is "/". A SUFFIX is removed unless that would leave nothing.
pure basename_value(name: Bytes, suffix: Bytes) -> Bytes {
  var end = name.len()
  while end > 0 and name.byte_at(end - 1) == 47 { end -= 1 }
  return b"/" when end == 0 and name != b""

  var begin = end
  while begin > 0 and name.byte_at(begin - 1) != 47 { begin -= 1 }
  let base = name.slice(begin, length: end - begin)
  return base when suffix == b"" or base == suffix or ! base.ends_with(suffix)
  base.slice(0, length: base.len() - suffix.len())
}

proc main(...argv: List[Bytes]) [process, env, error, io] {
  let prepared = prepare_arguments(argv)
  let opts: BasenameOptions = cli.applet(
    prepared.text,
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

  var names: List[Bytes] = []
  for name in opts.names { names += [argument_bytes(name, prepared.raw)] }
  var suffix = argument_bytes(opts.suffix ?? "", prepared.raw)

  if names.is_empty() {
    gnu.missing_operand()
  }

  if ! (opts.multiple or opts.suffix != null) {
    if names.len() > 2 {
      let extra = names[2]
      match extra.utf8() {
        Ok(value) => gnu.extra_operand(value)
        Err(_) => gnu.usage_error(f"extra operand {gnu.quote_bytes(extra, always: true)}")
      }
    }

    if names.len() == 2 {
      suffix = names[1]
      names = [names[0]]
    }
  }

  let ending = if opts.zero { "\0" } else { "\n" }

  for name in names {
    gnu.write_bytes(bytes.concat([basename_value(name, suffix), bytes.from_text(ending)]))
  }
}
