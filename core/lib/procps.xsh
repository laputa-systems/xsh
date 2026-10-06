##! Process selection uses one snapshot and excludes the selecting process.
type GrepOptions = {full: Bool, exact: Bool, ignore_case: Bool, inverse: Bool, newest: Bool, oldest: Bool, count: Bool, list_name: Bool, list_full: Bool, delimiter: Str, users: Str?, parents: Str?, signal: Str, groups: Str?, sessions: Str?, help: Bool, operands: List[Str]}

## Constrains process fields before optionally choosing an age extreme.
export type Selector = {pattern: Str?, exact: Bool, full: Bool, ignore_case: Bool, inverse: Bool, pids: List[Int], parents: List[Int], users: List[Str], names: List[Str], groups: List[Int], sessions: List[Int], newest: Bool, oldest: Bool}

## Builds an unconstrained selector.
export pure selector() -> Selector {
  {pattern: null, exact: false, full: false, ignore_case: false, inverse: false, pids: [], parents: [], users: [], names: [], groups: [], sessions: [], newest: false, oldest: false}
}

## Parses positive decimal process identifiers without accepting empty fields.
export proc ids(text: Str) [error] -> Result[List[Int], Error] {
  var out: List[Int] = []
  for word in text.replace(" ", with: ",").split(",") {
    return Err(error.failure(f"invalid process id: {word}")) when ! rx"^[0-9]+$".matches(word)
    let value = word.parse_int()?
    return Err(error.failure(f"invalid process id: {word}")) when value <= 0
    out = out.push(value)
  }
  out
}

type AgeKey = {start_time_ms: Int, tie_pid: Int}
pure age_key(row: ProcessEntry) -> AgeKey { {start_time_ms: row.start_time_ms, tie_pid: row.pid} }

## Filters a snapshot; name patterns never match unrelated argv fields unless full is selected.
export proc select(rows: List[ProcessEntry], options: Selector, self_pid: Int) [error] -> Result[List[ProcessEntry], Error] {
  return Err(error.failure("newest and oldest are mutually exclusive")) when options.newest and options.oldest
  let source = options.pattern ?? ""
  let bounded = if options.exact { f"^(?:{source})$" } else { source }
  let pattern = regex.compile(if options.ignore_case { f"(?i){bounded}" } else { bounded })?
  var out: List[ProcessEntry] = []
  for row in rows {
    continue when row.pid == self_pid
    var matches = options.pids.is_empty() or row.pid in options.pids
    matches = matches and (options.parents.is_empty() or row.parent_pid in options.parents)
    matches = matches and (options.users.is_empty() or row.user in options.users or f"{row.uid}" in options.users)
    matches = matches and (options.names.is_empty() or (if options.ignore_case { row.command.lower() in [name.lower() for name in options.names] } else { row.command in options.names }))
    if ! options.groups.is_empty() {
      guard let group_id = row.pgrp else { return Err(error.failure("process group selection is unavailable on this host")) }
      matches = matches and group_id in options.groups
    }
    if ! options.sessions.is_empty() {
      guard let session_id = row.session else { return Err(error.failure("session selection is unavailable on this host")) }
      matches = matches and session_id in options.sessions
    }
    matches = matches and (options.pattern == null or pattern.matches(if options.full { row.argv } else { row.command }))
    if matches != options.inverse { out = out.push(row) }
  }
  if options.newest { return out |> sort-by(desc: true) age_key(.) |> take(1) }
  if options.oldest { return out |> sort-by age_key(.) |> take(1) }
  out |> sort-by .pid
}

## Renders an elapsed counter in hours, minutes, and seconds.
export pure clock(seconds: Int) -> Str {
  f"{seconds / 3600:02}:{seconds / 60 % 60:02}:{seconds % 60:02}"
}

