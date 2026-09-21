proc main() [io, fs, error] {
  # A large Unicode-containing fixture, read once and wrapped in a bounded
  # number of passes so the per-line cost is what the workload measures.
  let text = fs.read_text(p"fixtures/unicode.txt")?
  var sink = 0
  var round = 0
  while round < 6 {
    sink = sink + [line.byte_len() for line in text.wrap(72)].len()
    sink = sink + text.fields().len()
    round = round + 1
  }
  print f"${sink}"
}
