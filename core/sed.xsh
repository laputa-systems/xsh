#!/bin/xsh
use lib.gnu
use lib.sed as editing

# Long options in GNU's table order; ARGUMENT is 0 (none), 1 (required), or 2
# (optional, only through `=`). Abbreviations are unique prefixes, and an
# ambiguity is reported in this order.
type LongOption = {name: Str, argument: Int, code: Str}

const LONG_OPTIONS: List[LongOption] = [
  {name: "binary", argument: 0, code: "b"},
  {name: "regexp-extended", argument: 0, code: "r"},
  {name: "debug", argument: 0, code: "debug"},
  {name: "in-place", argument: 2, code: "i"},
  {name: "expression", argument: 1, code: "e"},
  {name: "file", argument: 1, code: "f"},
  {name: "line-length", argument: 1, code: "l"},
  {name: "null-data", argument: 0, code: "z"},
  {name: "zero-terminated", argument: 0, code: "z"},
  {name: "quiet", argument: 0, code: "n"},
  {name: "posix", argument: 0, code: "posix"},
  {name: "silent", argument: 0, code: "n"},
  {name: "sandbox", argument: 0, code: "sandbox"},
  {name: "separate", argument: 0, code: "s"},
  {name: "unbuffered", argument: 0, code: "u"},
  {name: "version", argument: 0, code: "version"},
  {name: "help", argument: 0, code: "help"},
  {name: "follow-symlinks", argument: 0, code: "follow"},
]

const USAGE = "Usage: sed [OPTION]... {script-only-if-no-other-script} [input-file]...\n\n  -n, --quiet, --silent\n                 suppress automatic printing of pattern space\n      --debug\n                 annotate program execution\n  -e script, --expression=script\n                 add the script to the commands to be executed\n  -f script-file, --file=script-file\n                 add the contents of script-file to the commands to be executed\n  --follow-symlinks\n                 follow symlinks when processing in place\n  -i[SUFFIX], --in-place[=SUFFIX]\n                 edit files in place (makes backup if SUFFIX supplied)\n  -l N, --line-length=N\n                 specify the desired line-wrap length for the 'l' command\n  --posix\n                 disable all GNU extensions.\n  -E, -r, --regexp-extended\n                 use extended regular expressions in the script\n                 (for portability use POSIX -E).\n  -s, --separate\n                 consider files as separate rather than as a single,\n                 continuous long stream.\n      --sandbox\n                 operate in sandbox mode (disable e/r/w commands).\n  -u, --unbuffered\n                 load minimal amounts of data from the input files and flush\n                 the output buffers more often\n  -z, --null-data\n                 separate lines by NUL characters\n      --help     display this help and exit\n      --version  output version information and exit\n\nIf no -e, --expression, -f, or --file option is given, then the first\nnon-option argument is taken as the sed script to interpret.  All\nremaining arguments are names of input files; if no input files are\nspecified, then the standard input is read.\n\nGNU sed home page: <https://www.gnu.org/software/sed/>.\nGeneral help using GNU software: <https://www.gnu.org/gethelp/>.\nE-mail bug reports to: <bug-sed@gnu.org>.\n"

# The first line keeps the `GNU sed version` spelling that configure scripts and
# the pinned BusyBox suite probe for; GNU 4.10 itself prints `sed (GNU sed) 4.10`.
const VERSION_TEXT = "GNU sed version 4.10\nCopyright (C) 2026 Free Software Foundation, Inc.\nLicense GPLv3+: GNU GPL version 3 or later <https://gnu.org/licenses/gpl.html>.\nThis is free software: you are free to change and redistribute it.\nThere is NO WARRANTY, to the extent permitted by law.\n\nWritten by Jay Fenlason, Tom Lord, Ken Pizzini,\nPaolo Bonzini, Jim Meyering, and Assaf Gordon.\n\nThis sed program was built without SELinux support.\n\nGNU sed home page: <https://www.gnu.org/software/sed/>.\nGeneral help using GNU software: <https://www.gnu.org/gethelp/>.\nE-mail bug reports to: <bug-sed@gnu.org>.\n"

type Settings = {
  quiet: Bool,
  extended: Bool,
  separate: Bool,
  null_data: Bool,
  in_place: Bool,
  suffix: Str,
  follow: Bool,
  line_length: Int,
  sandbox: Bool,
  chunks: List[editing.Chunk],
  expressions: Int,
  operands: List[Str],
  have_script: Bool,
}

type LongScan = {settings: Settings, at: Int}

# Report a usage failure the way getopt_long's caller does: the complaint, then
# the usage text on stderr, and status 1.
proc bad_usage(message: Str) [process, env, io] -> Unit {
  if message != "" { gnu.error(message) }
  # GNU prints the bug-report address only for --help.
  let _ = io.write_stderr(USAGE.replace("E-mail bug reports to: <bug-sed@gnu.org>.\n", with: ""))
  let _ = io.flush_stderr()
  exit 1
}

# A script file is read whole; `-` is standard input. A directory reads as an
# empty script, as GNU does.
proc read_script_file(name: Str) [fs, io, error, process, env] -> Bytes {
  if name == "-" {
    match io.stdin_bytes() {
      Ok(data) => { return data }
      Err(failure) => {
        gnu.error(f"couldn't open file -: {gnu.strerror(failure)}")
        exit 4
      }
    }
  }
  match fp"{name}".read_bytes() {
    Ok(data) => { return data }
    Err(failure) => {
      if gnu.errno(failure) == 21 { return b"" }
      gnu.error(f"couldn't open file {name}: {gnu.strerror(failure)}")
      exit 4
    }
  }
  b""
}

