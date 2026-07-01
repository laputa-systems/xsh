#!/usr/bin/env -S xsh --
# Process Explorer
# Inspect matching processes with lineage, memory, threads, and listening ports.
# Usage: xsh showcase/px.xsh -- [-f] [-t] [--kill[=SIGNAL]] [-p PORT] [PATTERN...]
# Example: xsh showcase/px.xsh -- -f postgres
error PxError = Usage(message: Str) : Usage

type Process = {
  pid: Int,
  parent_pid: Int,
  command: Str,
  argv: Str,
  argv0: Str,
  user: Str,
  uid: Int,
  status: Str,
  start_time: Str,
  start_time_ms: Int,
  runtime_seconds: Int,
}

type Thread = {
  pid: Int,
  parent_pid: Int,
  command: Str,
  argv: Str,
  argv0: Str,
  user: Str,
  uid: Int,
  status: Str,
  start_time: Str,
  start_time_ms: Int,
  runtime_seconds: Int,
  owner_pid: Int,
  thread_id: Int,
  thread_name: Str,
}

type PortProcess = {
  pid: Int,
  parent_pid: Int,
  command: Str,
  argv: Str,
  argv0: Str,
  user: Str,
  uid: Int,
  protocol: Str,
  local_address: Str,
  local_port: Int,
  local: Str,
  remote_address: Str,
  remote_port: Int,
  remote: Str,
  state: Str,
  fd: Int,
  inode: Int,
}

type Row = {
  pid: Int,
  parent_pid: Int,
  command: Str,
  argv: Str,
  argv0: Str,
  user: Str,
  uid: Int,
  status: Str,
  start_time: Str,
  start_time_ms: Int,
  runtime_seconds: Int,
  owner_pid: Int,
  thread_id: Int,
  thread_name: Str,
}

type ProcessStats = {cpu: Str, vsz_kb: Int, rss_kb: Int, cputime: Str}

type StatsRow = {pid: Int, stats: ProcessStats}

type ThreadRows = {pid: Int, rows: List[Row]}

type Options = {full: Bool, show_threads: Bool, kill: Int?, port: Int, patterns: List[Str]}

type Query = {text: Str, numeric: Int, is_numeric: Bool}

pure is_decimal(text: Str) -> Bool {
  if text == "" {
    return false
  }

  for ch in text.split("") {
    if ! "0123456789".contains(ch) {
      return false
    }
  }

  return true
}

pure empty_stats() -> ProcessStats {
  return {cpu: "-", vsz_kb: -1, rss_kb: -1, cputime: "    0:00"}
}

pure query(pattern: Str) -> Query {
  if is_decimal(pattern) {
    return {text: pattern, numeric: pattern.parse_int() ?? -1, is_numeric: true}
  }

  return {text: pattern, numeric: -1, is_numeric: false}
}

pure queries(patterns: List[Str]) -> List[Query] {
  [query(pattern) for pattern in patterns]
}

pure normalize_kill_args(argv: List[Str]) -> List[Str] {
  var normalized: List[Str] = []
  var index = 0

  while index < argv.len() {
    let arg = argv[index]

    if arg == "--kill" {
      if index + 1 < argv.len() and is_decimal(argv[index + 1]) {
        normalized = normalized.push(arg)
      } else {
        normalized = normalized.push("--kill=15")
      }
    } else {
      normalized = normalized.push(arg)
    }

    index += 1
  }

  return normalized
}

pure unique_ints(items: List[Int]) -> List[Int] {
  var unique: List[Int] = []

  for item in items {
    if ! unique.contains(item) {
      unique = unique.push(item)
    }
  }

  return unique
}

