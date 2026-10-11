#!/bin/xsh
use lib.gnu
use lib.text_a2 as text

type Options = {tabs: List[Str], all: Bool, first: Bool, help: Bool, version: Bool, paths: List[Str]}
proc main(...argv: List[Bytes]) {
  let arguments = text.normalize_arguments(argv)
  # The obsolete -N form reports overflow without the value, unlike -t N. Digits-only
  # text parses to a non-negative Int, so a negative fallback marks overflow.
  for arg in arguments.values {
    break when arg == "--"
    if rx"^-[0-9]+$".matches(arg) and (arg.byte_slice(1).parse_int() ?? -1) < 0 {
      gnu.error("tab stop is too large")
      exit 1
    }
  }
  let opts: Options = cli.applet(text.numeric_options(arguments.values, "-t"), {
    gnu: {status: 1},
    tabs: {form: "-t --tabs LIST", repeated: true},
    all: {form: "-a --all", default: false}, first: {form: "--first-only", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    paths: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: unexpand [OPTION]... [FILE]...\n  -t, --tabs=LIST   set tab stops\nWith no FILE, or when FILE is -, read standard input."); return }
  if opts.version { gnu.version("unexpand"); return }
  let stops = text.tabs(opts.tabs)
  var explicit_tabs = false
  for arg in arguments.values {
    break when arg == "--"
    if arg.starts_with("-t") or arg.starts_with("--t") { explicit_tabs = true }
  }
  let input = text.read_arguments(arguments, opts.paths)
  gnu.write_bytes(text.unexpand(input.data, stops, ! opts.first and (opts.all or explicit_tabs)))
  exit text.finish(input.failed)
}
