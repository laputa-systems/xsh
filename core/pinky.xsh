#!/bin/xsh
use lib.auth
use lib.gnu

const USAGE = """Usage: pinky [OPTION]... [USER]...

  -l              produce long format output for the specified USERs
  -b              omit the user's home directory and shell in long format
  -h              omit the user's project file in long format
  -p              omit the user's plan file in long format
  -s              do short format output, this is the default
  -f              omit the line of column headings in short format
  -w              omit the user's full name in short format
  -i              omit the user's full name and remote host in short format
  -q              omit the user's full name, remote host and idle time
                  in short format
      --help        display this help and exit
      --version     output version information and exit

A lightweight 'finger' program;  print user information.
The utmp file will be /var/run/utmp.
"""

type PinkyOptions = {
  long: Bool,
  omit_home: Bool,
  omit_project: Bool,
  omit_plan: Bool,
  short: Bool,
  omit_heading: Bool,
  omit_name: Bool,
  omit_name_host: Bool,
  omit_name_host_idle: Bool,
  lookup: Bool,
  help: Bool,
  version: Bool,
  users: List[Str],
}

# Which columns the short format shows.
type Columns = {name: Bool, idle: Bool, where: Bool, hard_time: Bool, lookup: Bool}

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

# The idle column: blank under a minute, HH:MM under a day, then whole days.
pure idle_text(now: Int, last: Int) -> Str {
  let seconds = if now > last { now - last } else { 0 }

  return "     " when seconds < 60
  return f"{two(seconds / 3600)}:{two(seconds % 3600 / 60)}" when seconds < 86400

  f"{seconds / 86400}d"
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

pure capitalize(text: Str) -> Str {
  return text when text == ""

  let first = text.byte_slice(0, length: 1)
  let upper = if first >= "a" and first <= "z" { first.upper() } else { first }
  f"{upper}{text.byte_slice(1)}"
}

# The GECOS name up to its first comma, with each `&` replaced by the capitalized
# login name.
pure full_name(gecos: Str, login: Str) -> Str {
  let comma = gecos.find(",")
  let field = if comma == null { gecos } else { gecos.byte_slice(0, length: comma) }

  field.replace("&", with: capitalize(login))
}

proc account_named(accounts: List[auth.PasswdEntry], name: Str) -> auth.PasswdEntry? {
  for entry in accounts {
    return entry when entry.name == name
  }

  null
}

proc read_accounts() [fs, env, error] -> List[auth.PasswdEntry] {
  if let Ok(entries) = auth.read_passwd_entries() {
    entries
  } else {
    let none: List[auth.PasswdEntry] = []
    none
  }
}

# `XSH_UTMP_FILE` replaces /var/run/utmp the way `XSH_PASSWD_FILE` replaces
# /etc/passwd, so tests can supply sessions; GNU pinky has no FILE operand.
proc utmp_path() [env] -> Path {
  let named = env_text("XSH_UTMP_FILE") ?? ""

  if named == "" { /var/run/utmp } else { fp"{named}" }
}

# User sessions only. A missing or unreadable file lists nobody, as glibc does,
# and unlike who and users the sessions are not checked against live pids.
proc read_sessions() [fs, process, env, error] -> List[UnixUtmp] {
  let file = utmp_path()

  match unix.read_utmp(file) {
    Ok(records) => [entry for entry in records if entry.kind == "user_process" and entry.user != ""]
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

pure heading(columns: Columns) -> Str {
  var text = pad_right("Login", 8)

  if columns.name {
    text = f"{text} {pad_right("Name", 19)}"
  }

  text = f"{text} {pad_right(" TTY", 9)}"

  if columns.idle {
    text = f"{text} {pad_right("Idle", 6)}"
  }

  text = f"{text} {pad_right("When", if columns.hard_time { 16 } else { 12 })}"

  if columns.where {
    text = f"{text} Where"
  }

  text
}

proc entry_line(
  columns: Columns,
  accounts: List[auth.PasswdEntry],
  entry: UnixUtmp,
  tz: Tz,
  now: Int,
) [fs, process, env, error] -> Str {
  let device = if entry.line.starts_with("/") { fp"{entry.line}" } else { fp"/dev/{entry.line}" }
  var mesg = "?"
  var last = 0

  if let Ok(info) = fs.stat(device) {
    mesg = if info.mode.bit_and(0o020) != 0 { " " } else { "*" }
    last = info.atime_ns / 1000000000
  }

  var text = pad_right(entry.user, 8)

  if columns.name {
    let account = account_named(accounts, entry.user)

    if let found = account {
      let shown = full_name(found.gecos, found.name)
      text = f"{text} {pad_right(shown.byte_slice(0, length: 19), 19)}"
    } else {
      text = f"{text} {pad_left("        ???", 19)}"
    }
  }

  text = f"{text} {mesg}{pad_right(entry.line, 8)}"

  if columns.idle {
    text = f"{text} {pad_right(if last == 0 { "?????" } else { idle_text(now, last) }, 6)}"
  }

  text = f"{text} {time_string(entry.time_sec, tz, columns.hard_time)}"

  if columns.where and entry.host != "" {
    var host = entry.host
    var display: Str? = null
    let colon = host.find(":")

    if colon != null {
      display = host.byte_slice(colon + 1)
      host = host.byte_slice(0, length: colon)
    }

    if columns.lookup and host != "" {
      host = canonical_host(host)
    }

    text = if let shown = display { f"{text} {host}:{shown}" } else { f"{text} {host}" }
  }

  text
}

proc short_format(opts: PinkyOptions) [fs, process, env, time, error, io] {
  let columns: Columns = Columns(
    name: ! (opts.omit_name or opts.omit_name_host or opts.omit_name_host_idle),
    idle: ! opts.omit_name_host_idle,
    where: ! (opts.omit_name_host or opts.omit_name_host_idle),
    hard_time: hard_time_locale(),
    lookup: opts.lookup,
  )

  if ! opts.omit_heading {
    gnu.write_text(f"{heading(columns)}\n")
  }

  let accounts = if columns.name { read_accounts() } else { [] }
  let tz = load_tz()
  let now = time.now() / 1000

  for entry in read_sessions() {
    if opts.users.is_empty() or entry.user in opts.users {
      gnu.write_text(f"{entry_line(columns, accounts, entry, tz, now)}\n")
    }
  }
}

# A `.project` or `.plan` file is copied as it is; an absent one is skipped.
proc copy_file(label: Str, file: Path) [fs, process, env, error, io] {
  guard let data = file.read_bytes() else {
    return
  }

  gnu.write_text(label)
  gnu.write_bytes(data)
}

proc long_entry(opts: PinkyOptions, accounts: List[auth.PasswdEntry], login: Str) [fs, process, env, error, io] {
  gnu.write_text(f"Login name: {pad_right(login, 28)}In real life: ")

  guard let account = account_named(accounts, login) else {
    gnu.write_text(" ???\n")
    return
  }

  gnu.write_text(f" {full_name(account.gecos, account.name)}\n")

  if ! opts.omit_home {
    gnu.write_text(f"Directory: {pad_right(account.home.display(), 29)}Shell:  {account.shell}\n")
  }

  if ! opts.omit_project {
    copy_file("Project: ", fp"{account.home}/.project")
  }

  if ! opts.omit_plan {
    copy_file("Plan:\n", fp"{account.home}/.plan")
  }

  gnu.write_text("\n")
}

proc main(...argv: List[Str]) [fs, process, env, time, error, io] {
  let opts: PinkyOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      long: {form: "-l", default: false, conflicts: ["short"]},
      omit_home: {form: "-b", default: false},
      omit_project: {form: "-h", default: false},
      omit_plan: {form: "-p", default: false},
      short: {form: "-s", default: false, conflicts: ["long"]},
      omit_heading: {form: "-f", default: false},
      omit_name: {form: "-w", default: false},
      omit_name_host: {form: "-i", default: false},
      omit_name_host_idle: {form: "-q", default: false},
      lookup: {form: "--lookup", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      users: {form: "...USER"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("pinky")
    return
  }

  # `conflicts` makes the last of -l and -s decide, as in GNU.
  let long_format = opts.long

  if long_format and opts.users.is_empty() {
    gnu.usage_error("no username specified; at least one must be specified when using -l")
  }

  if long_format {
    let accounts = read_accounts()

    for login in opts.users {
      long_entry(opts, accounts, login)
    }
  } else {
    short_format(opts)
  }

  if let Err(failure) = io.flush_stdout() {
    gnu.write_failed(failure)
  }
}