proc macos_stats() [process, error] -> Result[Map[ProcessStats]] {
  var stats: Map[ProcessStats] = {}
  let text = run.text ps -axo pid=,%cpu=,vsz=,rss=,time= ?

  for line in text.lines() {
    let fields = line.fields()
    continue when fields.len() < 5
    let pid = fields[0].parse_int() ?? -1
    continue when pid <= 0

    stats[f"${pid}"] = {
      cpu: fields[1],
      vsz_kb: fields[2].parse_int() ?? -1,
      rss_kb: fields[3].parse_int() ?? -1,
      cputime: fields[4],
    }
  }

  return stats
}

proc native_stats_for(pid: Int) [process, error] -> Result[ProcessStats] {
  let stats = process.stats(pid)?

  if stats.rss_kb >= 0 and stats.vsz_kb >= 0 {
    return {cpu: "-", vsz_kb: stats.vsz_kb, rss_kb: stats.rss_kb, cputime: "    0:00"}
  }

  return empty_stats()
}

proc stats_map_for(pids: List[Int], os_name: Str) [process, error] -> Result[Map[ProcessStats]] {
  var stats_by_pid: Map[ProcessStats] = {}

  if pids.len() == 0 {
    return stats_by_pid
  }

  var rows: List[StatsRow] = []

  if pids.len() == 1 {
    let pid = pids[0]
    rows = [{pid: pid, stats: native_stats_for(pid)?}]
  } else {
    rows = pids
      |> par-map { |pid|
        {pid: pid, stats: native_stats_for(pid)?}
      }
  }

  var needs_fallback = false

  for row in rows {
    stats_by_pid[f"${row.pid}"] = row.stats

    if row.stats.rss_kb < 0 and os_name == "Darwin" {
      needs_fallback = true
    }
  }

  if needs_fallback {
    let fallback = macos_stats()?

    for row in rows {
      if row.stats.rss_kb < 0 {
        stats_by_pid[f"${row.pid}"] = fallback.get(f"${row.pid}", empty_stats())
      }
    }
  }

  return stats_by_pid
}

pure process_row(row: Process) -> Row {
  return {
    pid: row.pid,
    parent_pid: row.parent_pid,
    command: row.command,
    argv: row.argv,
    argv0: row.argv0,
    user: row.user,
    uid: row.uid,
    status: row.status,
    start_time: row.start_time,
    start_time_ms: row.start_time_ms,
    runtime_seconds: row.runtime_seconds,
    owner_pid: row.pid,
    thread_id: row.pid,
    thread_name: "",
  }
}

pure thread_row(row: Thread) -> Row {
  return {
    pid: row.pid,
    parent_pid: row.parent_pid,
    command: row.command,
    argv: row.argv,
    argv0: row.argv0,
    user: row.user,
    uid: row.uid,
    status: row.status,
    start_time: row.start_time,
    start_time_ms: row.start_time_ms,
    runtime_seconds: row.runtime_seconds,
    owner_pid: row.owner_pid,
    thread_id: row.thread_id,
    thread_name: row.thread_name,
  }
}

pure owner_row(row: Row) -> Row {
  return {
    pid: row.owner_pid,
    parent_pid: row.parent_pid,
    command: row.command,
    argv: row.argv,
    argv0: row.argv0,
    user: row.user,
    uid: row.uid,
    status: row.status,
    start_time: row.start_time,
    start_time_ms: row.start_time_ms,
    runtime_seconds: row.runtime_seconds,
    owner_pid: row.owner_pid,
    thread_id: row.owner_pid,
    thread_name: "",
  }
}

pure command_text(row: Row) -> Str {
  if row.argv != "" {
    return row.argv
  }

  return row.command
}

pure display_command(row: Row) -> Str {
  let command = command_text(row)

  if row.thread_name != "" and row.thread_name != row.command {
    return f"${command} [${row.thread_name}]"
  }

  return command
}

pure paint(text: Str, color: Str) -> Str {
  return f"${color}${text}${tui.reset()}"
}

