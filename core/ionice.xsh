#!/bin/xsh
use lib.gnu
use lib.proc_launch

const USAGE = """

Usage:
 ionice [options] -p <pid>...
 ionice [options] -P <pgid>...
 ionice [options] -u <uid>...
 ionice [options] <command>

Show or change the I/O-scheduling class and priority of a process.

Options:
 -c, --class <class>    name or number of scheduling class,
                          0: none, 1: realtime, 2: best-effort, 3: idle
 -n, --classdata <num>  priority (0..7) in the specified scheduling class,
                          only for the realtime and best-effort classes
 -p, --pid <pid>...     act on these already running processes
 -P, --pgid <pgrp>...   act on already running processes in these groups
 -t, --ignore           ignore failures
 -u, --uid <uid>...     act on already running processes owned by these users

 -h, --help             display this help
 -V, --version          display version

For more details see ionice(1).
"""

type IoniceOptions = {
  class_name: Str?,
  classdata: Str?,
  pid: List[Str],
  pgid: List[Str],
  uid: List[Str],
  ignore: Bool,
  help: Bool,
  version: Bool,
  operands: List[Str],
}

const CLASSES = ["none", "realtime", "best-effort", "idle"]
const NUMBER = rx"^[ \t\n\u{b}\u{c}\r]*([+-]?)([0-9]+)$"
const DIGITS_FIRST = rx"^[0-9]"
const LEVEL_SPAN = 8192

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

# The target kind and the name its argument has in diagnostics.
type Target = {which: Str, what: Str, low: Int, high: Int}

const PROCESS = {which: "process", what: "PID", low: -2147483648, high: 2147483647}
const GROUP = {which: "group", what: "PGID", low: -2147483648, high: 2147483647}
const USER = {which: "user", what: "UID", low: 0, high: 4294967294}

proc print_priority(who: Int, target: Target) [process, env, io] {
  match process.io_priority(who, target.which) {
    Ok(priority) => {
      if priority.class == "idle" {
        gnu.write_text("idle\n")
      } else {
        gnu.write_text(f"{priority.class}: prio {priority.level}\n")
      }
    }
    Err(failure) => {
      gnu.error(f"ioprio_get failed: {gnu.strerror(failure)}")
      exit 1
    }
  }
}

# Sets the class and level the way the kernel macro combines them: a level
# past the 13 bits of its field spills into the class, and a value no class
# can hold is an invalid argument. Returns the failure text, or "" on success.
proc apply_priority(who: Int, target: Target, class: Int, level: Int) [process] -> Str {
  return "Invalid argument" when level < 0

  let value = class * LEVEL_SPAN + level
  let effective = value / LEVEL_SPAN

  return "Invalid argument" when effective > 3

  match process.set_io_priority(who, CLASSES[effective], value % LEVEL_SPAN, target.which) {
    Ok(_) => ""
    Err(failure) => gnu.strerror(failure)
  }
}

proc main(...argv: List[Str]) [process, env, error, io] {
  let opts: IoniceOptions = cli.applet(
    argv,
    {
      gnu: {status: 1, permute: false},
      class_name: {form: "-c --class CLASS"},
      classdata: {form: "-n --classdata NUM"},
      pid: {form: "-p --pid PID", repeated: true},
      pgid: {form: "-P --pgid PGID", repeated: true},
      uid: {form: "-u --uid UID", repeated: true},
      ignore: {form: "-t --ignore", default: false},
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
    gnu.version("ionice")
    return
  }

  var class = 2
  var level = 4
  var class_given = false
  var level_given = false

  if let given = opts.classdata {
    level = number_argument(given, "class data", -2147483648, 2147483647)
    level_given = true
  }

  if let given = opts.class_name {
    class_given = true

    if DIGITS_FIRST.matches(given) {
      class = number_argument(given, "class", -2147483648, 2147483647)
    } else {
      var found = -1
      var index = 0

      for name in CLASSES {
        if name == given.lower() {
          found = index
        }

        index += 1
      }

      if found < 0 {
        gnu.error(f"unknown scheduling class: '{given}'")
        exit 1
      }

      class = found
    }
  }

  let mentioned = opts.pid.len() + opts.pgid.len() + opts.uid.len()
  var target = PROCESS
  var ids: List[Str] = []

  if ! opts.pid.is_empty() {
    ids = opts.pid
    target = PROCESS
  } else if ! opts.pgid.is_empty() {
    ids = opts.pgid
    target = GROUP
  } else if ! opts.uid.is_empty() {
    ids = opts.uid
    target = USER
  }

  # The first target is checked before the conflict, as util-linux reads the
  # options in order.
  let who = if mentioned > 0 { number_argument(ids[0], target.what, target.low, target.high) } else { 0 }

  if mentioned > 1 {
    gnu.error("can handle only one of pid, pgid or uid at once")
    exit 1
  }

  let changing = class_given or level_given

  if class == 0 {
    if class_given and level_given {
      gnu.error("ignoring given class data for none class")
    }

    level = 0
  } else if class == 3 {
    if level_given {
      gnu.error("ignoring given class data for idle class")
    }

    level = 7
  } else if class != 1 and class != 2 and ! opts.ignore {
    gnu.error(f"unknown prio class {class}")
  }

  let operands = opts.operands

  if ! changing and mentioned == 0 and operands.is_empty() {
    print_priority(0, PROCESS)
    return
  }

  if mentioned > 0 {
    let rest = operands

    if ! changing {
      print_priority(who, target)

      for text in rest {
        print_priority(number_argument(text, target.what, target.low, target.high), target)
      }

      return
    }

    let first = apply_priority(who, target, class, level)

    if first != "" and ! opts.ignore {
      gnu.error(f"ioprio_set failed: {first}")
      exit 1
    }

    for text in rest {
      let next = apply_priority(number_argument(text, target.what, target.low, target.high), target, class, level)

      if next != "" and ! opts.ignore {
        gnu.error(f"ioprio_set failed: {next}")
        exit 1
      }
    }

    return
  }

  if operands.is_empty() {
    gnu.usage_error("bad usage", 1)
  }

  let failure = apply_priority(0, PROCESS, class, level)

  if failure != "" and ! opts.ignore {
    gnu.error(f"ioprio_set failed: {failure}")
    exit 1
  }

  let command = operands[0]
  let status = proc_launch.launch_status(command)

  if status != 0 {
    let reason = if status == 127 { "No such file or directory" } else { "Permission denied" }

    gnu.error(f"failed to execute {command}: {reason}")
    exit status
  }

  if let Err(failure) = io.flush_stdout() {
    gnu.write_failed(failure)
  }

  if let Err(failure) = unix.exec(process.command_argv(command, operands)) {
    gnu.error(f"failed to execute {command}: {gnu.strerror(failure)}")
    exit 126
  }
}
