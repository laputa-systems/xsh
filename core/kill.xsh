#!/bin/xsh
use lib.gnu

const USAGE = """Usage: kill [-s SIGNAL | -SIGNAL] PID...
  or:  kill -l [SIGNAL]...
  or:  kill -t [SIGNAL]...
Send signals to processes, or list signals.

Mandatory arguments to long options are mandatory for short options too.
  -s, --signal=SIGNAL, -SIGNAL
                   specify the name or number of the signal to be sent
  -l, --list       list signal names, or convert signal names to/from numbers
  -t, --table      print a table of signal information
      --help        display this help and exit
      --version     output version information and exit

SIGNAL may be a signal name like 'HUP', or a signal number like '1',
or the exit status of a process terminated by a signal.
PID is an integer; if negative it identifies a process group.

NOTE: your shell may have its own version of kill, which usually supersedes
the version described here.  Please refer to your shell's documentation
for details about the options it supports.
"""

type KillOptions = {
  list: Bool,
  table: Bool,
  signal: Str?,
  help: Bool,
  version: Bool,
  operands: List[Str],
}

type Sig = {name: Str, number: Int}

const SIGNAL_WORD = rx"^[A-Za-z][A-Za-z0-9+-]*$"
const DIGITS = rx"^[0-9]+$"
const PID_WORD = rx"^[ \t\n\u{b}\u{c}\r]*[+-]?[0-9]+$"

# A signal named by number or by name (any case, optional SIG prefix, RTMIN+N
# and RTMAX-N); anything else, including padded or signed numbers, is not one.
proc find_signal(text: Str) [process] -> Sig? {
  return null when ! (DIGITS.matches(text) or SIGNAL_WORD.matches(text))

  let table = process.signals()
  let bound = table[table.len() - 1].number

  match process.signal(text) {
    Ok(found) => if found.number > bound { null } else { found }
    Err(_) => null
  }
}

# The `strerror` text of a failed signal delivery.
pure delivery_text(failure: Error) -> Str {
  let code = failure.errno ?? 0

  return "No such process" when code == 3
  return "Operation not permitted" when code == 1
  return "Invalid argument" when code == 22

  gnu.strerror(failure)
}

# Write listing output and report a failed write the way GNU does.
proc emit(text: Str) [process, env, error, io] {
  gnu.write_text(text)

  if let Err(failure) = io.flush_stdout() {
    gnu.write_failed(failure)
  }
}

proc print_table() [process, env, error, io] {
  var out = ""

  for entry in process.signals() {
    let number = f"{entry.number}"
    let pad = if number.byte_len() < 2 { " " } else { "" }

    out = f"{out}{pad}{number} {entry.name}\n"
  }

  emit(out)
}

proc print_names() [process, env, error, io] {
  var out = ""

  for entry in process.signals() {
    out = f"{out}{entry.name}\n"
  }

  emit(out)
}

# What `kill -l OPERAND` prints, or null for an invalid signal: a number is a
# signal number, a wait status whose low byte is a signal, or 128 plus one, and
# prints its name (the number itself when the signal has none); a name prints
# its number.
proc list_operand(operand: Str, bound: Int) [process] -> Str? {
  if DIGITS.matches(operand) and operand.byte_len() < 12 {
    let value = operand.parse_int() ?? -1
    let low = value % 256
    var found = -1

    if low <= bound {
      found = low
    } else if value >= 128 and value - 128 <= bound {
      found = value - 128
    }

    return null when found < 0

    let named = find_signal(f"{found}")

    return null when named == null

    return named.name
  }

  let named = find_signal(operand)

  return null when named == null

  f"{named.number}"
}

proc list_signals(operands: List[Str]) [process, env, error, io] {
  if operands.len() == 0 {
    print_names()
    return
  }

  let table = process.signals()
  let bound = table[table.len() - 1].number
  var out = ""
  var failed = false

  for operand in operands {
    let shown = list_operand(operand, bound)

    if shown == null {
      gnu.error(f"{gnu.quote_value(operand)}: invalid signal")
      failed = true
    } else {
      out = f"{out}{shown}\n"
    }
  }

  emit(out)
  exit 1 when failed
}

# A process id the way `strtoimax` and `pid_t` accept it.
pure parse_pid(text: Str) -> Int? {
  return null when ! PID_WORD.matches(text)

  let digits = text.trim()
  let unsigned = if digits.starts_with("+") or digits.starts_with("-") { digits.byte_slice(1) } else { digits }

  return null when unsigned.byte_len() > 10

  let value = digits.parse_int() ?? 0

  return null when value > 2147483647 or value < -2147483648

  value
}

# Deliver `name` to one operand. Zero is the caller's own process group and a
# negative number names a group.
proc deliver(pid: Int, name: Str) [process, error] -> Result[Unit, Error] {
  return process.kill(pid, name) when pid > 0
  return process.kill_group(process.group_id()?, name) when pid == 0

  process.kill_group(0 - pid, name)
}

proc main(...argv: List[Str]) [process, env, error, io] {
  var args = argv
  var obsolete: Sig? = null

  # `-9`, `-TERM`, `-SIGTERM`: only the first argument can be one, and a name
  # starts with an upper-case letter.
  if args.len() > 0 and args[0].starts_with("-") and args[0].byte_len() > 1 {
    let word = args[0].byte_slice(1)
    let lead = word.byte_slice(0, length: 1)
    let lower = lead.lower() == lead and lead.upper() != lead

    if ! lower {
      let found = find_signal(word)

      if found != null {
        obsolete = found
        args = args[1..]
      } else if DIGITS.matches(word) or (word.byte_len() > 1 and SIGNAL_WORD.matches(word) and lead.upper() == lead) {
        gnu.error(f"{gnu.quote_value(word)}: invalid signal")
        exit 1
      }
    }
  }

  let opts: KillOptions = cli.applet(
    args,
    {
      gnu: {status: 1},
      list: {form: "-l --list", default: false},
      table: {form: "-t --table", default: false},
      signal: {form: "-s -n --signal SIGNAL"},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      operands: {form: "...PID"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("kill")
    return
  }

  if opts.list or opts.table {
    if opts.signal != null or obsolete != null {
      gnu.usage_error("cannot combine signal with -l or -t")
    }

    if opts.table and opts.operands.len() == 0 {
      print_table()
    } else {
      list_signals(opts.operands)
    }

    return
  }

  var chosen: Sig = {name: "TERM", number: 15}

  if let given = obsolete {
    chosen = given
  }

  if let text = opts.signal {
    let found = find_signal(text)

    if found == null {
      gnu.error(f"{gnu.quote_value(text)}: invalid signal")
      exit 1
    }

    chosen = found ?? chosen
  }

  if opts.operands.len() == 0 {
    gnu.usage_error("no process ID specified")
  }

  var failed = false

  for operand in opts.operands {
    let pid = parse_pid(operand)

    if pid == null {
      gnu.error(f"{gnu.quote_value(operand)}: invalid process id")
      failed = true
    } else if pid == -1 {
      gnu.error(f"{gnu.quote_value(operand)}: signaling every process is not supported")
      failed = true
    } else if let Err(failure) = deliver(pid ?? 0, chosen.name) {
      gnu.error(f"{gnu.quote_value(operand)}: {delivery_text(failure)}")
      failed = true
    }
  }

  exit 1 when failed
}
