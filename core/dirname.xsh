#!/bin/xsh
use lib.gnu

type Options = {zero: Bool, help: Bool, version: Bool, paths: List[Str]}

# dirname removes components lexically, retaining dots and repeated interior slashes.
pure dirname_value(name: Str) -> Str {
  var end = name.byte_len()
  while end > 0 and name.byte_slice(end - 1, length: 1) == "/" { end -= 1 }
  return "/" when end == 0 and name != ""
  while end > 0 and name.byte_slice(end - 1, length: 1) != "/" { end -= 1 }
  return "." when end == 0
  while end > 0 and name.byte_slice(end - 1, length: 1) == "/" { end -= 1 }
  if end == 0 { "/" } else { name.byte_slice(0, length: end) }
}

proc main(...argv: List[Str]) [process, env, error, io] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1},
    zero: {form: "-z --zero", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    paths: {form: "...NAME"},
  })?
  if opts.help { gnu.help("Usage: dirname [OPTION] NAME...\nPrint NAME with its last component removed.\n  -z, --zero  end output with NUL\n"); return }
  if opts.version { gnu.version("dirname"); return }
  if opts.paths.is_empty() { gnu.missing_operand() }
  for name in opts.paths { gnu.write_text(dirname_value(name) + (if opts.zero { "\0" } else { "\n" })) }
}
