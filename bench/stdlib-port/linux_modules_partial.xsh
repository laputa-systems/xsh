# The module stream, consumed only as far as its first record: 200 partial
# consumptions. A producer that interprets rows as they are consumed does work
# proportional to what the consumer takes; one that materializes first does the
# same work as a full consumption.
proc main() [io, env, error, time] {
  var sink = 0
  var round = 0
  let start = time.now()
  while round < 200 {
    let first = linux.modules()? |> first()?
    sink = sink + first.size
    round = round + 1
  }
  let elapsed = time.now() - start
  print f"modules_partial ${elapsed} ms bytes=${sink}"
}
