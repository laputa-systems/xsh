#!/bin/xsh
use lib.gnu

const USAGE = """Usage: who [OPTION]... [ FILE | ARG1 ARG2 ]
Print information about users who are currently logged in.

  -a, --all         same as -b -d --login -p -r -t -T -u
  -b, --boot        time of last system boot
  -d, --dead        print dead processes
  -H, --heading     print line of column headings
  -l, --login       print system login processes
      --lookup      attempt to canonicalize hostnames via DNS
  -m                only hostname and user associated with stdin
  -p, --process     print active processes spawned by init
  -q, --count       all login names and number of users logged on
  -r, --runlevel    print current runlevel
  -s, --short       print only name, line, and time (default)
  -t, --time        print last system clock change
  -T, -w, --mesg    add user's message status as +, - or ?
  -u, --users       list users logged in
      --message     same as -T
      --writable    same as -T
      --help        display this help and exit
      --version     output version information and exit

If FILE is not specified, use /var/run/utmp.  /var/log/wtmp as FILE is common.
If ARG1 ARG2 given, -m presumed: 'am i' or 'mom likes' are usual.
"""

type WhoOptions = {
  all: Bool,
  boot: Bool,
  dead: Bool,
  heading: Bool,
  login: Bool,
  lookup: Bool,
  only_hostname_user: Bool,
  process: Bool,
  count: Bool,
  runlevel: Bool,
  short: Bool,
  time: Bool,
  users: Bool,
  mesg: Bool,
  help: Bool,
  version: Bool,
  files: List[Str],
}

# Which records to print and which columns to show, as the options select them.
type Plan = {
  users: Bool,
  boot: Bool,
  dead: Bool,
  login: Bool,
  initspawn: Bool,
  runlevel: Bool,
  clockchange: Bool,
  mesg: Bool,
  idle: Bool,
  exit: Bool,
  short: Bool,
  lookup: Bool,
  mine: Bool,
  hard_time: Bool,
}

type Tz = {name: Str, offset: Int}

type Civil = {year: Int, month: Int, day: Int, hour: Int, minute: Int, second: Int}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]

proc env_text(name: Str) [env] -> Str? {
  if let Ok(text) = env.get(name) {
    text
  } else {
    null
  }
}

# The first non-empty of LC_ALL, LC_TIME, then LANG; any name but C or POSIX
# selects the numeric ISO time format, as GNU's hard_locale does.
proc hard_time_locale() [env] -> Bool {
  for name in ["LC_ALL", "LC_TIME", "LANG"] {
    let found = env_text(name) ?? ""

    return found != "C" and found != "POSIX" when found != ""
  }

  false
}

# TZ as UTC or a fixed POSIX offset (`EST5`, `<+03>-3`); zone names and DST
# rules need a tz database, so anything else reads as UTC (gaps.json).
proc load_tz() [env] -> Tz {
  let value = env_text("TZ") ?? ""
  let parts = rx"^<?([A-Za-z0-9+-]+?)>?([+-]?)([0-9]{1,2})(?::([0-9]{2}))?(?::([0-9]{2}))?$".captures(value)

  if ! parts.is_empty() {
    let seconds = (parts[3].parse_int() ?? 0) * 3600 + (parts[4].parse_int() ?? 0) * 60 + (parts[5].parse_int() ?? 0)

    return {name: parts[1], offset: if parts[2] == "-" { seconds } else { -seconds }}
  }

  {name: "UTC", offset: 0}
}

pure floor_div(a: Int, b: Int) -> Int {
  let q = a / b

  if a % b != 0 and (a < 0) != (b < 0) { q - 1 } else { q }
}

