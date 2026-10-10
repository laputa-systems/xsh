#!/bin/xsh
use lib.gnu
use lib.proc_launch

const USAGE = """Usage: taskset [options] [mask | cpu-list] [pid|cmd [args...]]


Show or change the CPU affinity of a process.

Options:
 -a, --all-tasks         operate on all the tasks (threads) for a given pid
 -p, --pid               operate on existing given pid
 -c, --cpu-list          display and specify cpus in list format
 -h, --help              display this help
 -V, --version           display version

The default behavior is to run a new command:
    taskset 03 sshd -b 1024
You can retrieve the mask of an existing task:
    taskset -p 700
Or set it:
    taskset -p 03 700
List format uses a comma-separated list instead of a mask:
    taskset -pc 0,3,7-11 700
Ranges in list format can take a stride argument:
    e.g. 0-31:2 is equivalent to mask 0x55555555

For more details see taskset(1).
"""

type TasksetOptions = {
  all_tasks: Bool,
  pid: Bool,
  cpu_list: Bool,
  help: Bool,
  version: Bool,
  operands: List[Str],
}

# Kernels configure at most 8192 CPUs, and the kernel ignores mask bits past
# its own width, so CPUs from here up are dropped instead of tracked.
const CPU_LIMIT = 8192

const LIST_ELEMENT = rx"^([0-9]+)(?:-([0-9]+)(?::([0-9]+))?)?$"
const MASK_TEXT = rx"^[0-9a-fA-F,]*$"
const PID_TEXT = rx"^[ \t\n\u{b}\u{c}\r]*([+-]?)([0-9]+)$"
const HEX_DIGITS = "0123456789abcdef"

type PidText = {value: Int, range: Bool}

# A decimal count; digit runs too long for Int clamp to a value past every
# CPU number.
pure count_of(digits: Str) -> Int {
  return 1000000000 when digits.byte_len() > 9

  digits.parse_int_decimal() ?? 0
}

pure sorted_unique(values: List[Int]) -> List[Int] {
  let ordered = values |> sort
  var out: List[Int] = []

  for value in ordered {
    if out.is_empty() or out[-1] != value {
      out += [value]
    }
  }

  out
}

# The CPUs of a list such as `0,3,7-11` or `0-31:2`, ascending, or null when
# the text is not a list. Spaces, empty elements, and a reversed range are
# errors, as in util-linux.
pure parse_list(text: Str) -> List[Int]? {
  return null when text == ""

  var cpus: List[Int] = []

  for element in text.split(",") {
    let parts = LIST_ELEMENT.captures(element)

    return null when parts.is_empty()

    let first = count_of(parts[1])
    let last = if parts[2] == "" { first } else { count_of(parts[2]) }
    let stride = if parts[3] == "" { 1 } else { count_of(parts[3]) }

    return null when last < first or stride < 1

    var at = first
    let end = if last < CPU_LIMIT { last } else { CPU_LIMIT - 1 }

    while at <= end {
      cpus += [at]
      at += stride
    }
  }

  sorted_unique(cpus)
}

# The CPUs of a hexadecimal mask, with an optional lowercase `0x`, ascending,
# or null when the text is not a mask. A single comma may separate digit
# groups and may end the text; it may not start it.
pure parse_mask(text: Str) -> List[Int]? {
  let body = if text.byte_len() > 1 and text.starts_with("0x") { text.byte_slice(2) } else { text }

  return null when ! MASK_TEXT.matches(body)

  var cpus: List[Int] = []
  var number = 0
  var at = body.byte_len() - 1

  while at >= 0 {
    var digit_text = body.byte_slice(at, length: 1)

    if digit_text == "," {
      at -= 1

      return null when at < 0

      digit_text = body.byte_slice(at, length: 1)
    }

    let digit = HEX_DIGITS.find(digit_text.lower())

    return null when digit == null

    let value = digit ?? 0

    for bit in range(4) {
      let weight = [1, 2, 4, 8][bit]

      if value / weight % 2 == 1 and number + bit < CPU_LIMIT {
        cpus += [number + bit]
      }
    }

    number += 4
    at -= 1
  }

  cpus
}

# The hexadecimal mask of ascending CPU numbers, without leading zeros.
pure format_mask(cpus: List[Int]) -> Str {
  return "0" when cpus.is_empty()

  var nibbles: List[Int] = []

  for _ in range(cpus[-1] / 4 + 1) {
    nibbles += [0]
  }

  for id in cpus {
    nibbles[id / 4] = nibbles[id / 4] + [1, 2, 4, 8][id % 4]
  }

  var out = ""
  var at = nibbles.len() - 1

  while at >= 0 {
    out = f"{out}{HEX_DIGITS.byte_slice(nibbles[at], length: 1)}"
    at -= 1
  }

  out
}

# The list util-linux prints: a run of three or more CPUs a constant distance
# apart is one range, with the distance after a colon unless it is 1. Each run
# starts at the first CPU not yet written; a shorter run is written one CPU at a
# time, so the tail of a two-CPU run can start the next range.
pure format_list(cpus: List[Int]) -> Str {
  var entries: List[Str] = []
  var at = 0

  while at < cpus.len() {
    if at + 1 >= cpus.len() {
      entries += [f"{cpus[at]}"]
      break
    }

    let stride = cpus[at + 1] - cpus[at]
    var last = at + 1

    while last + 1 < cpus.len() and cpus[last + 1] - cpus[last] == stride {
      last += 1
    }

    if last - at >= 2 {
      let range = f"{cpus[at]}-{cpus[last]}"

      entries += [if stride == 1 { range } else { f"{range}:{stride}" }]
      at = last + 1
    } else {
      entries += [f"{cpus[at]}"]
      at += 1
    }
  }

  entries.join(",")
}

