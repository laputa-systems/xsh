pure choose_text(s: Str) -> Int {
  match s {
    "a" => 1
    "b" => 2
    else => 3
  }
}
test decisions {
  var n = 0
  while n < 2 { n += 1 }
  assert choose_text("b") == 2
  assert false or true
}
