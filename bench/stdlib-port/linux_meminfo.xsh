# The Linux memory entry: 200 calls, each parsing every line of
# `/proc/meminfo` through the interpreter and reporting a field from the parsed
# record, so the parse is observed rather than skipped.
proc main() [io, env, error, time] {
  var sink = 0
  var round = 0
  let start = time.now()
  while round < 200 {
    let memory = linux.meminfo()?
    sink = sink + memory.total + memory.available + memory.swap_free
    round = round + 1
  }
  let elapsed = time.now() - start
  print f"meminfo ${elapsed} ms bytes=${sink}"
}