pure civil_from_seconds(seconds: Int) -> Civil {
  let days = floor_div(seconds, 86400)
  let rest = seconds - days * 86400
  let z = days + 719468
  let era = floor_div(z, 146097)
  let doe = z - era * 146097
  let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365
  let doy = doe - (365 * yoe + yoe / 4 - yoe / 100)
  let mp = (5 * doy + 2) / 153
  let day = doy - (153 * mp + 2) / 5 + 1
  let month = if mp < 10 { mp + 3 } else { mp - 9 }
  let year = yoe + era * 400 + (if month <= 2 { 1 } else { 0 })

  {year: year, month: month, day: day, hour: rest / 3600, minute: rest % 3600 / 60, second: rest % 60}
}

pure two(value: Int) -> Str {
  f"{value:02}"
}

# `%b %e %H:%M` in the C locale, `%Y-%m-%d %H:%M` otherwise.
pure time_string(seconds: Int, tz: Tz, hard: Bool) -> Str {
  let t = civil_from_seconds(seconds + tz.offset)

  if hard {
    return f"{t.year:04}-{two(t.month)}-{two(t.day)} {two(t.hour)}:{two(t.minute)}"
  }

  f"{MONTHS[t.month - 1]} {t.day:>2} {two(t.hour)}:{two(t.minute)}"
}

# One output line the way GNU print_line lays it out: user, optional message
# state, line, time, optional idle and pid columns, comment, optional exit
# text, with trailing blanks removed.
pure layout(plan: Plan, account: Str, state: Str, line: Str, stamp: Str, idle: Str, pid: Str, comment: Str, exit: Str) -> Str {
  let mesg = if plan.mesg { f" {state}" } else { "" }
  let idle_column = if plan.idle and ! plan.short { f" {pad_right(idle, 6)}" } else { "" }
  let pid_column = if plan.short { "" } else { f" {pad_left(pid, 10)}" }
  let exit_column = if plan.exit { f" {pad_right(exit, 12)}" } else { "" }
  let first = f"{pad_right(account, 8)}{mesg} {pad_right(line, 12)} {pad_right(stamp, if plan.hard_time { 16 } else { 12 })}"
  trim_end(f"{first}{idle_column}{pid_column} {pad_right(comment, 8)}{exit_column}")
}

pure pad_right(text: Str, width: Int) -> Str {
  var out = text

  while out.count_chars() < width {
    out = f"{out} "
  }

  out
}

pure pad_left(text: Str, width: Int) -> Str {
  var out = text

  while out.count_chars() < width {
    out = f" {out}"
  }

  out
}

pure trim_end(text: Str) -> Str {
  var end = text.byte_len()

  while end > 0 and text.byte_slice(end - 1, length: 1) == " " {
    end -= 1
  }

  text.byte_slice(0, length: end)
}

# GNU `--lookup` replaces a host name by its canonical name (getaddrinfo with
# AI_CANONNAME). `dns` resolves only A and AAAA records and gives no canonical
# name, so a numeric address (its own canonical form) passes and a name is a
# plain failure rather than a quietly ignored option (request: dns.canonical).
proc canonical_host(host: Str) [process, env] -> Str {
  return host when rx"^[0-9.]+$".matches(host) or rx"^[0-9A-Fa-f:]+$".matches(host)

  gnu.error(f"--lookup: cannot canonicalize host name {gnu.quote_value(host)}: canonical names are not available")
  exit 1
}

proc read_sessions(file: Path) [process, env, error] -> List[UnixUtmp] {
  match unix.read_utmp(file) {
    Ok(records) => records
    Err(failure) => {
      guard failure.errno != null else {
        gnu.error(f"{gnu.quote_maybe(file.display())}: {gnu.strerror(failure)}")
        exit 1
      }

      let none: List[UnixUtmp] = []
      none
    }
  }
}

