#!/bin/xsh
use lib.gnu

const USAGE = """Usage: timeout [OPTION] DURATION COMMAND [ARG]...
Start COMMAND, and kill it if still running after DURATION.

      --preserve-status  exit with the same status as COMMAND, even when the
                         command times out
      --foreground       do not create a separate process group
  -k, --kill-after=DURATION
                         also send a KILL signal if COMMAND is still running
                         this long after the initial signal was sent
  -s, --signal=SIGNAL    specify the signal to send on timeout
  -v, --verbose          diagnose the signal sent upon timeout
      --help             display this help and exit
      --version          output version information and exit

DURATION is a number with an optional suffix: 's' for seconds (the default),
'm' for minutes, 'h' for hours, or 'd' for days.
"""

type TimeoutOptions = {
  foreground: Bool,
  kill_after: Str?,
  preserve_status: Bool,
  signal: Str?,
  verbose: Bool,
  help: Bool,
  version: Bool,
  operands: List[Str],
}

type Scanned = {value: Float, rest: Str, negative: Bool, number: Str}

type Interval = {milliseconds: Int, zero: Bool}

type SignalName = {name: Str, number: Int}

const DECIMAL = rx"^([0-9]+\.?[0-9]*|\.[0-9]+)([eE][+-]?[0-9]+)?"
const HEXADECIMAL = rx"^0[xX](?:([0-9a-fA-F]+)(?:\.([0-9a-fA-F]*))?|\.([0-9a-fA-F]+))(?:[pP]([+-]?[0-9]+))?"

pure hex_digit(text: Str) -> Float {
  ("0123456789abcdef".find(text.lower()) ?? 0).float()
}

pure hex_value(whole: Str, fraction: Str, exponent: Int) -> Float {
  var value = 0.0

  for index in range(whole.byte_len()) {
    value = value * 16.0 + hex_digit(whole.byte_slice(index, length: 1))
  }

  var scale = 1.0 / 16.0
  for index in range(fraction.byte_len()) {
    value += hex_digit(fraction.byte_slice(index, length: 1)) * scale
    scale = scale / 16.0
  }

  value * 2.0.pow(exponent.float())
}

pure scan_number(text: Str) -> Scanned? {
  var body = text.trim()
  var negative = false

  if body.starts_with("-") or body.starts_with("+") {
    negative = body.starts_with("-")
    body = body.byte_slice(1)
  }

  let hex = HEXADECIMAL.captures(body)
  if hex.len() > 0 {
    let exponent = if hex[4] == "" {
      0
    } else {
      hex[4].parse_int() ?? (if hex[4].starts_with("-") { -100000 } else { 100000 })
    }
    let number = hex[0]
    return {
      value: hex_value(hex[1], hex[2] + hex[3], exponent),
      rest: body.byte_slice(number.byte_len()),
      negative: negative,
      number: number,
    }
  }

  let decimal = DECIMAL.captures(body)
  return null when decimal.len() == 0

  let number = decimal[0]
  {value: number.parse_float() ?? 0.0, rest: body.byte_slice(number.byte_len()), negative: negative, number: number}
}

pure interval(text: Str) -> Interval? {
  let scanned = scan_number(text)
  return null when scanned == null or (scanned ?? {value: 0.0, rest: "", negative: true, number: ""}).negative
  let value = scanned ?? {value: 0.0, rest: "", negative: true, number: ""}
  var seconds = value.value

  if value.rest == "" or value.rest == "s" {
    seconds = value.value
  } else if value.rest == "m" {
    seconds = value.value * 60.0
  } else if value.rest == "h" {
    seconds = value.value * 3600.0
  } else if value.rest == "d" {
    seconds = value.value * 86400.0
  } else {
    return null
  }

  var zero = true
  for index in range(value.number.byte_len()) {
    let byte = value.number.byte_slice(index, length: 1)
    if "0123456789".find(byte) != null and byte != "0" { zero = false }
  }

  return {milliseconds: 0, zero: true} when zero
  return {milliseconds: 1, zero: false} when seconds <= 0.0
  return {milliseconds: 9223372036854775807, zero: false} when seconds >= 9223372036854775.0

  let millis = seconds * 1000.0
  return {milliseconds: 1, zero: false} when millis < 1.0
  {milliseconds: millis.ceil() ?? 9223372036854775807, zero: false}
}