pure process_matches_pattern(row: Process, pattern: Query, full: Bool, own_pid: Int) -> Bool {
  if pattern.is_numeric {
    return row.pid == pattern.numeric
  }

  if own_pid > 0 and row.pid == own_pid {
    return false
  }

  let command = command_text(process_row(row))

  if full {
    return command.contains(pattern.text)
  }

  return row.command.contains(pattern.text) or row.argv0.contains(pattern.text)
}

pure process_matches_any(row: Process, patterns: List[Query], full: Bool, own_pid: Int) -> Bool {
  if patterns.len() == 0 {
    return true
  }

  for pattern in patterns {
    if process_matches_pattern(row, pattern, full, own_pid) {
      return true
    }
  }

  return false
}

pure process_has_port(pid: Int, ports: List[PortProcess], port: Int) -> Bool {
  if port <= 0 {
    return true
  }

  for row in ports {
    if row.pid == pid and row.local_port == port {
      return true
    }
  }

  return false
}

pure thread_matches_pattern(row: Thread, pattern: Query, full: Bool, own_pid: Int) -> Bool {
  if pattern.is_numeric {
    return row.pid == pattern.numeric or row.owner_pid == pattern.numeric or row.thread_id == pattern.numeric
  }

  if own_pid > 0 and row.owner_pid == own_pid {
    return false
  }

  let command = command_text(thread_row(row))

  if full {
    return command.contains(pattern.text) or row.thread_name.contains(pattern.text)
  }

  return row.command.contains(pattern.text) or row.argv0.contains(pattern.text) or row.thread_name.contains(
    pattern.text,
  )
}

pure thread_matches_any(row: Thread, patterns: List[Query], full: Bool, own_pid: Int) -> Bool {
  if patterns.len() == 0 {
    return true
  }

  for pattern in patterns {
    if thread_matches_pattern(row, pattern, full, own_pid) {
      return true
    }
  }

  return false
}

pure label(name: Str) -> Str {
  return paint(tui.right_pad(name, 6), tui.gray())
}

pure port_label(row: PortProcess) -> Str {
  if row.state == "LISTEN" {
    return f"${row.protocol}:${row.local}"
  }

  return f"${row.protocol}:${row.local}"
}

pure port_summary(ports: List[PortProcess]) -> Str {
  if ports.len() == 0 {
    return ""
  }

  let labels = ports
    |> group-by f"${.protocol}:${.local}"
    |> sort-by .key
    |> map { |bucket|
      port_label(bucket.items[0])
    }

  return labels.join(", ")
}

proc print_row(row: Row, stats: ProcessStats, ports: List[PortProcess]) [time, error] {
  let elapsed = time.duration_compact(row.runtime_seconds)
  let rss = paint(bytes.human(stats.rss_kb * 1024), tui.yellow())
  let vsz = paint(bytes.human(stats.vsz_kb * 1024), tui.magenta())
  let port_text = port_summary(ports)
  print f"  ${label("pid")}${paint(f"${row.pid}", tui.cyan())}"
  print f"  ${label("user")}${paint(row.user, tui.blue())}"
  print f"  ${label("alive")}${paint(elapsed, tui.gray())}"
  print f"  ${label("mem")}rss ${rss}  vsz ${vsz}"

  if port_text != "" {
    print f"  ${label("ports")}${paint(port_text, tui.green())}"
  }
}

pure thread_label(name: Str, count: Int) -> Str {
  let display = if name == "" { "(unnamed)" } else { name }

  if count > 1 {
    return f"${display} x${count}"
  }

  return display
}

pure connector(last: Bool) -> Str {
  return if last { "\u{2514} " } else { "\u{251c} " }
}

pure lineage_text(row: Row) -> Str {
  return f"${display_command(row)} (${row.pid})"
}

pure lineage_indent(depth: Int) -> Str {
  var text = ""
  var index = 0

  while index < depth {
    text = f"${text}  "
    index += 1
  }

  return text
}

pure lineage_marker(depth: Int) -> Str {
  if depth == 0 {
    return ""
  }

  return f"${lineage_indent(depth - 1)}${connector(true)}"
}

