#!/bin/xsh
use lib.procps

type Options = {all: Bool, full: Bool, pids: Str?, parents: Str?, output: List[Str], sort: Str?, no_headers: Bool, help: Bool, operands: List[Str]}
type Column = {field: Str, header: Str}

proc columns(specs: List[Str]) [error] -> Result[List[Column]] {
  var out: List[Column] = []
  for spec in specs {
    for token in spec.split(",") {
      let parts = token.split("=")
      let field = parts[0]
      return Err(error.failure(f"ps: unsupported output field '{field}'")) when field not in ["pid", "ppid", "uid", "user", "comm", "args", "cmd", "command", "stat", "etime", "etimes", "start", "lstart", "time", "c", "pcpu", "%cpu", "pmem", "%mem", "rss", "vsz", "tty", "state", "ni", "pri", "nlwp", "pgid", "sid"]
      let header = if parts.len() > 1 { parts[1] } else { match field { "pcpu" | "%cpu" => "%CPU"; "pmem" | "%mem" => "%MEM"; "comm" | "args" | "cmd" | "command" => "COMMAND"; "etime" => "ELAPSED"; "start" => "START"; else => field.upper() } }
      out = out.push({field: field, header: header})
    }
  }
  out
}

pure status(row: LinuxProcessSample) -> Str {
  let priority = if row.nice < 0 { "<" } else if row.nice > 0 { "N" } else { "" }
  let session_flag = if row.session == row.pid { "s" } else { "" }
  let threads = if row.thread_count > 1 { "l" } else { "" }
  f"{row.status}{priority}{session_flag}{threads}"
}

pure cell(row: LinuxProcessSample, field: Str, memory_total: Int) -> Str {
  match field {
    "pid" => f"{row.pid}"
    "ppid" => f"{row.parent_pid}"
    "uid" => f"{row.uid}"
    "user" => row.user
    "comm" => row.command
    "args" | "cmd" | "command" => row.argv
    "stat" => status(row)
    "time" => procps.clock(row.cpu_ticks / row.ticks_per_second)
    "c" | "pcpu" | "%cpu" => (100.0 * row.cpu_ticks.float() / row.ticks_per_second.float() / (if row.runtime_seconds <= 0 { 1.0 } else { row.runtime_seconds.float() })).format(1)
    "pmem" | "%mem" => (100.0 * row.rss_bytes.float() / memory_total.float()).format(1)
    "rss" => f"{row.rss_bytes / 1024}"
    "vsz" => f"{row.vsize_bytes / 1024}"
    "tty" => row.tty
    "state" => row.status
    "ni" => f"{row.nice}"
    "pri" => f"{row.priority}"
    "nlwp" => f"{row.thread_count}"
    "pgid" => f"{row.pgrp}"
    "sid" => f"{row.session}"
    "etimes" => f"{row.runtime_seconds}"
    "etime" => procps.clock(row.runtime_seconds)
    else => row.start_time
  }
}