proc signal_by_name(text: Str) [process] -> SignalName? {
  let requested = text.upper()
  let name = if requested.starts_with("SIG") { requested.byte_slice(3) } else { requested }
  let number = requested.parse_int() ?? -1

  for signal in process.signals() {
    if (number >= 0 and signal.number == number) or signal.name.upper() == name {
      return {name: if signal.number == 0 { "0" } else { signal.name }, number: signal.number}
    }
  }

  null
}

proc command_status(command: Str) [fs, process] -> Int {
  if command.find("/") != null {
    let target = fp"{command}"

    return 127 when ! (target.exists() ?? false)
    let launch = match target.metadata() {
      Ok(entry) => if entry.kind == "dir" or ! entry.executable { 126 } else { 0 },
      Err(_) => 126,
    }
    return launch
  }

  match process.which(command) {
    Ok(_) => 0
    Err(_) => 127
  }
}

proc report_signal(verbose: Bool, signal: Str, command: Str) [process, env] {
  if verbose {
    gnu.error(f"sending signal {signal} to command {gnu.quote(command)}")
  }
}

proc signal_child(pid: Int, use_group: Bool, signal: Str) [process] {
  if use_group {
    let _ = process.kill_group(pid, signal)
  } else {
    let _ = process.kill(pid, signal)
  }
}

proc main(...argv: List[Str]) [fs, process, env, time, error, io] {
  let opts: TimeoutOptions = cli.applet(
    argv,
    {
      gnu: {status: 125, permute: false},
      foreground: {form: "-f --foreground", default: false},
      kill_after: {form: "-k --kill-after DURATION"},
      preserve_status: {form: "-p --preserve-status", default: false},
      signal: {form: "-s --signal SIGNAL"},
      verbose: {form: "-v --verbose", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      operands: {form: "...OPERAND"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("timeout")
    return
  }

  if opts.operands.len() < 2 {
    gnu.missing_operand(125)
  }

  let duration_text = opts.operands[0]
  let duration = interval(duration_text)

  if duration == null {
    gnu.usage_error(f"invalid time interval {gnu.quote_value(duration_text)}", 125)
  }

  var kill_after: Interval? = null
  if let given = opts.kill_after {
    kill_after = interval(given)
    if kill_after == null {
      gnu.usage_error(f"invalid time interval {gnu.quote_value(given)}", 125)
    }
  }

  let signal = signal_by_name(opts.signal ?? "TERM")
  if signal == null {
    gnu.usage_error(f"{gnu.quote_value(opts.signal ?? "TERM")}: invalid signal", 125)
  }

  let timeout_signal = signal ?? {name: "TERM", number: 15}

  let command_words = opts.operands[1..]
  let command = command_words[0]
  let launch = command_status(command)
  if launch != 0 {
    let reason = if launch == 127 { "No such file or directory" } else { "Permission denied" }
    gnu.error(f"failed to run command {gnu.quote(command)}: {reason}")
    exit launch
  }

  let timed = duration ?? {milliseconds: 0, zero: true}
  let grace = kill_after
  let timeout_plan = process.command_argv(
    command,
    command_words,
    new_session: ! opts.foreground,
  )
  let child = spawn timeout_plan?
  let initial = if timed.zero {
    process.wait_any([child])?
  } else {
    let waited = process.wait_timeout([child], time.millis(timed.milliseconds))?

    if let completion = waited {
      exit completion.status.shell_code()?
    }

    report_signal(opts.verbose, timeout_signal.name, command)
    signal_child(child.pid, ! opts.foreground, timeout_signal.name)

    if grace != null {
      let grace_time = grace ?? {milliseconds: 0, zero: true}

      if grace_time.zero {
        report_signal(opts.verbose, "KILL", command)
        signal_child(child.pid, ! opts.foreground, "KILL")
        let completed = process.wait_any([child])?
        exit if opts.preserve_status {
          completed.status.shell_code()?
        } else {
          if timeout_signal.number == 0 { 137 } else { 124 }
        }
      }

      let after_signal = process.wait_timeout([child], time.millis(grace_time.milliseconds))?
      if let completion = after_signal {
        exit if opts.preserve_status { completion.status.shell_code()? } else { 124 }
      }

      report_signal(opts.verbose, "KILL", command)
      signal_child(child.pid, ! opts.foreground, "KILL")
      let completed = process.wait_any([child])?
      exit if timeout_signal.number == 0 { 137 } else { 124 }
    }

    let completed = process.wait_any([child])?
    exit if opts.preserve_status { completed.status.shell_code()? } else { 124 }
  }

  exit initial.status.shell_code()?
}
