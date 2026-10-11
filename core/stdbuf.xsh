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

pure without_leading_space(mode: Str) -> Str {
  let raw = bytes.from_text(mode)
  var at = 0
  while at < raw.len() and (raw.byte_at(at) == 32 or ((raw.byte_at(at) ?? 0) >= 9 and (raw.byte_at(at) ?? 0) <= 13)) { at += 1 }
  mode.byte_slice(at)
}

# Two base-2^32 limbs retain the full unsigned size_t range while XSH Int
# remains signed. Check each multiply before narrowing its carry.
pure size_problem(mode: Str) -> Str? {
  let text = without_leading_space(mode)
  let raw = bytes.from_text(text)
  var at = if text.starts_with("+") { 1 } else { 0 }
  let start = at
  var high = 0
  var low = 0
  var overflow = false
  while at < text.byte_len() {
    let digit = raw.byte_at(at) ?? 0
    break when digit < 48 or digit > 57
    let next = low * 10 + digit - 48
    let carry = next / 4294967296
    low = next % 4294967296
    let upper = high * 10 + carry
    overflow = overflow or upper >= 4294967296
    high = upper % 4294967296
    at += 1
  }
  let suffix = text.byte_slice(at)
  let units = ["K", "M", "G", "T", "P", "E", "Z", "Y", "R", "Q"]
  var steps = 0
  var base = 1024
  var valid = suffix == ""
  let normalized_suffix = if suffix.starts_with("k") { "K" + suffix.byte_slice(1) } else { suffix }
  for index in range(units.len()) {
    let unit = units[index]
    if normalized_suffix == unit or normalized_suffix == unit + "B" or normalized_suffix == unit + "iB" {
      valid = true
      steps = index + 1
      base = if normalized_suffix == unit + "B" { 1000 } else { 1024 }
    }
  }
  if at == start {
    if start != 0 or !valid or suffix == "" { return "invalid" }
    low = 1
  }
  if !valid { return "invalid suffix in" }
  for _ in range(steps) {
    let next = low * base
    let upper = high * base + next / 4294967296
    low = next % 4294967296
    overflow = overflow or upper >= 4294967296
    high = upper % 4294967296
  }
  if overflow { "too large" } else { null }
}

proc validate(mode: Str, option: Str) [process, env] {
  let text = without_leading_space(mode)
  if option == "-i" and text.starts_with("L") {
    gnu.usage_error("line buffering standard input is meaningless", 125)
  }
  if text == "L" { return }
  if let problem = size_problem(text) {
    let argument = f"{option} argument {gnu.quote_value(text)}"
    gnu.error(if problem == "too large" { f"{argument} too large" } else { f"{problem} {argument}" })
    exit 125
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
  if let mode = opts.input { validate(mode, "-i") }
  if let mode = opts.output { validate(mode, "-o") }
  if let mode = opts.error { validate(mode, "-e") }
  if opts.command.is_empty() { gnu.missing_operand(125) }
  if opts.input == null and opts.output == null and opts.error == null {
    gnu.usage_error("you must specify a buffering mode option", 125)
  }
  proc_launch.check_command(opts.command[0])
  # Buffering belongs to the child libc; stream pipes cannot change setvbuf.
  gnu.error("unsupported: changing child stdio buffering requires a compatible preload library; none is shipped")
  exit 125
}
