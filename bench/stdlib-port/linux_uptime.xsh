# The Linux uptime entry: 200 calls, each an eager read of `/proc/uptime`
# through the interpreter, in one process so the per-call cost is what is
# measured rather than process startup.
proc main() [io, env, error, time] {
  var sink = 0
  var round = 0
  let start = time.now()
  while round < 200 {
    sink = sink + unix.uptime_seconds()?
    round = round + 1
  }
  let elapsed = time.now() - start
  print f"uptime ${elapsed} ms seconds=${sink}"
}
