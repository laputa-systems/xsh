#!/bin/xsh
use lib.procps

type Options = {pids: Str, memory: Bool, command: Str?, help: Bool, operands: List[Str]}
proc main(...argv: List[Str]) [fs, process, time, error] {
  let opts: Options = cli.applet(argv, {gnu: {status: 2}, pids: {form: "-p PID", default: "ALL"}, memory: {form: "-r", default: false}, command: {form: "-C COMMAND"}, help: {form: "--help", default: false}, operands: {form: "...ARG"}})?
  if opts.help { print "Usage: pidstat [-p PID,...|ALL] [-r] [-C PATTERN] [INTERVAL [COUNT]]"; return }
  let sampling = procps.sampling_args(opts.operands)?
  let pids = if opts.pids == "ALL" { [] } else { procps.ids(opts.pids)? }
  let pattern = regex.compile(opts.command ?? "")?
  var previous: LinuxSample? = null
  var index = 0
  while index < sampling.count {
    let current = linux.sample()?
    let elapsed_ms = if previous == null { current.uptime_ms } else { current.sampled_at_ms - previous.sampled_at_ms }
    if elapsed_ms <= 0 { return Err(error.failure("sampling clock did not advance")) }
    let header = if opts.memory { "UID PID VSZ RSS %MEM Command" } else { "UID PID %usr %system %CPU CPU Command" }
    print $header
    for row in current.processes {
      continue when (! pids.is_empty() and row.pid not in pids) or ! pattern.matches(row.command)
      let older: List[LinuxProcessSample] = if previous == null { [] } else { previous.processes |> where .pid == row.pid and .start_ticks == row.start_ticks }
      if opts.memory {
        let percent = 100.0 * row.rss_bytes.float() / current.memory.total.float()
        print f"{row.uid} {row.pid} {row.vsize_bytes / 1024} {row.rss_bytes / 1024} {percent.format(2)} {row.command}"
      } else {
        let scale = 100000.0 / elapsed_ms.float() / current.ticks_per_second.float()
        let cpu_user = procps.delta(row.user_ticks, if older.is_empty() { 0 } else { older[0].user_ticks })?.float() * scale
        let cpu_system = procps.delta(row.system_ticks, if older.is_empty() { 0 } else { older[0].system_ticks })?.float() * scale
        print f"{row.uid} {row.pid} {cpu_user.format(2)} {cpu_system.format(2)} {(cpu_user + cpu_system).format(2)} {row.processor} {row.command}"
      }
    }
    previous = current
    index = index + 1
    if index < sampling.count { time.sleep(time.millis((sampling.interval * 1000.0).ceil()?)) }
  }
}
