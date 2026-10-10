#!/bin/xsh
use lib.gnu
use lib.proc_launch

const USAGE = """

Usage:
 prlimit [options] [--<resource>=<limit>] [-p PID]
 prlimit [options] [--<resource>=<limit>] COMMAND

Show or change the resource limits of a process.

Options:
 -p, --pid <pid>        process id
 -o, --output <list>    define which output columns to use
     --noheadings       don't print headings
     --raw              use the raw output format
     --verbose          verbose output
 -h, --help             display this help
 -V, --version          display version

Resources:
 -c, --core             maximum size of core files created
 -d, --data             maximum size of a process's data segment
 -e, --nice             maximum nice priority allowed to raise
 -f, --fsize            maximum size of files written by the process
 -i, --sigpending       maximum number of pending signals
 -l, --memlock          maximum size a process may lock into memory
 -m, --rss              maximum resident set size
 -n, --nofile           maximum number of open files
 -q, --msgqueue         maximum bytes in POSIX message queues
 -r, --rtprio           maximum real-time scheduling priority
 -s, --stack            maximum stack size
 -t, --cpu              maximum amount of CPU time in seconds
 -u, --nproc            maximum number of user processes
 -v, --as               size of virtual memory
 -x, --locks            maximum number of file locks
 -y, --rttime           CPU time in microseconds a process scheduled
                        under real-time scheduling

Arguments:
 <limit> is defined as a range soft:hard, soft:, :hard or a value to
         define both limits (e.g. -e=0:10 -r=:10).

Available output columns:
 DESCRIPTION  resource description
    RESOURCE  resource name
        SOFT  soft limit
        HARD  hard limit (ceiling)
       UNITS  units

For more details see prlimit(1).
"""

# One resource: its option letter, the name `process.rlimit` knows it by, and
# the text of the listing, which is in the order util-linux prints it.
type Resource = {name: Str, letter: Str, rlimit: Str, description: Str, units: Str}

const RESOURCES = [
  {name: "AS", letter: "v", rlimit: "as", description: "address space limit", units: "bytes"},
  {name: "CORE", letter: "c", rlimit: "core", description: "max core file size", units: "bytes"},
  {name: "CPU", letter: "t", rlimit: "cpu", description: "CPU time", units: "seconds"},
  {name: "DATA", letter: "d", rlimit: "data", description: "max data size", units: "bytes"},
  {name: "FSIZE", letter: "f", rlimit: "fsize", description: "max file size", units: "bytes"},
  {name: "LOCKS", letter: "x", rlimit: "locks", description: "max number of file locks held", units: "locks"},
  {name: "MEMLOCK", letter: "l", rlimit: "memlock", description: "max locked-in-memory address space", units: "bytes"},
  {name: "MSGQUEUE", letter: "q", rlimit: "msgqueue", description: "max bytes in POSIX mqueues", units: "bytes"},
  {name: "NICE", letter: "e", rlimit: "nice", description: "max nice prio allowed to raise", units: ""},
  {name: "NOFILE", letter: "n", rlimit: "nofile", description: "max number of open files", units: "files"},
  {name: "NPROC", letter: "u", rlimit: "nproc", description: "max number of processes", units: "processes"},
  {name: "RSS", letter: "m", rlimit: "rss", description: "max resident set size", units: "bytes"},
  {name: "RTPRIO", letter: "r", rlimit: "rtprio", description: "max real-time priority", units: ""},
  {name: "RTTIME", letter: "y", rlimit: "rttime", description: "timeout for real-time tasks", units: "microsecs"},
  {name: "SIGPENDING", letter: "i", rlimit: "sigpending", description: "max number of pending signals", units: "signals"},
  {name: "STACK", letter: "s", rlimit: "stack", description: "max stack size", units: "bytes"},
]

const COLUMNS = ["RESOURCE", "DESCRIPTION", "SOFT", "HARD", "UNITS"]
const FLAG_OPTIONS = ["noheadings", "raw", "verbose", "help", "version"]
const VALUE_OPTIONS = ["pid", "output"]
const NUMBER = rx"^[ \t\n\u{b}\u{c}\r]*([+-]?)([0-9]+)$"

# A limit request: `value` is the text after the equals sign, or null when the
# option only asks to show the resource.
type Request = {resource: Int, value: Str?}

type Invocation = {
  requests: List[Request],
  pid: Str?,
  output: Str?,
  noheadings: Bool,
  raw: Bool,
  verbose: Bool,
  rest: List[Str],
}

# What a bound of a limit was written as: kept (omitted), unlimited, a
# number, or a number beyond what Int holds.
type Bound = {kind: Str, value: Int}

# `kind` is "ok", "parse", or "range".
type Number = {kind: Str, value: Int}

