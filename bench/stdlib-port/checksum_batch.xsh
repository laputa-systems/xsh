proc main() [io, error] {
  var sink = 0
  var i = 0
  while i < 500 {
    let parsed = hash.parse_check_line("d41d8cd98f00b204e9800998ecf8427e  build/out.bin")?
    sink = sink + parsed.hex.byte_len() + parsed.path.byte_len()
    i = i + 1
  }
  print "${sink}"
}
