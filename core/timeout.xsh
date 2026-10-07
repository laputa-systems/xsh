#!/bin/xsh
use lib.gnu
use lib.proc_launch

const USAGE = """Usage: timeout [OPTION] DURATION COMMAND [ARG]...
Start COMMAND, and send a signal if it is still running after DURATION.
  -s, --signal=SIGNAL  signal to send on timeout (default TERM)
  -k, --kill-after=DURATION  send KILL if COMMAND remains after this interval
  -p, --preserve-status  return COMMAND's status on timeout
  -f, --foreground  signal only COMMAND, allowing terminal interaction
  -v, --verbose  diagnose each signal sent
      --help  display this help and exit
      --version  output version information and exit
DURATION is a nonnegative number with optional s, m, h, or d suffix.
A zero DURATION disables the timeout. Timeout status is 124; KILL is 137.
"""

type Options = {signal: Str, kill_after: Str?, preserve: Bool, foreground: Bool, verbose: Bool, help: Bool, version: Bool, command: List[Str]}

const SIGNAL_POLL = 50ms

# The evaluator forwards these signals to the managed child group when each
# hook exits, while these statuses preserve timeout's shell-facing result.
on INT [] {
  exit 130
}

on TERM [] {
  exit 143
}

# Options may follow DURATION, but parsing stops at COMMAND so its flags survive.
pure normalize(argv: List[Str]) -> List[Str] {
  var prefix: List[Str] = []
  var interval_word: Str? = null
  var index = 0
  while index < argv.len() {
    let word = argv[index]
    if word == "--" {
      prefix += [word]
      if let given = interval_word { prefix += [given] }
      return prefix.extend(argv[index + 1..])
    }
    if word.starts_with("-") and word != "-" {
      prefix += [word]
      let takes_value = word == "-s" or word == "-k" or (word.byte_len() > 2 and ("--signal".starts_with(word) or "--kill-after".starts_with(word)))
      if takes_value and index + 1 < argv.len() {
        prefix += [argv[index + 1]]
        index += 1
      }
    } else if interval_word == null {
      interval_word = word
    } else {
      prefix += [interval_word]
      return prefix.extend(argv[index..])
    }
    index += 1
  }
  if let given = interval_word { prefix += [given] }
  prefix
}

proc duration(text: Str) [process, env] -> Duration {
  let parsed = proc_launch.interval(text)
  if parsed == null {
    gnu.usage_error(f"invalid time interval {gnu.quote_value(text)}", 125)
  }
  parsed ?? 0ms
}

proc send(handle: ProcessHandle, signal: Str, verbose: Bool, command: Str) [process, env, error] {
  if verbose {
    gnu.error(f"sending signal {signal} to command {gnu.quote(command)}")
  }
  # Managed children own a process group whose identifier is the leader pid.
  if let Err(failure) = process.kill_group(handle.pid, signal) {
    if failure.errno != 3 {
      gnu.error(f"failed to send signal: {gnu.strerror(failure)}")
      exit 125
    }
  }
}

# Return to the evaluator between bounded waits so signal hooks can forward
# signals to the child while its own timeout or kill-after deadline is pending.
proc wait_for(handle: ProcessHandle, duration: Duration) [process, error] -> Result[Status?] {
  var remaining = duration

  while remaining > 0ms {
    let slice = if remaining > SIGNAL_POLL { SIGNAL_POLL } else { remaining }
    let completed = process.wait_timeout([handle], slice)?

    if let finished = completed {
      return Ok(finished.status)
    }

    remaining -= slice
  }

  Ok(null)
}

proc wait_until_exit(handle: ProcessHandle) [process, error] -> Result[Status] {
  while true {
    if let status = wait_for(handle, SIGNAL_POLL)? {
      return Ok(status)
    }
  }

  return Err(error.failure("process wait loop ended unexpectedly"))
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: Options = cli.applet(normalize(argv), {
    gnu: {status: 125, permute: false},
    signal: {form: "-s --signal SIGNAL", default: "TERM"},
    kill_after: {form: "-k --kill-after DURATION"},
    preserve: {form: "-p --preserve-status", default: false},
    foreground: {form: "-f --foreground", default: false},
    verbose: {form: "-v --verbose", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    command: {form: "...COMMAND"},
  })?
  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("timeout"); return }
  if opts.command.is_empty() { gnu.missing_operand(125) }
  if opts.command.len() < 2 { gnu.missing_operand_after(opts.command[0], 125) }
  let limit = duration(opts.command[0])
  let grace = if let given = opts.kill_after { duration(given) } else { 0ms }
  if ! rx"^([0-9]+|[A-Za-z][A-Za-z0-9+-]*)$".matches(opts.signal) {
    gnu.usage_error(f"{gnu.quote_value(opts.signal)}: invalid signal", 125)
  }
  let chosen = process.signal(opts.signal)
  if let Err(_) = chosen {
    gnu.usage_error(f"{gnu.quote_value(opts.signal)}: invalid signal", 125)
    exit 125
  }
  let named = chosen?
  if opts.foreground {
    gnu.error("--foreground is unsupported: managed children require their own process group")
    exit 125
  }
  let command = opts.command[1..]
  proc_launch.check_command(command[0])
  let launched = spawn run @command
  if let Err(failure) = launched {
    gnu.error(f"failed to run command {gnu.quote(command[0])}: {failure.message}")
    exit 126
  }
  let child = launched?
  if limit == 0ms { exit (wait_until_exit(child)?).shell_code()? }
  let completed = wait_for(child, limit)?
  if let finished = completed { exit finished.shell_code()? }
  send(child, if named.number == 0 { "0" } else { named.name }, opts.verbose, command[0])
  # A stopped command must resume to observe a pending signal; signal zero
  # only probes the process and leaves nothing pending.
  if named.number != 0 and named.name != "KILL" and named.name != "CONT" {
    send(child, "CONT", false, command[0])
  }
  if grace != 0ms {
    let stopped = wait_for(child, grace)?
    if let finished = stopped {
      exit if opts.preserve { finished.shell_code()? } else if named.number == 9 { 137 } else { 124 }
    }
    send(child, "KILL", opts.verbose, command[0])
    let _ = wait_until_exit(child)?
    exit 137
  }
  let status = wait_until_exit(child)?
  exit if opts.preserve { status.shell_code()? } else if named.number == 9 { 137 } else { 124 }
}