# A row of the listing, one text per column.
type Row = {resource: Str, description: Str, soft: Str, hard: Str, units: Str}

pure resource_of_letter(letter: Str) -> Int {
  var at = 0

  for resource in RESOURCES {
    if resource.letter == letter {
      return at
    }

    at += 1
  }

  -1
}

pure resource_of_name(name: Str) -> Int {
  var at = 0

  for resource in RESOURCES {
    if resource.rlimit == name {
      return at
    }

    at += 1
  }

  -1
}

# The long option names an abbreviation can stand for, in alphabetical order.
pure long_names() -> List[Str] {
  var names: List[Str] = FLAG_OPTIONS.extend(VALUE_OPTIONS)

  for resource in RESOURCES {
    names += [resource.rlimit]
  }

  names |> sort
}

pure matching_names(prefix: Str) -> List[Str] {
  var found: List[Str] = []
  var exact = false

  for name in long_names() {
    if name == prefix {
      exact = true
    }

    if name.starts_with(prefix) {
      found += [name]
    }
  }

  if exact { [prefix] } else { found }
}

# A decimal limit as `strtoul` reads it: blanks, a sign, digits and nothing
# else. A minus sign wraps, which is how `-1` spells unlimited.
pure parse_limit(text: Str) -> Bound? {
  return {kind: "unlimited", value: 0} when text == "unlimited"

  let parts = NUMBER.captures(text)

  return null when parts.is_empty()

  var digits = parts[2]

  while digits.byte_len() > 1 and digits.starts_with("0") {
    digits = digits.byte_slice(1)
  }

  let negative = parts[1] == "-"

  return {kind: "unlimited", value: 0} when negative and digits == "1"
  return {kind: "huge", value: 0} when negative and digits != "0"
  return null when digits.byte_len() > 20

  let parsed = digits.parse_int_decimal()

  # Between 2^63 and 2^64 - 2 the number is a valid limit that Int cannot hold.
  if parsed is Err(_) {
    return null when digits.byte_len() > 20 or digits > "18446744073709551615"
    return {kind: "unlimited", value: 0} when digits == "18446744073709551615"

    return {kind: "huge", value: 0}
  }

  {kind: "value", value: parsed ?? 0}
}

# One side of `soft:hard`, where nothing at all keeps the limit as it is.
pure parse_bound(text: Str) -> Bound? {
  return {kind: "keep", value: 0} when text == ""

  parse_limit(text)
}

pure parse_integer(text: Str, low: Int, high: Int) -> Number {
  let parts = NUMBER.captures(text)

  return {kind: "parse", value: 0} when parts.is_empty()

  var digits = parts[2]

  while digits.byte_len() > 1 and digits.starts_with("0") {
    digits = digits.byte_slice(1)
  }

  return {kind: "range", value: 0} when digits.byte_len() > 19

  let parsed = digits.parse_int_decimal()

  return {kind: "range", value: 0} when parsed is Err(_)

  let magnitude = parsed ?? 0
  let value = if parts[1] == "-" { 0 - magnitude } else { magnitude }

  return {kind: "range", value: 0} when value < low or value > high

  {kind: "ok", value: value}
}

pure limit_text(limit: Int?) -> Str {
  if let value = limit { f"{value}" } else { "unlimited" }
}

pure exceeds(left: Int?, right: Int?) -> Bool {
  return false when left != null and right == null
  return left != null or right != null when left == null or right == null

  (left ?? 0) > (right ?? 0)
}

pure escaped(text: Str) -> Str {
  text.replace("\\", with: "\\x5c").replace(" ", with: "\\x20")
}

pure cell(row: Row, column: Str) -> Str {
  return row.resource when column == "RESOURCE"
  return row.description when column == "DESCRIPTION"
  return row.soft when column == "SOFT"
  return row.hard when column == "HARD"

  row.units
}

pure padded(text: Str, width: Int, right: Bool, last: Bool) -> Str {
  return text when last and ! right

  var fill = ""

  for _ in range(width - text.byte_len()) {
    fill = f"{fill} "
  }

  if right { f"{fill}{text}" } else { f"{text}{fill}" }
}

# The table the way libsmartcols prints it: single spaces between columns,
# every column but the last padded to its widest cell, SOFT and HARD right
# aligned. `--raw` drops the padding and escapes blanks.
pure render(rows: List[Row], columns: List[Str], headings: Bool, raw: Bool) -> Str {
  var widths: List[Int] = []

  for column in columns {
    var width = if headings { column.byte_len() } else { 0 }

    for row in rows {
      let size = cell(row, column).byte_len()

      if size > width {
        width = size
      }
    }

    widths += [width]
  }

  var lines: List[Str] = []

  if headings {
    var pieces: List[Str] = []

    for index in range(columns.len()) {
      pieces += [if raw { columns[index] } else { padded(columns[index], widths[index], columns[index] == "SOFT" or columns[index] == "HARD", index == columns.len() - 1) }]
    }

    lines += [pieces.join(" ")]
  }

  for row in rows {
    var pieces: List[Str] = []

    for index in range(columns.len()) {
      let column = columns[index]
      let text = cell(row, column)

      pieces += [if raw { escaped(text) } else { padded(text, widths[index], column == "SOFT" or column == "HARD", index == columns.len() - 1) }]
    }

    lines += [pieces.join(" ")]
  }

  var out = ""

  for line in lines {
    out = f"{out}{line}\n"
  }

  out
}

