#!/bin/xsh
use lib.gnu

const USAGE = """Usage: uptime [OPTION]... [FILE]
Print the current time, the length of time the system has been up,
the number of users on the system, and the average number of jobs
in the run queue over the last 1, 5 and 15 minutes.
Processes in an uninterruptible sleep state also contribute to the load average.
If FILE is not specified, use /var/run/utmp.  /var/log/wtmp as FILE is common.

  -p, --pretty   show uptime in pretty format
  -s, --since    system up since
      --help     display this help and exit
      --version  output version information and exit
"""

type UptimeOptions = {pretty: Bool, since: Bool, help: Bool, version: Bool, files: List[Str]}

# Seconds the host has been up, or null when no boot time can be found, and
# the number of user sessions the utmp source lists.
type Boot = {uptime: Int?, users: Int, problem: Str?}

type Tz = {name: Str, offset: Int}

type Civil = {year: Int, month: Int, day: Int, hour: Int, minute: Int, second: Int}

proc env_text(name: Str) [env] -> Str? {
  if let Ok(text) = env.get(name) {
    text
  } else {
    null
  }
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

pure plural(count: Int, singular: Str, many: Str) -> Str {
  if count == 1 { f"{count} {singular}" } else { f"{count} {many}" }
}

# The `up` text of the default line: `up  1:47` or `up 3 days  1:47`.
pure up_text(uptime: Int) -> Str {
  let days = uptime / 86400
  let hours = uptime % 86400 / 3600
  let minutes = uptime % 3600 / 60
  let clock = f"{hours:>2}:{two(minutes)}"

  return f"up {plural(days, "day", "days")} {clock}" when days > 0

  f"up {clock}"
}

# procps `-p`: weeks, days, hours and minutes, omitting zero units except a
# lone `0 minutes`.
pure pretty_text(uptime: Int) -> Str {
  let weeks = uptime / 604800
  let days = uptime % 604800 / 86400
  let hours = uptime % 86400 / 3600
  let minutes = uptime % 3600 / 60
  var parts: List[Str] = []

  if weeks > 0 {
    parts += [plural(weeks, "week", "weeks")]
  }

  if days > 0 {
    parts += [plural(days, "day", "days")]
  }

  if hours > 0 {
    parts += [plural(hours, "hour", "hours")]
  }

  if minutes > 0 or parts.is_empty() {
    parts += [plural(minutes, "minute", "minutes")]
  }

  f"up {parts |> join(", ")}"
}

proc read_sessions(file: Path) [process, env, error] -> Result[List[UnixUtmp], Error] {
  unix.read_utmp(file)
}

# The boot time and user count of an explicit utmp file. A file that cannot be
# read has no boot time and the failure names why; a FIFO is refused before it
# is opened, since opening it would wait for a writer.
proc boot_from_file(file: Path, now: Int) [fs, process, env, error] -> Boot {
  if let Ok(info) = fs.stat(file) {
    return {uptime: null, users: 0, problem: "Illegal seek"} when info.kind == "fifo"
  }

  match read_sessions(file) {
    Ok(records) => {
      var booted = 0
      var users = 0

      for entry in records {
        if entry.kind == "user_process" and entry.user != "" {
          users += 1
        } else if entry.kind == "boot_time" {
          booted = entry.time_sec
        }
      }

      {uptime: if booted == 0 { null } else { now - booted }, users: users, problem: ""}
    }
    Err(failure) => {
      {uptime: null, users: 0, problem: gnu.strerror(failure)}
    }
  }
}

# With no operand the kernel's own uptime is used, and the default utmp file
# only supplies the user count (a missing file lists nobody, as with glibc).
proc boot_from_host() [fs, process, env, error] -> Boot {
  var users = 0

  if let Ok(records) = read_sessions(p"/var/run/utmp") {
    users = [entry for entry in records if entry.kind == "user_process" and entry.user != ""].len()
  }

  match unix.uptime_seconds() {
    Ok(seconds) => {
      {uptime: seconds, users: users, problem: ""}
    }
    Err(failure) => {
      {uptime: null, users: users, problem: gnu.strerror(failure)}
    }
  }
}

proc load_text() [process, error] -> Str {
  if let Ok(load) = unix.load_average() {
    f",  load average: {load.one.format(2)}, {load.five.format(2)}, {load.fifteen.format(2)}"
  } else {
    ","
  }
}

proc main(...argv: List[Str]) [process, env, error, io, fs, time] {
  let opts: UptimeOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      pretty: {form: "-p --pretty", default: false},
      since: {form: "-s --since", default: false},
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
    gnu.version("uptime")
    return
  }

  if opts.files.len() > 1 {
    gnu.extra_operand(opts.files[1])
  }

  let now = time.now() / 1000
  let boot = if opts.files.len() == 1 { boot_from_file(fp"{opts.files[0]}", now) } else { boot_from_host() }
  let tz = load_tz()
  var status = 0

  if boot.uptime == null {
    let reason = boot.problem ?? ""
    gnu.error(if reason == "" { "couldn't get boot time" } else { f"couldn't get boot time: {reason}" })
    status = 1
  }

  if opts.since {
    if let seconds = boot.uptime {
      let t = civil_from_seconds(now - seconds + tz.offset)
      gnu.write_text(f"{t.year:04}-{two(t.month)}-{two(t.day)} {two(t.hour)}:{two(t.minute)}:{two(t.second)}\n")
    }
  } else if opts.pretty {
    if let seconds = boot.uptime {
      gnu.write_text(f"{pretty_text(seconds)}\n")
    }
  } else {
    let t = civil_from_seconds(now + tz.offset)
    let up = if let seconds = boot.uptime { up_text(seconds) } else { "up ???? days ??:??" }
    gnu.write_text(f" {t.hour:>2}:{two(t.minute)}:{two(t.second)} {up},  {plural(boot.users, "user", "users")}{load_text()}\n")
  }

  if let Err(failure) = io.flush_stdout() {
    gnu.write_failed(failure)
  }

  if status != 0 {
    exit status
  }
}
