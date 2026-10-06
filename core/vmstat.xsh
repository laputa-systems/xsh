#!/bin/xsh
use lib.procps

type Options = {no_header: Bool, help: Bool, operands: List[Str]}
proc main(...argv: List[Str]) [fs, process, time, error] {
  let opts: Options = cli.applet(argv, {gnu: {status: 2}, no_header: {form: "-n --one-header", default: false}, help: {form: "--help", default: false}, operands: {form: "...ARG"}})?
  if opts.help { print "Usage: vmstat [-n] [INTERVAL [COUNT]]"; return }
  let sampling = procps.sampling_args(opts.operands)?
  var previous: LinuxSample? = null
  var index = 0
  while index < sampling.count {
    let current = linux.sample()?
    let cpu_usage = procps.cpu_percent(current.cpu, if previous == null { null } else { previous.cpu })?
    let milliseconds = if previous == null { current.uptime_ms } else { current.sampled_at_ms - previous.sampled_at_ms }
    if milliseconds <= 0 { return Err(error.failure("sampling clock did not advance")) }
    let scale = 1000.0 / milliseconds.float()
    let si = procps.delta(current.swap_in_pages, if previous == null { 0 } else { previous.swap_in_pages })?.float() * current.page_size.float() / 1024.0 * scale
    let so = procps.delta(current.swap_out_pages, if previous == null { 0 } else { previous.swap_out_pages })?.float() * current.page_size.float() / 1024.0 * scale
    let bi = procps.delta(current.page_in_kib, if previous == null { 0 } else { previous.page_in_kib })?.float() * scale
    let bo = procps.delta(current.page_out_kib, if previous == null { 0 } else { previous.page_out_kib })?.float() * scale
    let interrupts = procps.delta(current.interrupts, if previous == null { 0 } else { previous.interrupts })?.float() * scale
    let switches = procps.delta(current.context_switches, if previous == null { 0 } else { previous.context_switches })?.float() * scale
    if index == 0 or ! opts.no_header { print "procs -----------memory---------- ---swap-- -----io---- -system-- ------cpu-----"; print " r  b   swpd   free   buff  cache   si   so    bi    bo   in   cs us sy id wa st" }
    let memory = current.memory
    print f"{current.running} {current.blocked} {(memory.swap_total - memory.swap_free) / 1024} {memory.free / 1024} {memory.buffers / 1024} {(memory.cached + memory.sreclaimable) / 1024} {si.format(0)} {so.format(0)} {bi.format(0)} {bo.format(0)} {interrupts.format(0)} {switches.format(0)} {(cpu_usage.user + cpu_usage.nice).format(0)} {(cpu_usage.system + cpu_usage.irq + cpu_usage.softirq).format(0)} {cpu_usage.idle.format(0)} {cpu_usage.wait.format(0)} {cpu_usage.steal.format(0)}"
    previous = current
    index = index + 1
    if index < sampling.count { time.sleep(time.millis((sampling.interval * 1000.0).ceil()?)) }
  }
}
