#!/bin/xsh
use lib.gnu
use lib.memory_report as report
use lib.util_linux_io as ul_io

const HELP = "\nUsage:\n lsmem [options]\n\nList the ranges of available memory with their online status.\n\nOptions:\n -J, --json           use JSON output format\n -P, --pairs          use key=\"value\" output format\n -a, --all            list each individual memory block\n -b, --bytes          print SIZE in bytes rather than in human readable format\n -n, --noheadings     don't print headings\n -o, --output <list>  output columns\n     --output-all     output all columns\n -r, --raw            use raw output format\n -S, --split <list>   split ranges by specified columns\n -s, --sysroot <dir>  use the specified directory as system root\n     --summary[=when] print summary information (never,always or only)\n\n -h, --help           display this help\n -V, --version        display version\n\nAvailable output columns:\n      RANGE  start and end address of the memory range\n       SIZE  size of the memory range\n      STATE  online status of the memory range\n  REMOVABLE  memory is removable\n      BLOCK  memory block number or blocks range\n       NODE  numa node of memory\n      ZONES  valid zones for the memory range\n CONFIGURED  configuration status of the memory range\n MEMMAP-ON-MEMORY  memmap-on-memory status of the memory range\n\nFor more details see lsmem(1)."

const LONG_OPTIONS = ["all", "bytes", "json", "noheadings", "output", "output-all", "pairs", "raw", "split", "sysroot", "summary", "help", "version"]
const SHORT_OPTIONS = ["a", "b", "J", "P", "n", "o", "r", "S", "s", "h", "V"]

proc unknown_long_option(argv: List[Str]) [env] -> Str? {
  var index = 0
  while index < argv.len() {
    let word = argv[index]
    index += 1
    if word == "--" { return null }
    if word.starts_with("--") {
      let parts = word.byte_slice(2).split("=", maxsplit: 1)
      let name = parts[0]
      let matches = [option for option in LONG_OPTIONS if option.starts_with(name)]
      if matches.is_empty() { return name }
      if matches.len() != 1 { return null }
      let option = matches[0]
      if option in ["help", "version"] { return null }
      if parts.len() > 1 and option not in ["output", "split", "sysroot", "summary"] { return null }
      if option in ["output", "split", "sysroot"] and parts.len() == 1 {
        if index == argv.len() { return null }
        index += 1
      }
      continue
    }
    if word.starts_with("-") and word != "-" {
      let flags = word.byte_slice(1)
      for position in range(flags.byte_len()) {
        let flag = flags.byte_slice(position, length: 1)
        if flag not in SHORT_OPTIONS { return null }
        if flag == "h" or flag == "V" { return null }
        if flag in ["o", "S", "s"] {
          if position + 1 == flags.byte_len() {
            if index == argv.len() { return null }
            index += 1
          }
          break
        }
      }
      continue
    }
    if let Ok(_) = env.get("POSIXLY_CORRECT") { return null }
  }
  null
}

pure sysroot_equals_at(argument: Bytes) -> Int? {
  return null when ! argument.starts_with(b"--")
  var equals: Int? = null
  for index in range(argument.len()) {
    if argument.byte_at(index) == 61 { equals = index; break }
  }
  return null when equals == null
  let at = equals
  return null when at == 2
  let name = argument.slice(2, length: at - 2).utf8() ?? ""
  let matches = [option for option in LONG_OPTIONS if option.starts_with(name)]
  if matches.len() == 1 and matches[0] == "sysroot" { at } else { null }
}

pure short_sysroot_value_start(argument: Bytes) -> Int? {
  return null when ! argument.starts_with(b"-") or argument.starts_with(b"--")
  for index in range(1, argument.len()) {
    let flag = argument.byte_at(index) ?? 0
    if flag == 115 { return index + 1 when index + 1 < argument.len() }
    if flag in [97, 98, 74, 80, 110, 114] { continue }
    return null
  }
  null
}

pure normalize_sysroot_arguments(argv: List[Bytes]) -> List[Bytes] {
  var normalized: List[Bytes] = []
  for argument in argv {
    if let Ok(_) = argument.utf8() { normalized += [argument]; continue }
    if let at = sysroot_equals_at(argument) {
      normalized += [argument.slice(0, length: at), argument.slice(at + 1, length: argument.len() - at - 1)]
    } else if let at = short_sysroot_value_start(argument) {
      normalized += [argument.slice(0, length: at), argument.slice(at, length: argument.len() - at)]
    } else {
      normalized += [argument]
    }
  }
  normalized
}

proc main(...argv: List[Bytes]) [fs, process, env, error, io] {
  if process.signal_action("PIPE")? == "default" {
    process.set_signal_action("PIPE", "default")
  }
  let prepared = gnu.prepare_arguments(normalize_sysroot_arguments(argv))
  let arguments = prepared.text
  if let option = unknown_long_option(arguments) { gnu.usage_error(f"unrecognized option: {option}") }

  let parsed = cli.applet(arguments, {
    gnu: {status: 1},
    all: {form: "-a --all", default: false},
    bytes: {form: "-b --bytes", default: false},
    json: {form: "-J --json", default: false},
    noheadings: {form: "-n --noheadings", default: false},
    output: {form: "-o --output LIST"},
    output_all: {form: "--output-all", default: false},
    pairs: {form: "-P --pairs", default: false},
    raw: {form: "-r --raw", default: false},
    split: {form: "-S --split LIST"},
    sysroot: {form: "-s --sysroot DIR"},
    summary: {form: "--summary[=WHEN]", optional_default: "only"},
    help: {form: "-h --help", default: false, stop: true},
    version: {form: "-V --version", default: false, stop: true},
    operands: {form: "...ARG"},
  })?
  let sysroot: Path? = if parsed.sysroot == null { null } else { Path.parse_bytes(gnu.argument_bytes(parsed.sysroot ?? "", prepared.raw))? }
  let opts: report.Options = {...parsed, sysroot: sysroot}
  if opts.help { ul_io.help(HELP); return }
  if opts.version { ul_io.version("lsmem"); return }
  report.check_exclusive(arguments)
  if ! opts.operands.is_empty() { gnu.usage_error("bad usage") }
  let modes = report.modes(arguments)
  if !modes.table and !modes.summary {
    gnu.error("options --{raw,json,pairs} and --summary=only are mutually exclusive")
    exit 1
  }

  var selected: List[Str] = []
  var split: List[Str] = []
  if modes.table {
    let environment_columns: Str? = if opts.output == null { if let Ok(value) = env.get("LSMEM_COLUMNS") { value } else { null } } else { null }
    selected = report.columns(opts, environment_columns)
    split = report.split_columns(opts, selected)
  }

  let root_path = if opts.sysroot == null { p"/" } else { opts.sysroot ?? p"/" }
  let root = fs.open_root(root_path)?
  defer root.close()
  let inventory = report.collect(root, split, opts.all, modes.summary)
  ul_io.write_text(report.render(inventory, opts, selected, modes))
}
