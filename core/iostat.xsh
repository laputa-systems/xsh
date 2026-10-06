#!/bin/xsh
use lib.procps

type Options = {cpu: Bool, devices: Bool, mebi: Bool, no_first: Bool, help: Bool, operands: List[Str]}
proc main(...argv: List[Str]) [fs, process, time, error] {
  let opts: Options = cli.applet(argv, {gnu: {status: 2}, cpu: {form: "-c", default: false}, devices: {form: "-d", default: false}, mebi: {form: "-m", default: false}, no_first: {form: "-y", default: false}, help: {form: "--help", default: false}, operands: {form: "...ARG"}})?
  if opts.help { print "Usage: iostat [-cdmy] [INTERVAL [COUNT]]"; return }
  let sampling = procps.sampling_args(opts.operands)?
  var previous: LinuxSample? = null
  var index = 0
  while index < sampling.count {
    let current = linux.sample()?
    if ! opts.no_first or previous != null {
      let cpu_usage = procps.cpu_percent(current.cpu, if previous == null { null } else { previous.cpu })?
      if opts.cpu or ! opts.devices { print "avg-cpu: %user %nice %system %iowait %steal %idle"; print f"{cpu_usage.user.format(2)} {cpu_usage.nice.format(2)} {(cpu_usage.system + cpu_usage.irq + cpu_usage.softirq).format(2)} {cpu_usage.wait.format(2)} {cpu_usage.steal.format(2)} {cpu_usage.idle.format(2)}" }
      if opts.devices or ! opts.cpu {
        let header = if opts.mebi { "Device tps MB_read/s MB_wrtn/s MB_read MB_wrtn" } else { "Device tps kB_read/s kB_wrtn/s kB_read kB_wrtn" }
        print $header
        let elapsed_ms = if previous == null { current.uptime_ms } else { current.sampled_at_ms - previous.sampled_at_ms }
        if elapsed_ms <= 0 { return Err(error.failure("sampling clock did not advance")) }
        let rate = 1000.0 / elapsed_ms.float()
        let divisor = if opts.mebi { 2048.0 } else { 2.0 }
        for disk in current.disks {
          let older: List[LinuxDiskSample] = if previous == null { [] } else { previous.disks |> where .major == disk.major and .minor == disk.minor and .name == disk.name }
          let read = procps.delta(disk.sectors_read, if older.is_empty() { 0 } else { older[0].sectors_read })?.float() / divisor
          let written = procps.delta(disk.sectors_written, if older.is_empty() { 0 } else { older[0].sectors_written })?.float() / divisor
          let transactions = procps.delta(disk.reads_completed + disk.writes_completed, if older.is_empty() { 0 } else { older[0].reads_completed + older[0].writes_completed })?.float() * rate
          print f"{disk.name} {transactions.format(2)} {(read * rate).format(2)} {(written * rate).format(2)} {read.format(0)} {written.format(0)}"
        }
      }
      print ""
      index = index + 1
    }
    previous = current
    if index < sampling.count { time.sleep(time.millis((sampling.interval * 1000.0).ceil()?)) }
  }
}
