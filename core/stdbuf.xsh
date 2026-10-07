#!/bin/xsh
use lib.gnu
use lib.proc_launch

const USAGE = """Usage: stdbuf OPTION... COMMAND [ARG]...
Change the standard stream buffering of COMMAND.
  -i, --input=MODE   standard input buffering
  -o, --output=MODE  standard output buffering
  -e, --error=MODE   standard error buffering
      --help        display this help and exit
      --version     output version information and exit
MODE is 0 (unbuffered), L (line buffered, output only), or a buffer size.
This build cannot inject a stdio buffering constructor into external commands.
"""
type Options = {input: Str?, output: Str?, error: Str?, help: Bool, version: Bool, command: List[Str]}

proc validate(mode: Str) [process, env] {
  if rx"^[0-9]*[1-9][0-9]*[ZYRQ]([i]?B)?$".matches(mode) {
    gnu.usage_error(f"invalid mode {gnu.quote_value(mode)}: Value too large for defined data type", 125)
  }
  if mode != "L" and ! rx"^[0-9]+([KMGTPEZYRQ]([i]?B)?|B)?$".matches(mode) {
    gnu.usage_error(f"invalid mode {gnu.quote_value(mode)}", 125)
  }
}

proc main(...argv: List[Str]) [process, env, error, io] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 125, permute: false},
    input: {form: "-i --input MODE"},
    output: {form: "-o --output MODE"},
    error: {form: "-e --error MODE"},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    command: {form: "...COMMAND"},
  })?
  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("stdbuf"); return }
  if opts.input == null and opts.output == null and opts.error == null {
    gnu.usage_error("you must specify a buffering mode option", 125)
  }
  if opts.command.is_empty() { gnu.missing_operand(125) }
  if opts.input == "L" { gnu.usage_error("line buffering stdin is meaningless", 125) }
  if let mode = opts.input { validate(mode) }
  if let mode = opts.output { validate(mode) }
  if let mode = opts.error { validate(mode) }
  proc_launch.check_command(opts.command[0])
  # Buffering belongs to the child libc; stream pipes cannot change setvbuf.
  gnu.error("unsupported: changing child stdio buffering requires a compatible preload library; none is shipped")
  exit 125
}