pure format_affinity(cpus: List[Int], as_list: Bool) -> Str {
  if as_list { format_list(cpus) } else { format_mask(cpus) }
}

# A pid as `strtol` reads it: blanks, a sign, then decimal digits and nothing
# else. null is a syntax error; `range` marks a number outside 32 bits.
pure parse_pid(text: Str) -> PidText? {
  let parts = PID_TEXT.captures(text)

  return null when parts.is_empty()

  var digits = parts[2]

  while digits.byte_len() > 1 and digits.starts_with("0") {
    digits = digits.byte_slice(1)
  }

  let negative = parts[1] == "-"

  return {value: 0, range: true} when digits.byte_len() > 10

  let magnitude = digits.parse_int_decimal() ?? 0
  let limit = if negative { 2147483648 } else { 2147483647 }

  return {value: 0, range: true} when magnitude > limit

  {value: if negative { 0 - magnitude } else { magnitude }, range: false}
}

proc pid_argument(text: Str) [process, env] -> Int {
  let parsed = parse_pid(text)

  if parsed == null {
    gnu.error(f"invalid PID argument: '{text}'")
    exit 1
  }

  if parsed.range {
    gnu.error(f"invalid PID argument: '{text}': Result not representable")
    exit 1
  }

  parsed.value
}

# The task ids of a process, ascending, or null when the process has no task
# directory.
proc task_ids(pid: Int) [fs, error] -> List[Int]? {
  match fs.children(fp"/proc/{pid}/task") {
    Ok(entries) => {
      var ids: List[Int] = []

      for entry in entries {
        ids += [entry.name.parse_int_decimal() ?? 0]
      }

      ids |> sort
    }
    Err(_) => null
  }
}

proc show(pid: Int, label: Str, as_list: Bool) [process, env, io] {
  let shown = if pid == 0 { process.current_pid() ?? 0 } else { pid }

  match process.affinity(pid) {
    Ok(cpus) => {
      let kind = if as_list { "list" } else { "mask" }

      gnu.write_text(f"pid {shown}'s {label} affinity {kind}: {format_affinity(cpus, as_list)}\n")
    }
    Err(failure) => {
      gnu.error(f"failed to get pid {shown}'s affinity: {gnu.strerror(failure)}")
      exit 1
    }
  }
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: TasksetOptions = cli.applet(
    argv,
    {
      gnu: {status: 1, permute: false},
      all_tasks: {form: "-a --all-tasks", default: false},
      pid: {form: "-p --pid", default: false},
      cpu_list: {form: "-c --cpu-list", default: false},
      help: {form: "-h --help", default: false, stop: true},
      version: {form: "-V --version", default: false, stop: true},
      operands: {form: "...ARG"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("taskset")
    return
  }

  let operands = opts.operands
  let count = operands.len()

  if opts.pid {
    # With no operand at all, util-linux reads the last command-line word as
    # the pid, which is always the option that asked for pid mode.
    if count == 0 {
      let _ = pid_argument(argv[-1])
    }

    if count > 2 {
      gnu.usage_error("bad usage", 1)
    }
  } else if count < 2 {
    gnu.usage_error("bad usage", 1)
  }

  let setting = if opts.pid { count == 2 } else { true }
  var requested: List[Int] = []

  let pid = if opts.pid { pid_argument(operands[count - 1]) } else { 0 }

  if setting {
    let wanted = if opts.cpu_list { parse_list(operands[0]) } else { parse_mask(operands[0]) }

    if wanted == null {
      let kind = if opts.cpu_list { "list" } else { "mask" }

      gnu.error(f"failed to parse CPU {kind}: {operands[0]}")
      exit 1
    }

    requested = wanted ?? []
  }

  if ! opts.pid {
    let own = process.current_pid()?

    if let Err(failure) = process.set_affinity(0, requested) {
      gnu.error(f"failed to set pid {own}'s affinity: {gnu.strerror(failure)}")
      exit 1
    }

    let command = operands[1]

    if let Err(failure) = io.flush_stdout() {
      gnu.write_failed(failure)
    }

    let status = proc_launch.launch_status(command)

    if status != 0 {
      let reason = if status == 127 { "No such file or directory" } else { "Permission denied" }

      gnu.error(f"failed to execute {command}: {reason}")
      exit status
    }

    if let Err(failure) = unix.exec(process.command_argv(command, operands[1..])) {
      gnu.error(f"failed to execute {command}: {gnu.strerror(failure)}")
      exit 126
    }

    return
  }

  var targets = [pid]

  if opts.all_tasks {
    let listed = task_ids(if pid == 0 { process.current_pid()? } else { pid })

    # A process without a task directory has nothing to report or change.
    if listed == null {
      return
    }

    targets = listed ?? []
  }

  for target in targets {
    show(target, "current", opts.cpu_list)

    if setting {
      if let Err(failure) = process.set_affinity(target, requested) {
        let shown = if target == 0 { process.current_pid()? } else { target }

        gnu.error(f"failed to set pid {shown}'s affinity: {gnu.strerror(failure)}")
        exit 1
      }

      show(target, "new", opts.cpu_list)
    }
  }
}