proc fail(message: Str) [process, env] {
  gnu.error(message)
  exit 1
}

# The output columns named by `-o`, or null after the diagnostic. An unknown
# name is reported with the rest of the list that follows it, as util-linux
# does.
proc parse_columns(text: Str) [process, env, error] -> List[Str] {
  var columns: List[Str] = []
  var consumed = 0

  for element in text.split(",") {
    let rest = text.byte_slice(consumed)

    if element == "" {
      if rest == "" or consumed + 1 >= text.byte_len() {
        fail(f"unknown column: {text}")
      }

      exit 1
    }

    let name = element.upper()

    if ! (name in COLUMNS) {
      fail(f"unknown column: {rest}")
    }

    columns += [name]
    consumed += element.byte_len() + 1
  }

  columns
}

proc option_error(message: Str) [process, env] {
  gnu.usage_error(message, 1)
}

# Reads the option words up to the first operand. `--help` and `--version` act
# the moment they are read, as `getopt_long` loops do, so later words are not
# examined.
proc read_options(argv: List[Str]) [process, env, error, io] -> Invocation {
  var requests: List[Request] = []
  var pid: Str? = null
  var output: Str? = null
  var noheadings = false
  var raw = false
  var verbose = false
  var at = 0

  while at < argv.len() {
    let word = argv[at]

    if word == "--" {
      at += 1
      break
    }

    if ! word.starts_with("-") or word == "-" {
      break
    }

    at += 1

    if word.starts_with("--") {
      let body = word.byte_slice(2)
      let equals = body.find("=")
      let given = if equals == null { body } else { body.byte_slice(0, equals ?? 0) }
      let attached: Str? = if equals == null { null } else { body.byte_slice((equals ?? 0) + 1) }
      let found = matching_names(given)

      if found.is_empty() {
        option_error(f"unrecognized option '--{given}'")
      }

      if found.len() > 1 {
        let listed = found |> map f"'--{.}'"

        option_error(f"option '--{given}' is ambiguous; possibilities: {listed.join(" ")}")
      }

      let name = found[0]

      if name == "help" {
        gnu.help(USAGE)
        exit 0
      }

      if name == "version" {
        gnu.version("prlimit")
        exit 0
      }

      if name in FLAG_OPTIONS {
        if attached != null {
          option_error(f"option '--{name}' doesn't allow an argument")
        }

        if name == "noheadings" { noheadings = true }
        if name == "raw" { raw = true }
        if name == "verbose" { verbose = true }
      } else if name in VALUE_OPTIONS {
        var value = attached

        if value == null {
          if at >= argv.len() {
            option_error(f"option '--{name}' requires an argument")
          }

          value = argv[at]
          at += 1
        }

        if name == "pid" { pid = value } else { output = value }
      } else {
        requests += [{resource: resource_of_name(name), value: attached}]
      }

      continue
    }

    # A short cluster: a letter that takes an argument ends it.
    var index = 1

    while index < word.byte_len() {
      let letter = word.byte_slice(index, length: 1)
      let rest = word.byte_slice(index + 1)
      let resource = resource_of_letter(letter)

      if resource >= 0 {
        var value: Str? = null

        if rest.starts_with("=") {
          value = rest.byte_slice(1)
        } else if rest != "" {
          value = rest
        }

        requests += [{resource: resource, value: value}]
        break
      }

      if letter == "h" {
        gnu.help(USAGE)
        exit 0
      }

      if letter == "V" {
        gnu.version("prlimit")
        exit 0
      }

      if letter == "p" or letter == "o" {
        var value: Str? = null

        if rest != "" {
          value = rest
        } else {
          if at >= argv.len() {
            option_error(f"option requires an argument -- '{letter}'")
          }

          value = argv[at]
          at += 1
        }

        if letter == "p" { pid = value } else { output = value }

        break
      }

      option_error(f"invalid option -- '{letter}'")
    }
  }

  {requests: requests, pid: pid, output: output, noheadings: noheadings, raw: raw, verbose: verbose, rest: argv[at..]}
}