## Parses the shared pgrep/pkill selector interface and renders or signals matches.
export proc grep_main(argv: List[Str], send_signal: Bool) [process, env, error, io] {
  var normalized: List[Str] = []
  var option_words = true
  for word in argv {
    if word == "--" { option_words = false }
    let signal_name = word.split("") |> drop(1).join("")
    if send_signal and option_words and word.starts_with("-") and ! word.starts_with("--") and word.byte_len() > 1 and process.signal(signal_name) is Ok(_) { normalized = normalized.push("--signal").push(signal_name) } else { normalized = normalized.push(word) }
  }
  let opts: GrepOptions = cli.applet(normalized, {
    gnu: {status: 2},
    full: {form: "-f --full", default: false},
    exact: {form: "-x --exact", default: false},
    ignore_case: {form: "-i --ignore-case", default: false},
    inverse: {form: "-v --inverse", default: false},
    newest: {form: "-n --newest", default: false},
    oldest: {form: "-o --oldest", default: false},
    count: {form: "-c --count", default: false},
    list_name: {form: "-l --list-name", default: false},
    list_full: {form: "-a --list-full", default: false},
    delimiter: {form: "-d --delimiter TEXT", default: "\n"},
    users: {form: "-u --euid USER"},
    parents: {form: "-P --parent PID"},
    signal: {form: "--signal SIGNAL", default: "TERM"},
    groups: {form: "-g --pgroup GROUP"},
    sessions: {form: "-s --session SESSION"},
    help: {form: "--help", default: false},
    operands: {form: "...PATTERN"},
  })?
  if ! send_signal and ! (argv |> where . == "--signal" or .starts_with("--signal=")).is_empty() { eprint "pgrep: signal delivery requires pkill"; exit 2 }
  if send_signal and (opts.list_name or opts.list_full or opts.delimiter != "\n") { eprint "pkill: output formatting options are unsupported"; exit 2 }
  if opts.help { print "Usage: pgrep/pkill [-fixvno] [-u USER,...] [-P PPID,...] PATTERN"; return }
  if opts.operands.len() > 1 or (opts.operands.is_empty() and opts.users == null and opts.parents == null and opts.groups == null and opts.sessions == null) {
    eprint "a single pattern or an explicit process selector is required"; exit 2
  }
  let base = selector()
  let options: Selector = {...base, pattern: if opts.operands.is_empty() { null } else { opts.operands[0] }, exact: opts.exact, full: opts.full, ignore_case: opts.ignore_case, inverse: opts.inverse, newest: opts.newest, oldest: opts.oldest, users: (opts.users ?? "").split(",") |> where . != "", parents: if opts.parents == null { [] } else { ids(opts.parents)? }, groups: if opts.groups == null { [] } else { ids(opts.groups)? }, sessions: if opts.sessions == null { [] } else { ids(opts.sessions)? }}
  let rows: List[ProcessEntry] = process.list()? |> collect
  let matching = select(rows, options, process.current_pid()?)
  if let Err(problem) = matching { eprint f"pgrep/pkill: {problem.message}"; exit 2 }
  let selected = matching?
  var delivered = 0
  var output: List[Str] = []
  if send_signal {
    let requested = process.signal(opts.signal)
    if let Err(problem) = requested { eprint f"pkill: {problem.message}"; exit 2 }
    let signal = requested?
    for row in selected {
      if let Err(failure) = process.kill(row.pid, signal.name) {
        eprint f"pkill: {row.pid}: {failure.message}"
      } else { delivered = delivered + 1 }
    }
  } else {
    for row in selected {
      output = output.push(if opts.list_full { f"{row.pid} {row.argv}" } else if opts.list_name { f"{row.pid} {row.command}" } else { f"{row.pid}" })
    }
  }
  if opts.count { print selected.len() } else if ! send_signal and ! output.is_empty() { print output.join(opts.delimiter) }
  if selected.is_empty() or (send_signal and delivered == 0) { exit 1 }
}

## Sampling arguments use seconds and a finite count when supplied.
export type SamplingOptions = {interval: Float, count: Int}

## Parses the positional interval/count interface shared by sampling tools.
export proc sampling_args(operands: List[Str]) [error] -> Result[SamplingOptions, Error] {
  return Err(error.failure("expected [INTERVAL [COUNT]]")) when operands.len() > 2
  let interval = if operands.is_empty() { 1.0 } else { operands[0].parse_float()? }
  let count = if operands.len() > 1 { operands[1].parse_int()? } else if operands.is_empty() { 1 } else { 9223372036854775807 }
  return Err(error.failure("interval and count must be positive")) when interval <= 0.0 or count <= 0
  {interval: interval, count: count}
}

## Rejects reset counters instead of reporting a negative rate.
export proc delta(current: Int, previous: Int) [error] -> Result[Int, Error] {
  return Err(error.failure("kernel counter decreased between samples")) when current < previous
  current - previous
}

## CPU percentages count each kernel tick once; guest ticks are already included in user and nice.
export type CpuPercent = {user: Float, nice: Float, system: Float, irq: Float, softirq: Float, idle: Float, wait: Float, steal: Float}

## Computes aggregate percentages since boot or between two observations.
## The kernel can correct iowait downward, so a correction contributes zero interval ticks.
export proc cpu_percent(current: LinuxCpuSample, previous: LinuxCpuSample?) [error] -> Result[CpuPercent, Error] {
  let before: LinuxCpuSample = previous ?? {user: 0, nice: 0, system: 0, idle: 0, iowait: 0, irq: 0, softirq: 0, steal: 0}
  let user_ticks = delta(current.user, before.user)?
  let nice_ticks = delta(current.nice, before.nice)?
  let sys = delta(current.system, before.system)?
  let hard_irq = delta(current.irq, before.irq)?
  let soft_irq = delta(current.softirq, before.softirq)?
  let idle = delta(current.idle, before.idle)?
  let waiting = if current.iowait < before.iowait { 0 } else { delta(current.iowait, before.iowait)? }
  let steal = delta(current.steal, before.steal)?
  let total = user_ticks + nice_ticks + sys + hard_irq + soft_irq + idle + waiting + steal
  let scale = if total == 0 { 0.0 } else { 100.0 / total.float() }
  {user: user_ticks.float() * scale, nice: nice_ticks.float() * scale, system: sys.float() * scale, irq: hard_irq.float() * scale, softirq: soft_irq.float() * scale, idle: idle.float() * scale, wait: waiting.float() * scale, steal: steal.float() * scale}
}
