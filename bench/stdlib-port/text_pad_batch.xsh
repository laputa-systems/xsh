proc main() [io] {
  var seed: List[Str] = ["\x1b[31mred\x1b[0m", "plain", "wide 日本"]
  var out: List[Str] = []
  var round = 0
  while round < 700 {
    out = out.extend([tui.left_pad(t, 24) for t in seed])
    round = round + 1
  }
  print "${out.len()}"
}