pure parent_lineage(row: Row, rows_by_pid: Map[Row]) -> List[Row] {
  var rows: List[Row] = []
  var parent_pid = row.parent_pid
  var depth = 0

  while parent_pid > 0 and depth < 128 {
    match rows_by_pid.get(f"${parent_pid}") {
      Ok(parent) => {
        rows = rows.push(parent)
        parent_pid = parent.parent_pid
      }
      Err(_) => return rows
    }

    depth += 1
  }

  return rows
}

proc print_lineage(row: Row, rows_by_pid: Map[Row]) [error] {
  let parents = parent_lineage(row, rows_by_pid)
  var index = parents.len() - 1
  var depth = 0

  while index >= 0 {
    let parent = parents[index]
    let name = if depth == 0 { "tree" } else { "" }
    print f"  ${label(name)}${paint(lineage_marker(depth), tui.gray())}${paint(lineage_text(parent), tui.dim())}"
    index -= 1
    depth += 1
  }

  let name = if depth == 0 { "tree" } else { "" }
  let state_suffix = if row.status == "Z" or row.status == "zombie" { " <defunct>" } else { "" }
  print f"  ${label(name)}${paint(lineage_marker(depth), tui.gray())}${paint(display_command(row), tui.bold())}${state_suffix}"
}

proc print_thread_names(threads: List[Row], depth: Int) [error] {
  let names = threads
    |> group-by .thread_name
    |> sort-by .key

  let count = names.len()

  for item in names |> enumerate() {
    let bucket = item.value
    let last = item.index + 1 == count
    let name = if item.index == 0 { "thread" } else { "" }

    print f"  ${label(name)}${paint(lineage_indent(depth), tui.gray())}${paint(connector(last), tui.gray())}${paint(
      thread_label(bucket.key, bucket.items.len()),
      tui.dim(),
    )}"
  }
}

proc signal_matched_pids(pids: List[Int], signal: Int, own_pid: Int) [process, error] -> Result[Int] {
  let info = process.signal(f"${signal}")?
  var signaled = 0

  for pid in unique_ints(pids) {
    continue when pid == own_pid
    process.kill(pid, signal: f"${info.number}")?
    signaled += 1
  }

  return signaled
}

proc ports_for_pids(pids: List[Int]) [process, error] -> Result[List[PortProcess]] {
  var ports: List[PortProcess] = []

  for pid in unique_ints(pids) {
    for row in process.ports(pid)? {
      ports = ports.push(row)
    }
  }

  return ports |> sort-by .pid * 1000 + .fd
}