proc nr_open() [fs] -> Int? {
  var found: Int? = null

  if let Ok(text) = fp"/proc/sys/fs/nr_open".read_text() {
    if let Ok(value) = text.trim().parse_int_decimal() {
      found = value
    }
  }

  found
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let invocation = read_options(argv)
  var pid = 0

  if let given = invocation.pid {
    let parsed = parse_integer(given, 1, 2147483647)

    if parsed.kind == "parse" {
      fail(f"invalid PID argument: '{given}'")
    }

    if parsed.kind == "range" {
      fail(f"invalid PID argument: '{given}': Result not representable")
    }

    pid = parsed.value
  }

  var columns = COLUMNS

  if let named = invocation.output {
    if named == "" {
      exit 1
    }

    columns = parse_columns(named)
  }

  if pid != 0 and ! invocation.rest.is_empty() {
    fail("options --pid and COMMAND are mutually exclusive")
  }

  let shown_pid = if pid == 0 { process.current_pid()? } else { pid }
  var requests = invocation.requests

  if requests.is_empty() {
    var at = 0

    for _ in RESOURCES {
      requests += [{resource: at, value: null}]
      at += 1
    }
  }

  let ceiling = nr_open()
  var rows: List[Row] = []

  for request in requests {
    let resource = RESOURCES[request.resource]

    if let text = request.value {
      let pieces = text.split(":")
      var soft: Bound? = null
      var hard: Bound? = null

      if text == "" or text == ":" or pieces.len() > 2 {
        fail(f"failed to parse {resource.name} limit")
      }

      if pieces.len() == 1 {
        soft = parse_limit(pieces[0])
        hard = soft
      } else {
        soft = parse_bound(pieces[0])
        hard = parse_bound(pieces[1])
      }

      if soft == null or hard == null {
        fail(f"failed to parse {resource.name} limit")
      }

      let wanted_soft = soft ?? {kind: "keep", value: 0}
      let wanted_hard = hard ?? {kind: "keep", value: 0}

      for bound in [wanted_soft, wanted_hard] {
        if resource.rlimit == "nofile" {
          let over = bound.kind == "unlimited" or bound.kind == "huge" or (bound.kind == "value" and ceiling != null and bound.value > (ceiling ?? 0))

          if over {
            fail(f"the NOFILE resource limit is not allowed to exceed {ceiling ?? 0} (fs.nr_open)")
          }
        } else if bound.kind == "huge" {
          fail(f"the {resource.name} limit is beyond the largest value this implementation passes to the kernel")
        }
      }

      var new_soft: Int? = null
      var new_hard: Int? = null

      # The bounds left out keep the limits the process has now.
      if wanted_soft.kind == "keep" or wanted_hard.kind == "keep" {
        match process.rlimit(resource.rlimit, pid: pid) {
          Ok(current) => {
            new_soft = current.soft
            new_hard = current.hard
          }
          Err(failure) => fail(f"failed to get the {resource.name} resource limit: {gnu.strerror(failure)}")
        }
      }

      if wanted_soft.kind == "unlimited" { new_soft = null }
      if wanted_soft.kind == "value" { new_soft = wanted_soft.value }
      if wanted_hard.kind == "unlimited" { new_hard = null }
      if wanted_hard.kind == "value" { new_hard = wanted_hard.value }

      if exceeds(new_soft, new_hard) {
        fail(f"the soft limit {resource.name} cannot exceed the hard limit")
      }

      if invocation.verbose {
        gnu.write_text(f"New {resource.name} limit for pid {shown_pid}: <{limit_text(new_soft)}:{limit_text(new_hard)}>\n")
      }

      if let Err(failure) = process.set_rlimit(resource.rlimit, soft: new_soft, hard: new_hard, pid: pid) {
        fail(f"failed to set the {resource.name} resource limit: {gnu.strerror(failure)}")
      }
    } else {
      match process.rlimit(resource.rlimit, pid: pid) {
        Ok(current) => {
          rows += [{resource: resource.name, description: resource.description, soft: limit_text(current.soft), hard: limit_text(current.hard), units: resource.units}]
        }
        Err(failure) => fail(f"failed to get the {resource.name} resource limit: {gnu.strerror(failure)}")
      }
    }
  }

  var shown = false

  for request in requests {
    if request.value == null {
      shown = true
    }
  }

  if shown {
    gnu.write_text(render(rows, columns, ! invocation.noheadings, invocation.raw))
  }

  if invocation.rest.is_empty() {
    return
  }

  let command = invocation.rest[0]
  let status = proc_launch.launch_status(command)

  if status != 0 {
    let reason = if status == 127 { "No such file or directory" } else { "Permission denied" }

    gnu.error(f"failed to execute {command}: {reason}")
    exit status
  }

  if let Err(failure) = unix.exec(process.command_argv(command, invocation.rest)) {
    gnu.error(f"failed to execute {command}: {gnu.strerror(failure)}")
    exit 126
  }
}
