#!/bin/xsh
use lib.gnu
use lib.proc_launch

const USAGE = """Show or change the real-time scheduling attributes of a process.

Set policy:
 chrt [options] [<priority>] <command> [<argument>...]
 chrt --pid <policy-option> [options] [<priority>] <PID>

Get policy:
 chrt --pid <PID>

Policy options:
 -b, --batch          set policy to SCHED_BATCH
 -d, --deadline       set policy to SCHED_DEADLINE
 -e, --ext            set policy to SCHED_EXT
 -f, --fifo           set policy to SCHED_FIFO
 -i, --idle           set policy to SCHED_IDLE
 -o, --other          set policy to SCHED_OTHER
 -r, --rr             set policy to SCHED_RR (default)

Scheduling options:
 -R, --reset-on-fork       set reset-on-fork flag
 -T, --sched-runtime <ns>  runtime parameter for DEADLINE
 -P, --sched-period <ns>   period parameter for DEADLINE
 -D, --sched-deadline <ns> deadline parameter for DEADLINE

Other options:
 -a, --all-tasks      operate on all the tasks (threads) for a given pid
 -m, --max            show min and max valid priorities
 -p, --pid            operate on existing given pid
 -v, --verbose        display status information

 -h, --help           display this help
 -V, --version        display version

For more details see chrt(1).
"""

type ChrtOptions = {
  all_tasks: Bool,
  batch: Bool,
  deadline: Bool,
  ext: Bool,
  fifo: Bool,
  idle: Bool,
  other: Bool,
  rr: Bool,
  reset_on_fork: Bool,
  runtime: Str?,
  period: Str?,
  deadline_time: Str?,
  max: Bool,
  pid: Bool,
  verbose: Bool,
  help: Bool,
  version: Bool,
  operands: List[Str],
}

# The policies in the order `--max` lists them, with the names util-linux
# prints for each.
type Policy = {name: Str, label: Str}

const POLICIES = [
  {name: "other", label: "SCHED_OTHER"},
  {name: "fifo", label: "SCHED_FIFO"},
  {name: "rr", label: "SCHED_RR"},
  {name: "batch", label: "SCHED_BATCH"},
  {name: "idle", label: "SCHED_IDLE"},
  {name: "deadline", label: "SCHED_DEADLINE"},
  {name: "ext", label: "SCHED_EXT"},
]

const NUMBER = rx"^[ \t\n\u{b}\u{c}\r]*([+-]?)([0-9]+)$"
const DIGITS = rx"^[0-9]+$"

# `problem` is "" for a number, "syntax" for text that is not one, and "range"
# for a number outside the bounds asked for.
type Number = {value: Int, problem: Str}

# A decimal number as `strtol` reads it: blanks, a sign, then digits and
# nothing else.
pure parse_number(text: Str, low: Int, high: Int) -> Number {
  let parts = NUMBER.captures(text)

  return {value: 0, problem: "syntax"} when parts.is_empty()

  var digits = parts[2]

  while digits.byte_len() > 1 and digits.starts_with("0") {
    digits = digits.byte_slice(1)
  }

  return {value: 0, problem: "range"} when digits.byte_len() > 19

  let parsed = digits.parse_int_decimal()

  return {value: 0, problem: "range"} when parsed is Err(_)

  let magnitude = parsed ?? 0
  let value = if parts[1] == "-" { 0 - magnitude } else { magnitude }

  return {value: 0, problem: "range"} when value < low or value > high

  {value: value, problem: ""}
}

pure label_of(name: Str) -> Str {
  for policy in POLICIES {
    if policy.name == name {
      return policy.label
    }
  }

  "unknown"
}

proc number_argument(text: Str, what: Str, low: Int, high: Int) [process, env] -> Int {
  let parsed = parse_number(text, low, high)

  if parsed.problem == "syntax" {
    gnu.error(f"invalid {what} argument: '{text}'")
    exit 1
  }

  if parsed.problem == "range" {
    gnu.error(f"invalid {what} argument: '{text}': Result not representable")
    exit 1
  }

  parsed.value
}

# The task ids of a process, ascending, or null when it has no task directory.
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