proc main(...argv: List[Str]) [fs, process, env, time, error] {
  let opts: Options = cli.parse(
    normalize_kill_args(argv),
    {
      full: {form: "-f", default: false},
      show_threads: {form: "-t", default: false},
      kill: {form: "--kill[=SIGNAL]", kind: "UInt", optional_default: 15, max: 128},
      port: {form: "-p PORT", kind: "UInt", default: 0},
      patterns: {form: "...PATTERN", repeated: true},
    },
  )?

  let kill_signal = opts.kill

  if kill_signal != null and opts.patterns.len() == 0 and opts.port <= 0 {
    return Err(PxError.Usage(message: "--kill requires at least one PATTERN or -p PORT"))
  }

  let own_pid = process.current_pid()?
  let query_items = queries(opts.patterns)
  let os = system.uname()?
  var matching_ports: List[PortProcess] = []

  if opts.port > 0 {
    matching_ports = process.port(opts.port)? |> sort-by .pid * 1000 + .fd
  }

  let empty_ports: List[PortProcess] = []
  let empty_rows: List[Row] = []
  var rows_by_pid: Map[Row] = {}
  var matched_rows: List[Row] = []
  var matched_threads_by_pid: Map[List[Row]] = {}
  var matched_owner_pids: List[Int] = []

  for proc_row in process.list()? {
    let row = process_row(proc_row)
    rows_by_pid[f"${row.pid}"] = row
    continue when ! process_has_port(row.pid, matching_ports, opts.port)

    if ! opts.show_threads and process_matches_any(proc_row, query_items, opts.full, own_pid) {
      matched_rows = matched_rows.push(row)
    } else if opts.show_threads and process_matches_any(proc_row, query_items, opts.full, own_pid) {
      matched_owner_pids = matched_owner_pids.push(row.pid)
    }
  }

  if opts.show_threads {
    if matched_owner_pids.len() > 0 {
      var thread_groups: List[ThreadRows] = []

      if matched_owner_pids.len() == 1 {
        let owner_pid = matched_owner_pids[0]
        thread_groups = [{pid: owner_pid, rows: [thread_row(thread) for thread in process.threads(owner_pid)?]}]
      } else {
        thread_groups = matched_owner_pids
          |> par-map { |owner_pid|
            {pid: owner_pid, rows: [thread_row(thread) for thread in process.threads(owner_pid)?]}
          }
      }

      for thread_group in thread_groups {
        matched_threads_by_pid[f"${thread_group.pid}"] = thread_group.rows
      }
    } else {
      for thread in process.threads()? {
        continue when ! thread_matches_any(thread, query_items, opts.full, own_pid)
        continue when ! process_has_port(thread.owner_pid, matching_ports, opts.port)
        let row = thread_row(thread)
        let key = f"${row.owner_pid}"

        if ! matched_threads_by_pid.has(key) {
          matched_owner_pids = matched_owner_pids.push(row.owner_pid)
        }

        matched_threads_by_pid = matched_threads_by_pid.push(key, row)
      }
    }
  }

  if opts.show_threads and matched_owner_pids.len() == 0 {
    abort(1)
  }

  if ! opts.show_threads and matched_rows.len() == 0 {
    abort(1)
  }

  if kill_signal != null {
    let kill_pids = if opts.show_threads { matched_owner_pids } else { [row.owner_pid for row in matched_rows] }
    let signaled = signal_matched_pids(kill_pids, kill_signal, own_pid)?
    print f"signaled ${signaled} process(es) with signal ${kill_signal}"
    return
  }

  let stats_pids = if opts.show_threads { matched_owner_pids } else { [row.owner_pid for row in matched_rows] }

  let display_ports = if opts.port > 0 {
    matching_ports
  } else if stats_pids.len() <= 32 {
    ports_for_pids(stats_pids)?
  } else {
    process.ports()? |> sort-by .pid * 1000 + .fd
  }

  var ports_by_pid: Map[List[PortProcess]] = {}

  for bucket in display_ports |> group-by .pid {
    ports_by_pid[f"${bucket.key}"] = bucket.items
  }

  let stats_by_pid = stats_map_for(stats_pids, os.sysname)?
  var printed = 0

  if opts.show_threads {
    for owner_pid in matched_owner_pids {
      let items = matched_threads_by_pid.get(f"${owner_pid}", empty_rows)

      if printed > 0 {
        print ""
      }

      printed += 1
      let row = owner_row(items[0])
      let stat_pid = row.owner_pid
      let stats = stats_by_pid.get(f"${stat_pid}", empty_stats())
      let tree_depth = parent_lineage(row, rows_by_pid).len()
      print_lineage(row, rows_by_pid)?
      print_row(row, stats, ports_by_pid.get(f"${stat_pid}", empty_ports))?
      print_thread_names(items, tree_depth + 1)?
    }
  } else {
    for row in matched_rows {
      if printed > 0 {
        print ""
      }

      printed += 1
      let stat_pid = row.owner_pid
      let stats = stats_by_pid.get(f"${stat_pid}", empty_stats())
      print_lineage(row, rows_by_pid)?
      print_row(row, stats, ports_by_pid.get(f"${stat_pid}", empty_ports))?
    }
  }
}
