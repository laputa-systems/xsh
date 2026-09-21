# The system memory entry: 200 calls, each parsing `/proc/meminfo` and reading
# three of the parsed fields.
proc main() [io, env, error, time] {
  var sink = 0
  var round = 0
  let start = time.now()
  while round < 200 {
    let memory = system.memory()?
    sink = sink + memory.total + memory.free + memory.swap_total
    round = round + 1
  }
  let elapsed = time.now() - start
  print f"memory ${elapsed} ms bytes=${sink}"
}