# The default utmp file often lacks a BOOT_TIME record (containers, systemd
# hosts). Like gnulib, a file that exists but has none is given one stamped
# with the file's modification time, and a missing file one derived from the
# host uptime. An explicit FILE is reported as it is.
proc with_boot_record(sessions: List[UnixUtmp], file: Path) [fs, process, time] -> List[UnixUtmp] {
  for entry in sessions {
    return sessions when entry.kind == "boot_time"
  }

  var stamp = 0

  if let Ok(info) = fs.stat(file) {
    stamp = info.mtime_ns / 1000000000
  } else {
    guard let up = unix.uptime_seconds() else {
      return sessions
    }

    stamp = time.now() / 1000 - up
  }

  sessions + [{
    addr: "",
    exit_status: 0,
    host: "",
    id: "~~",
    kind: "boot_time",
    line: "~",
    pid: 0,
    session: 0,
    termination: 0,
    time_sec: stamp,
    time_usec: 0,
    type: 2,
    user: "reboot",
  }]
}

pure is_user(entry: UnixUtmp) -> Bool {
  entry.kind == "user_process" and entry.user != ""
}

# The idle column of a user line: `.` under a minute, HH:MM under a day, `old`
# beyond, and `?` when the terminal cannot be examined.
pure idle_text(now: Int, last: Int) -> Str {
  return "  ?  " when last == 0

  let seconds = now - last

  return "  .  " when seconds < 60
  return " old " when seconds >= 86400

  f"{two(seconds / 3600)}:{two(seconds % 3600 / 60)}"
}

proc user_line(plan: Plan, entry: UnixUtmp, tz: Tz) [fs, error, time, process, env] -> Str {
  var state = "?"
  var last = 0

  if let Ok(info) = fs.stat(if entry.line.starts_with("/") { fp"{entry.line}" } else { fp"/dev/{entry.line}" }) {
    state = if info.mode.bit_and(0o020) != 0 { "+" } else { "-" }
    last = info.atime_ns / 1000000000
  }

  var comment = ""

  if entry.host != "" {
    var host = entry.host
    var display = ""
    let colon = host.find(":")

    if colon != null {
      display = host.byte_slice(colon + 1)
      host = host.byte_slice(0, length: colon)
    }

    if plan.lookup and host != "" {
      host = canonical_host(host)
    }

    comment = if colon != null { f"({host}:{display})" } else { f"({host})" }
  }

  let now = time.now() / 1000
  let idle = if plan.idle { idle_text(now, last) } else { "" }
  let pid = if plan.idle { f"{entry.pid}" } else { "" }

  layout(plan, entry.user, state, entry.line, time_string(entry.time_sec, tz, plan.hard_time), idle, pid, comment, "")
}

pure run_level_line(plan: Plan, entry: UnixUtmp, tz: Tz) -> Str {
  let current = entry.pid % 256
  let previous = entry.pid / 256 % 256
  let last = if previous == 0 { "" } else { f"last={if previous == 78 { "S" } else { chr(previous) }}" }

  layout(plan, "", " ", f"run-level {chr(current)}", time_string(entry.time_sec, tz, plan.hard_time), "", "", last, "")
}

pure chr(code: Int) -> Str {
  return "?" when code < 32 or code > 126

  let digits = " !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{{|}}~"
  digits.byte_slice(code - 32, length: 1)
}

# The selected line for one record, or null when the plan skips it.
proc describe(plan: Plan, entry: UnixUtmp, tz: Tz) [fs, error, time, process, env] -> Str? {
  let stamp = time_string(entry.time_sec, tz, plan.hard_time)

  return user_line(plan, entry, tz) when plan.users and is_user(entry)

  return run_level_line(plan, entry, tz) when plan.runlevel and entry.kind == "run_level"

  if plan.boot and entry.kind == "boot_time" {
    return layout(plan, "", " ", "system boot", stamp, "", "", "", "")
  }

  if plan.clockchange and entry.kind == "new_time" {
    return layout(plan, "", " ", "clock change", stamp, "", "", "", "")
  }

  if plan.initspawn and entry.kind == "init_process" {
    return layout(plan, "", " ", entry.line, stamp, "", f"{entry.pid}", f"id={entry.id}", "")
  }

  if plan.login and entry.kind == "login_process" {
    return layout(plan, entry.user, " ", entry.line, stamp, "", f"{entry.pid}", f"id={entry.id}", "")
  }

  if plan.dead and entry.kind == "dead_process" {
    let status = f"term={entry.termination} exit={entry.exit_status}"
    return layout(plan, "", " ", entry.line, stamp, "", f"{entry.pid}", f"id={entry.id}", status)
  }

  null
}

