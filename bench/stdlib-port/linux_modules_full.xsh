# The module stream, consumed to the end: 200 full consumptions of
# `/proc/modules`, summing a field of every parsed record so the parse of each
# row is observed. The container mounts a 200-module fixture at `/proc/modules`.
proc main() [io, env, error, time] {
  var sink = 0
  var round = 0
  let start = time.now()
  while round < 200 {
    let modules = linux.modules()?.collect()
    var index = 0
    while index < modules.len() {
      sink = sink + modules[index].size
      index = index + 1
    }
    round = round + 1
  }
  let elapsed = time.now() - start
  print f"modules_full ${elapsed} ms bytes=${sink}"
}