# Apply one option, identified by its short code or long-option code.
proc apply(settings: Settings, code: Str, value: Str?) [fs, io, error, process, env] -> Settings {
  var next = settings
  match code {
    "n" => next = {...next, quiet: true}
    "r" => next = {...next, extended: true}
    "E" => next = {...next, extended: true}
    "s" => next = {...next, separate: true}
    "z" => next = {...next, null_data: true}
    "u" => {}
    "b" => {}
    "i" => next = {...next, in_place: true, separate: true, suffix: value ?? ""}
    "follow" => next = {...next, follow: true}
    "sandbox" => next = {...next, sandbox: true}
    "debug" => {
      gnu.error("--debug is not supported")
      exit 1
    }
    "posix" => {
      gnu.error("--posix is not supported")
      exit 1
    }
    "l" => next = {...next, line_length: (value ?? "0").parse_int() ?? 0}
    "e" => {
      let number = next.expressions + 1
      let chunk: editing.Chunk = {text: bytes.from_text(value ?? ""), file: null, number: number}
      next = {...next, expressions: number, have_script: true, chunks: next.chunks + [chunk]}
    }
    "f" => {
      let name = value ?? ""
      let chunk: editing.Chunk = {text: read_script_file(name), file: name, number: 0}
      next = {...next, have_script: true, chunks: next.chunks + [chunk]}
    }
    _ => {}
  }
  next
}

# Resolve a `--name[=value]` word against the long-option table.
proc long_option(settings: Settings, word: Str, rest: List[Str], at: Int) [fs, io, error, process, env] -> LongScan {
  var name = word.byte_slice(2)
  var inline: Str? = null
  if let eq = name.find("=") {
    inline = name.byte_slice(eq + 1)
    name = name.byte_slice(0, eq)
  }
  var found: List[LongOption] = []
  for option in LONG_OPTIONS {
    if option.name == name {
      found = [option]
      break
    }
    if option.name.starts_with(name) { found += [option] }
  }
  if found.is_empty() { bad_usage(f"unrecognized option '--{name}'") }
  let chosen = found[0]
  if found.len() > 1 {
    var same = true
    var listed: List[Str] = []
    for option in found {
      if option.argument != chosen.argument or option.code != chosen.code { same = false }
      listed += [f"'--{option.name}'"]
    }
    if !same { bad_usage(f"option '--{name}' is ambiguous; possibilities: {listed.join(" ")}") }
  }
  var cursor = at
  var value: Str? = inline
  if chosen.argument == 0 and inline != null { bad_usage(f"option '--{chosen.name}' doesn't allow an argument") }
  if chosen.argument == 1 and inline == null {
    if cursor >= rest.len() { bad_usage(f"option '--{chosen.name}' requires an argument") }
    value = rest[cursor]
    cursor += 1
  }
  if chosen.code == "help" {
    gnu.write_text(USAGE)
    exit 0
  }
  if chosen.code == "version" {
    gnu.write_text(VERSION_TEXT)
    exit 0
  }
  {settings: apply(settings, chosen.code, value), at: cursor}
}

proc main(...argv: List[Str]) [fs, error, io, process, env] {
  var settings: Settings = {quiet: false, extended: false, separate: false, null_data: false, in_place: false, suffix: "", follow: false, line_length: 70, sandbox: false, chunks: [], expressions: 0, operands: [], have_script: false}
  var at = 0
  var only_operands = false
  while at < argv.len() {
    let word = argv[at]
    at += 1
    if only_operands or word == "-" or !word.starts_with("-") {
      settings = {...settings, operands: settings.operands + [word]}
      continue
    }
    if word == "--" {
      only_operands = true
      continue
    }
    if word.starts_with("--") {
      let done = long_option(settings, word, argv, at)
      settings = done.settings
      at = done.at
      continue
    }
    var index = 1
    while index < word.byte_len() {
      let letter = word.byte_slice(index, 1)
      index += 1
      if letter in ["n", "r", "E", "s", "u", "z", "b"] {
        settings = apply(settings, letter, null)
      } else if letter == "i" {
        settings = apply(settings, "i", word.byte_slice(index))
        break
      } else if letter in ["e", "f", "l"] {
        var value = word.byte_slice(index)
        if value == "" {
          if at >= argv.len() { bad_usage(f"option requires an argument -- '{letter}'") }
          value = argv[at]
          at += 1
        }
        settings = apply(settings, letter, value)
        break
      } else {
        bad_usage(f"invalid option -- '{letter}'")
      }
    }
  }
  var operands = settings.operands
  var chunks = settings.chunks
  if !settings.have_script {
    if operands.is_empty() { bad_usage("") }
    let number = settings.expressions + 1
    let chunk: editing.Chunk = {text: bytes.from_text(operands[0]), file: null, number: number}
    chunks += [chunk]
    operands = operands[1..]
  }
  let options: editing.Options = {extended: settings.extended, sandbox: settings.sandbox, separate: settings.separate, null_data: settings.null_data, line_length: settings.line_length}
  let parsed = match editing.compile(chunks, options) {
    Ok(value) => value
    Err(failure) => {
      gnu.error(failure.message)
      exit 1
    }
  }
  var program = match editing.link(parsed) {
    Ok(value) => value
    Err(failure) => {
      gnu.error(failure.message)
      exit 4
    }
  }
  if settings.quiet { program = {...program, quiet: true} }
  let status = editing.execute(program, options, {enabled: settings.in_place, suffix: settings.suffix, follow: settings.follow}, operands)
  exit status
}
