#!/bin/xsh

type Options = {human: Bool, mebi: Bool, gibi: Bool, bytes: Bool, seconds: Str?, count: Str?, help: Bool, operands: List[Str]}

pure size(value: Int, divisor: Int, human: Bool) -> Str {
  if human {
    if value >= 1073741824 { return f"{(value.float() / 1073741824.0).format(1)}Gi" }
    if value >= 1048576 { return f"{(value.float() / 1048576.0).format(1)}Mi" }
    if value >= 1024 { return f"{(value.float() / 1024.0).format(1)}Ki" }
    return f"{value}B"
  }
  f"{value / divisor}"
}

proc main(...argv: List[Str]) [fs, process, time, error] {
  let opts: Options = cli.applet(argv, {gnu: {status: 2}, human: {form: "-h --human", default: false, conflicts: ["mebi", "gibi", "bytes"]}, mebi: {form: "-m --mebi", default: false, conflicts: ["human", "gibi", "bytes"]}, gibi: {form: "-g --gibi", default: false, conflicts: ["human", "mebi", "bytes"]}, bytes: {form: "-b --bytes", default: false, conflicts: ["human", "mebi", "gibi"]}, seconds: {form: "-s --seconds SECONDS"}, count: {form: "-c --count COUNT"}, help: {form: "--help", default: false}, operands: {form: "...ARG"}})?
  if opts.help { print "Usage: free [-h|-m|-g|-b] [-s SECONDS] [-c COUNT]"; return }
  if ! opts.operands.is_empty() { eprint "free: unexpected operand"; exit 2 }
  let count = if opts.count == null { if opts.seconds == null { 1 } else { 9223372036854775807 } } else { opts.count.parse_int()? }
  let delay = (opts.seconds ?? "1").parse_float()?
  if delay <= 0.0 or count <= 0 { eprint "free: interval and count must be positive"; exit 2 }
  let divisor = if opts.bytes { 1 } else if opts.gibi { 1073741824 } else if opts.mebi { 1048576 } else { 1024 }
  var iterations = 0
  loop {
    let memory = linux.meminfo()?
    let used = memory.total - memory.available
    let cache = memory.buffers + memory.cached + memory.sreclaimable
    print "               total        used        free      shared  buff/cache   available"
    print f"Mem:    {size(memory.total, divisor, opts.human):>12} {size(used, divisor, opts.human):>12} {size(memory.free, divisor, opts.human):>12} {size(memory.shared, divisor, opts.human):>12} {size(cache, divisor, opts.human):>12} {size(memory.available, divisor, opts.human):>12}"
    print f"Swap:   {size(memory.swap_total, divisor, opts.human):>12} {size(memory.swap_total - memory.swap_free, divisor, opts.human):>12} {size(memory.swap_free, divisor, opts.human):>12}"
    iterations = iterations + 1
    break when iterations >= count
    time.sleep(time.millis((delay * 1000.0).ceil()?))
    print ""
  }
}
