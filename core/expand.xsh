#!/bin/xsh
use lib.gnu
use lib.text_a2 as text

type Options = {tabs: List[Str], initial: Bool, help: Bool, version: Bool, paths: List[Str]}
proc main(...argv: List[Str]) {
  let opts: Options = cli.applet(text.numeric_options(argv, "-t"), {
    gnu: {status: 1},
    tabs: {form: "-t --tabs LIST", repeated: true},
    initial: {form: "-i --initial", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    paths: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: expand [OPTION]... [FILE]...\n  -t, --tabs=LIST   set tab stops\nWith no FILE, or when FILE is -, read standard input."); return }
  if opts.version { gnu.version("expand"); return }
  let stops = text.tabs(opts.tabs)
  let input = text.read(opts.paths)
  gnu.write_bytes(text.expand(input.data, stops, opts.initial))
  exit text.finish(input.failed)
}