proc main(...argv: List[Str]) [fs, process, error] {
  var normalized: List[Str] = []
  for word in argv { normalized = normalized.push(if word == "aux" { "--bsd-aux" } else { word }) }
  let bsd = "--bsd-aux" in normalized
  normalized = normalized |> where . != "--bsd-aux"
  let opts: Options = cli.applet(normalized, {gnu: {status: 2}, all: {form: "-e -A --everyone", default: false}, full: {form: "-f --full", default: false}, pids: {form: "-p --pid PID"}, parents: {form: "--ppid PID"}, output: {form: "-o --format FORMAT", repeated: true}, sort: {form: "--sort KEYS"}, no_headers: {form: "--no-headers --no-heading", default: false}, help: {form: "--help", default: false}, operands: {form: "...ARG"}})?
  if opts.help { print "Usage: ps [-e] [-f] [-p PID,...] [--ppid PID,...] [-o FIELD,...] [--sort FIELD]"; return }
  if ! opts.operands.is_empty() { eprint "ps: unexpected operand"; exit 2 }
  let snapshot = linux.sample()?
  var rows: List[LinuxProcessSample] = snapshot.processes
  let pids = if opts.pids == null { [] } else { procps.ids(opts.pids)? }
  let parents = if opts.parents == null { [] } else { procps.ids(opts.parents)? }
  if ! pids.is_empty() or ! parents.is_empty() { rows = rows |> where .pid in pids or .parent_pid in parents } else if ! opts.all and ! bsd {
    let own_pid = process.current_pid()?
    let own = snapshot.processes |> where .pid == own_pid
    if own.is_empty() { return Err(error.failure("ps: selecting process disappeared")) }
    rows = rows |> where .uid == applet.current_euid() and .tty_number == own[0].tty_number and .tty_number != 0
  }
  let specs = if opts.output.is_empty() { if bsd { ["user,pid,pcpu,pmem,vsz,rss,tty,stat,start,time,args"] } else if opts.full { ["user,pid,ppid,c,start,tty,time,args"] } else { ["pid,tty,time,comm"] } } else { opts.output }
  let fields = columns(specs)?
  var groups: List[List[LinuxProcessSample]] = [rows]
  for sorting in (opts.sort ?? "pid").split(",") {
    if ! rx"^[+-]?(pid|ppid|uid|user|comm|start|rss|vsz|pcpu|%cpu)$".matches(sorting) { eprint f"ps: unsupported sort key '{sorting}'"; exit 2 }
    let reverse = sorting.starts_with("-")
    let key = sorting.replace("-", with: "").replace("+", with: "")
    var next_groups: List[List[LinuxProcessSample]] = []
    for tied_rows in groups {
      var ordered = tied_rows
    match key {
    "pid" => { ordered = ordered |> sort-by(desc: reverse) .pid }
    "ppid" => { ordered = ordered |> sort-by(desc: reverse) .parent_pid }
    "uid" => { ordered = ordered |> sort-by(desc: reverse) .uid }
    "user" => { ordered = ordered |> sort-by(desc: reverse) .user }
    "comm" => { ordered = ordered |> sort-by(desc: reverse) .command }
    "start" => { ordered = ordered |> sort-by(desc: reverse) .start_time_ms }
    "rss" => { ordered = ordered |> sort-by(desc: reverse) .rss_bytes }
    "vsz" => { ordered = ordered |> sort-by(desc: reverse) .vsize_bytes }
    "pcpu" | "%cpu" => { ordered = ordered |> sort-by(desc: reverse) (if .runtime_seconds <= 0 { 0 } else { .cpu_ticks * 1000 / .ticks_per_second / .runtime_seconds }) }
    else => { eprint f"ps: unsupported sort key '{sorting}'"; exit 2 }
  }
      var segment: List[LinuxProcessSample] = []
      var previous_value: Str? = null
      for row in ordered {
        let value = match key {
          "start" => f"{row.start_time_ms}"
          "rss" => f"{row.rss_bytes}"
          "vsz" => f"{row.vsize_bytes}"
          "pcpu" | "%cpu" => f"{if row.runtime_seconds <= 0 { 0 } else { row.cpu_ticks * 1000 / row.ticks_per_second / row.runtime_seconds }}"
          else => cell(row, key, snapshot.memory.total)
        }
        if previous_value != null and previous_value != value { next_groups = next_groups.push(segment); segment = [] }
        segment = segment.push(row)
        previous_value = value
      }
      if ! segment.is_empty() { next_groups = next_groups.push(segment) }
    }
    groups = next_groups
  }
  rows = []
  for tied_rows in groups { rows = rows + tied_rows }
  if ! opts.no_headers and ! (fields |> where .header != "").is_empty() { let header = [field.header for field in fields].join(" "); print $header }
  for row in rows { let line = [cell(row, field.field, snapshot.memory.total) for field in fields].join(" "); print $line }
  if rows.is_empty() { exit 1 }
}
