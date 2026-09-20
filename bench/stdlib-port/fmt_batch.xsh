proc main() [io, time] {
  var sink = 0
  var i = 0
  while i < 2000 {
    sink = sink + bytes.human(i).byte_len() + time.duration_compact(i).byte_len()
    i = i + 1
  }
  print "${sink}"
}
