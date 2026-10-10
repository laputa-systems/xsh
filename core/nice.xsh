#!/bin/xsh
use lib.gnu
use lib.proc_launch

const USAGE = """Usage: nice [OPTION] [COMMAND [ARG]...]
Run COMMAND with an adjusted niceness, which affects process scheduling.
With no COMMAND, print the current niceness.  Niceness values range from
-20 (most favorable to the process) to 19 (least favorable to the process).

Mandatory arguments to long options are mandatory for short options too.
  -n, --adjustment=N   add integer N to the niceness (default 10)
      --help        display this help and exit
      --version     output version information and exit

NOTE: your shell may have its own version of nice, which usually supersedes
the version described here.  Please refer to your shell's documentation
for details about the options it supports.

Exit status:
  125  if the nice command itself fails
  126  if COMMAND is found but cannot be invoked
  127  if COMMAND cannot be found
  -    the exit status of COMMAND otherwise
"""

type NiceOptions = {
  adjustment: Str?,
  help: Bool,
  version: Bool,
  command: List[Str],
}

# A legacy adjustment: `-5`, `--5` (negative), or `-+5`.
const LEGACY = rx"^-[-+]?[0-9]"
const ADJUSTMENT = rx"^[ \t\n\u{b}\u{c}\r]*[+-]?[0-9]+$"

# GNU nice accepts `-N`, `--N`, and `-+N` anywhere among the options. Turn each
# into `-n VALUE` so the option parser sees one grammar; stop at the first
# operand or `--`, after which everything belongs to the command.
pure normalize(argv: List[Str]) -> List[Str] {
  var at = 0

  let out = collect {
    while at < argv.len() {
      let word = argv[at]

      if word == "-n" or word == "--adjustment" or (word.byte_len() >= 3 and word.starts_with("--a") and "--adjustment".starts_with(
        word,
      )) {
        yield word

        yield argv[at + 1] when at + 1 < argv.len()

        at += 2
      } else if LEGACY.matches(word) {
        yield @["-n", word.byte_slice(1)]
        at += 1
      } else if word.starts_with("-") and word != "-" and word != "--" {
        yield word
        at += 1
      } else {
        yield @argv[at..]
        break
      }
    }
  }

  out
}

# The adjustment `xstrtol` reads, clamped to a range far wider than niceness:
# an out-of-range number is not an error, a malformed one is (null).
pure parse_adjustment(text: Str) -> Int? {
  return null when ! ADJUSTMENT.matches(text)

  let body = text.trim()
  let negative = body.starts_with("-")
  let digits = if body.starts_with("-") or body.starts_with("+") { body.byte_slice(1) } else { body }
  var at = 0

  while at < digits.byte_len() - 1 and digits.byte_slice(at, length: 1) == "0" {
    at += 1
  }

  let significant = digits.byte_slice(at)

  return if negative { -1000 } else { 1000 } when significant.byte_len() > 9

  let value = significant.parse_int() ?? 0
  let signed = if negative { 0 - value } else { value }

  return 1000 when signed > 1000

  return -1000 when signed < -1000

  signed
}

pure failure_text(failure: Error) -> Str {
  let code = failure.errno ?? 0

  return "Operation not permitted" when code == 1
  return "Permission denied" when code == 13
  return "Invalid argument" when code == 22

  gnu.strerror(failure)
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: NiceOptions = cli.applet(
    normalize(argv),
    {
      gnu: {status: 125, permute: false},
      adjustment: {form: "-n --adjustment N"},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      command: {form: "...COMMAND"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("nice")
    return
  }

  var adjustment = 10

  if let given = opts.adjustment {
    let parsed = parse_adjustment(given)

    if parsed == null {
      gnu.error(f"invalid adjustment {gnu.quote_value(given)}")
      exit 125
    }

    adjustment = parsed
  }

  if opts.command.is_empty() {
    if opts.adjustment != null {
      gnu.usage_error("A command must be given with an adjustment.", 125)
    }

    match process.priority() {
      Ok(current) => gnu.write_text(f"{current}\n")
      Err(failure) => {
        gnu.error(f"cannot get niceness: {failure_text(failure)}")
        exit 125
      }
    }

    return
  }

  # A refused change is a warning when it is about privilege: the command
  # still runs, at the niceness it already had. If the warning cannot be
  # written, the command does not run.
  if let Err(failure) = process.nice(adjustment) {
    if (failure.errno ?? 0) == 1 or (failure.errno ?? 0) == 13 {
      gnu.error(f"warning: setpriority: {failure_text(failure)}")
      if io.flush_stderr() is Err(_) { exit 125 }
    } else {
      gnu.error(f"cannot set niceness: {failure_text(failure)}")
      exit 125
    }
  }

  let command = opts.command[0]
  let status = proc_launch.launch_status(command)

  if status != 0 {
    let reason = if status == 127 { "No such file or directory" } else { "Permission denied" }

    gnu.error(f"{gnu.quote(command)}: {reason}")
    exit status
  }

  if let Err(failure) = io.flush_stdout() {
    gnu.write_failed(failure)
  }

  let plan = process.command_argv(command, opts.command)

  if let Err(failure) = unix.exec(plan) {
    gnu.error(f"{gnu.quote(command)}: {gnu.strerror(failure)}")
    exit 126
  }
}
