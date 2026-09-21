proc main() [io, error] {
  var sink = 0
  var round = 0
  while round < 500 {
    let by_ext = mime.lookup_ext("tar.gz") ?? {mime: "missing", exts: []}
    let by_path = mime.lookup_path(p"build/out.json") ?? {mime: "missing", exts: []}
    sink = sink + by_ext.mime.byte_len() + by_path.mime.byte_len()
    round = round + 1
  }
  print f"${sink}"
}
