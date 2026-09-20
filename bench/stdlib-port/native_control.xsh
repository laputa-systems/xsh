proc main() [io, error] {
  var sink = 0
  var round = 0
  while round < 20 {
    for entry in fs.dirs(p"../../core") {
      sink = sink + entry.name.byte_len()
    }
    round = round + 1
  }
  print "${sink}"
}
