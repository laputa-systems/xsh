# The os-release entry: 200 calls, each reading and parsing `/etc/os-release`
# and reading three of the parsed fields.
proc main() [io, env, error, time] {
  var sink = 0
  var round = 0
  let start = time.now()
  while round < 200 {
    let release = system.os_release()?
    sink = sink + release.id.byte_len() + release.name.byte_len() + release.pretty_name.byte_len()
    round = round + 1
  }
  let elapsed = time.now() - start
  print f"os_release ${elapsed} ms bytes=${sink}"
}