# The default utmp file outlives the sessions it lists, so (as gnulib's
# READ_UTMP_CHECK_PIDS does) a user session whose process is gone is dropped.
# An explicit FILE is reported as written.
proc drop_dead_sessions(sessions: List[UnixUtmp]) [process] -> List[UnixUtmp] {
  [entry for entry in sessions if entry.kind != "user_process" or entry.pid <= 0 or process_exists(entry.pid)]
}

proc process_exists(pid: Int) [process] -> Bool {
  match process.kill(pid, "EXIT") {
    Ok(_) => true
    Err(failure) => failure.errno != 3
  }
}

proc main(...argv: List[Str]) [process, env, error, io, fs, time] {
  let opts: WhoOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      all: {form: "-a --all", default: false},
      boot: {form: "-b --boot", default: false},
      dead: {form: "-d --dead", default: false},
      heading: {form: "-H --heading", default: false},
      login: {form: "-l --login", default: false},
      lookup: {form: "--lookup", default: false},
      only_hostname_user: {form: "-m", default: false},
      process: {form: "-p --process", default: false},
      count: {form: "-q --count", default: false},
      runlevel: {form: "-r --runlevel", default: false},
      short: {form: "-s --short", default: false},
      time: {form: "-t --time", default: false},
      users: {form: "-u --users", default: false},
      mesg: {form: "-T -w --mesg --message --writable", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      files: {form: "...FILE"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("who")
    return
  }

  if opts.files.len() > 2 {
    gnu.extra_operand(opts.files[2])
  }

  let selected = opts.all or opts.boot or opts.dead or opts.login or opts.process or opts.runlevel or opts.time or opts.users
  let exits = opts.all or opts.dead
  let plan: Plan = Plan(users: opts.all or opts.users or ! selected, boot: opts.all or opts.boot, dead: opts.all or opts.dead, login: opts.all or opts.login, initspawn: opts.all or opts.process, runlevel: opts.all or opts.runlevel, clockchange: opts.all or opts.time, mesg: opts.all or opts.mesg, idle: opts.all or opts.dead or opts.login or opts.runlevel or opts.users, exit: exits, short: (opts.short or ! selected) and ! exits, lookup: opts.lookup, mine: opts.only_hostname_user or opts.files.len() == 2, hard_time: hard_time_locale())

  let file = if opts.files.len() == 1 { fp"{opts.files[0]}" } else { p"/var/run/utmp" }
  let listed = read_sessions(file)
  let sessions = if opts.files.len() == 1 { listed } else { with_boot_record(drop_dead_sessions(listed), file) }

  if opts.count {
    let names = [entry.user for entry in sessions if is_user(entry)]
    gnu.write_text(f"{names |> join(" ")}\n# users={names.len()}\n")
    finish()
    return
  }

  if opts.heading {
    gnu.write_text(layout(plan, "NAME", " ", "LINE", "TIME", "IDLE", "PID", "COMMENT", "EXIT") + "\n")
  }

  var terminal = ""

  if plan.mine {
    guard let name = unix.ttyname(0) else {
      finish()
      return
    }

    terminal = if name.starts_with("/dev/") { name.byte_slice(5) } else { name }
  }

  let tz = load_tz()

  for entry in sessions {
    continue when plan.mine and entry.line != terminal

    if let text = describe(plan, entry, tz) {
      gnu.write_text(text + "\n")
    }
  }

  finish()
}

proc finish() [process, env, io] {
  if let Err(failure) = io.flush_stdout() {
    gnu.write_failed(failure)
  }
}