proc shown_pid(pid: Int) [process] -> Int {
  if pid == 0 { process.current_pid() ?? 0 } else { pid }
}

proc show_max() [process, env, io] {
  var text = ""

  for policy in POLICIES {
    match process.scheduler_priorities(policy.name) {
      Ok(range) => text = f"{text}{policy.label} min/max priority\t: {range.min}/{range.max}\n"
      Err(_) => text = f"{text}{policy.label} not supported?\n"
    }
  }

  gnu.write_text(text)
}

# The block of lines describing the scheduling of one process.
proc show_info(pid: Int, label: Str, full: Bool) [process, env, io] {
  let shown = shown_pid(pid)

  match process.scheduler(pid) {
    Ok(info) => {
      let flag = if info.reset_on_fork { "|SCHED_RESET_ON_FORK" } else { "" }
      var text = f"pid {shown}'s {label} scheduling policy: {label_of(info.policy)}{flag}\n"

      if full {
        text = f"{text}pid {shown}'s {label} scheduling priority: {info.priority}\n"

        if info.policy == "deadline" {
          text = f"{text}pid {shown}'s {label} runtime/deadline/period parameters: {info.runtime_ns}/{info.deadline_ns}/{info.period_ns}\n"
        } else if info.policy == "other" or info.policy == "batch" {
          text = f"{text}pid {shown}'s {label} runtime parameter: {info.runtime_ns}\n"
        }
      }

      gnu.write_text(text)
    }
    Err(failure) => {
      gnu.error(f"failed to get pid {shown}'s policy: {gnu.strerror(failure)}")
      exit 1
    }
  }
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: ChrtOptions = cli.applet(
    argv,
    {
      gnu: {status: 1, permute: false},
      all_tasks: {form: "-a --all-tasks", default: false},
      batch: {form: "-b --batch", default: false, conflicts: ["deadline", "ext", "fifo", "idle", "other", "rr"]},
      deadline: {form: "-d --deadline", default: false, conflicts: ["batch", "ext", "fifo", "idle", "other", "rr"]},
      ext: {form: "-e --ext", default: false, conflicts: ["batch", "deadline", "fifo", "idle", "other", "rr"]},
      fifo: {form: "-f --fifo", default: false, conflicts: ["batch", "deadline", "ext", "idle", "other", "rr"]},
      idle: {form: "-i --idle", default: false, conflicts: ["batch", "deadline", "ext", "fifo", "other", "rr"]},
      other: {form: "-o --other", default: false, conflicts: ["batch", "deadline", "ext", "fifo", "idle", "rr"]},
      rr: {form: "-r --rr", default: false, conflicts: ["batch", "deadline", "ext", "fifo", "idle", "other"]},
      reset_on_fork: {form: "-R --reset-on-fork", default: false},
      runtime: {form: "-T --sched-runtime NS"},
      period: {form: "-P --sched-period NS"},
      deadline_time: {form: "-D --sched-deadline NS"},
      max: {form: "-m --max", default: false},
      pid: {form: "-p --pid", default: false},
      verbose: {form: "-v --verbose", default: false},
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
    gnu.version("chrt")
    return
  }

  # The time parameters are read where they appear on the command line, so a
  # malformed one is reported before anything else about the invocation.
  let maximum = 9223372036854775807
  var runtime = 0
  var period = 0
  var deadline = 0

  if let given = opts.runtime {
    runtime = number_argument(given, "runtime", 0, maximum)
  }

  if let given = opts.period {
    period = number_argument(given, "period", 0, maximum)
  }

  if let given = opts.deadline_time {
    deadline = number_argument(given, "deadline", 0, maximum)
  }

  if opts.max {
    show_max()
    return
  }

  let operands = opts.operands

  if operands.is_empty() {
    gnu.usage_error("too few arguments", 1)
  }

  var explicit: Str? = null

  if opts.other { explicit = "other" }
  if opts.fifo { explicit = "fifo" }
  if opts.rr { explicit = "rr" }
  if opts.batch { explicit = "batch" }
  if opts.idle { explicit = "idle" }
  if opts.deadline { explicit = "deadline" }
  if opts.ext { explicit = "ext" }

  if let chosen = explicit {
    if (opts.period != null or opts.deadline_time != null) and chosen != "deadline" {
      gnu.error("--sched-{deadline,period} options are supported for SCHED_DEADLINE only")
      exit 1
    }

    if opts.runtime != null and chosen != "other" and chosen != "batch" and chosen != "deadline" {
      gnu.error("--sched-runtime option is supported for SCHED_OTHER, SCHED_BATCH and SCHED_DEADLINE")
      exit 1
    }
  }

  let policy = explicit ?? "rr"
  let needs_priority = policy == "fifo" or policy == "rr"
  let int_max = 2147483647
  let int_min = 0 - 2147483648

  # The pid and priority of a change, or only the pid of a report.
  var priority = 0
  var pid = 0
  var command_at = 0

  if opts.pid {
    if operands.len() == 1 and explicit == null {
      let queried = number_argument(operands[0], "PID", int_min, int_max)
      var targets = [queried]

      if opts.all_tasks {
        let listed = task_ids(shown_pid(queried))

        if listed == null {
          return
        }

        targets = listed ?? []
      }

      for target in targets {
        show_info(target, "current", true)
      }

      return
    }

    if operands.len() > 1 {
      priority = number_argument(operands[0], "priority", int_min, int_max)
    } else if needs_priority {
      gnu.error(f"policy {label_of(policy)} requires a priority argument")
      exit 1
    }

    pid = number_argument(operands[operands.len() - 1], "PID", int_min, int_max)
  } else {
    if needs_priority or operands.len() > 1 {
      if DIGITS.matches(operands[0]) {
        priority = number_argument(operands[0], "priority", int_min, int_max)
        command_at = 1
      } else if needs_priority {
        gnu.error(f"policy {label_of(policy)} requires a priority argument")
        exit 1
      }
    }

    if command_at >= operands.len() {
      gnu.error("no command or priority specified")
      exit 1
    }
  }

  # A deadline reservation needs all three times; omitted ones follow the
  # kernel's ordering rule runtime <= deadline <= period.
  if policy == "deadline" {
    if deadline == 0 { deadline = period }
    if runtime == 0 { runtime = deadline }
  }

  var targets = [pid]

  if opts.pid and opts.all_tasks {
    let listed = task_ids(shown_pid(pid))

    if listed == null {
      gnu.error("cannot obtain the list of tasks: No such file or directory")
      exit 1
    }

    targets = listed ?? []
  }

  for target in targets {
    if opts.verbose and opts.pid {
      show_info(target, "current", true)
    }

    var in_range = false

    if let Ok(range) = process.scheduler_priorities(policy) {
      in_range = priority >= range.min and priority <= range.max
    }

    if ! in_range {
      gnu.error(f"unsupported priority value for the policy: {priority}: see --max for valid range")
      exit 1
    }

    # Time slices of the fair policies are always passed, 0 included, so that
    # a change of policy also restores the default slice.
    let applied = process.set_scheduler(
      target,
      policy,
      priority,
      reset_on_fork: opts.reset_on_fork,
      runtime_ns: runtime,
      deadline_ns: deadline,
      period_ns: period,
    )

    if let Err(failure) = applied {
      gnu.error(f"failed to set pid {target}'s policy: {gnu.strerror(failure)}")
      exit 1
    }

    if opts.verbose {
      show_info(if opts.pid { target } else { 0 }, "new", opts.pid)
    }
  }

  if opts.pid {
    return
  }

  let command = operands[command_at]
  let status = proc_launch.launch_status(command)

  if status != 0 {
    let reason = if status == 127 { "No such file or directory" } else { "Permission denied" }

    gnu.error(f"failed to execute {command}: {reason}")
    exit status
  }

  if let Err(failure) = io.flush_stdout() {
    gnu.write_failed(failure)
  }

  if let Err(failure) = unix.exec(process.command_argv(command, operands[command_at..])) {
    gnu.error(f"failed to execute {command}: {gnu.strerror(failure)}")
    exit 126
  }
}
