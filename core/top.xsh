#!/bin/xsh
use lib.procps

type RatedProcess = {row: LinuxProcessSample, percent: Float, basis_points: Int}

type Options = {batch: Bool, delay: Str, count: Str, pids: Str?, full: Bool, help: Bool, operands: List[Str]}
proc main(...argv: List[Str]) [fs, process, time, error] {
  let opts: Options = cli.applet(argv, {gnu: {status: 2}, batch: {form: "-b", default: false}, delay: {form: "-d SECONDS", default: "3"}, count: {form: "-n COUNT", default: "9223372036854775807"}, pids: {form: "-p PID"}, full: {form: "-c", default: false}, help: {form: "--help", default: false}, operands: {form: "...ARG"}})?
  if opts.help { print "Usage: top -b [-d SECONDS] [-n COUNT] [-p PID,...] [-c]"; return }
  if ! opts.batch { eprint "top: interactive input is unsupported; use -b"; exit 2 }
  if ! opts.operands.is_empty() { eprint "top: unexpected operand"; exit 2 }
  let sampling = procps.sampling_args([opts.delay, opts.count])?
  let pids = if opts.pids == null { [] } else { procps.ids(opts.pids)? }
  var previous: LinuxSample? = null
  var index = 0
  while index < sampling.count {
    let current = linux.sample()?
    let cpu_usage = procps.cpu_percent(current.cpu, if previous == null { null } else { previous.cpu })?
    print f"top - up {current.uptime_ms / 1000}s"
    print f"Tasks: {current.processes.len()} total, {current.running} running"
    print f"%Cpu(s): {cpu_usage.user.format(1)} us, {cpu_usage.nice.format(1)} ni, {cpu_usage.system.format(1)} sy, {cpu_usage.idle.format(1)} id, {cpu_usage.wait.format(1)} wa, {cpu_usage.irq.format(1)} hi, {cpu_usage.softirq.format(1)} si, {cpu_usage.steal.format(1)} st"
    print f"KiB Mem: {current.memory.total / 1024} total, {current.memory.free / 1024} free, {(current.memory.total - current.memory.available) / 1024} used, {(current.memory.buffers + current.memory.cached + current.memory.sreclaimable) / 1024} buff/cache"
    print "PID USER PR NI VIRT RES S %CPU %MEM TIME+ COMMAND"
    let elapsed_ms = if previous == null { current.uptime_ms } else { current.sampled_at_ms - previous.sampled_at_ms }
    if elapsed_ms <= 0 { return Err(error.failure("sampling clock did not advance")) }
    var ranked: List[RatedProcess] = []
    for row in current.processes {
      continue when ! pids.is_empty() and row.pid not in pids
      let older: List[LinuxProcessSample] = if previous == null { [] } else { previous.processes |> where .pid == row.pid and .start_ticks == row.start_ticks }
      let percent = procps.delta(row.cpu_ticks, if older.is_empty() { 0 } else { older[0].cpu_ticks })?.float() * 100000.0 / elapsed_ms.float() / current.ticks_per_second.float()
      ranked = ranked.push({row: row, percent: percent, basis_points: (percent * 100.0).round()?})
    }
    for item in ranked |> sort-by(desc: true) .basis_points {
      let row = item.row
      let percent = item.percent
      let memory = 100.0 * row.rss_bytes.float() / current.memory.total.float()
      print f"{row.pid} {row.user} {row.priority} {row.nice} {row.vsize_bytes / 1024} {row.rss_bytes / 1024} {row.status} {percent.format(1)} {memory.format(1)} {procps.clock(row.cpu_ticks / row.ticks_per_second)} {if opts.full { row.argv } else { row.command }}"
    }
    print ""
    previous = current
    index = index + 1
    if index < sampling.count { time.sleep(time.millis((sampling.interval * 1000.0).ceil()?)) }
  }
}
