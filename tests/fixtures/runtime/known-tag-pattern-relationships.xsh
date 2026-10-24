enum Block {
  Para(Str),
  Pair(Str, Str),
  Stop,
}

pure render(block, marker) -> Str {
  let _ = marker
  match block {
    Para(text) => text
    Pair(left, right) => left + right
    Stop => "stop"
  }
}

pure first(items, marker) -> Str {
  let _ = marker
  if let Para(text) = items[0] { return text }
  "stop"
}

pure ignored(block, marker) -> Bool {
  let _ = marker
  if let Para(_) = block { return true }
  false
}

pure stopped(block: Block, marker) -> Bool {
  let _ = marker
  block is Stop
}

pure dynamic(block: Any) -> Bool {
  if let Para(_) = block { return true }
  false
}

assert render(Para("kept"), 1) == "kept"
assert render(Pair("left", "right"), "marker") == "leftright"
assert render(Stop, false) == "stop"
assert first([Para("first")], "marker") == "first"
assert first([Stop], 9) == "stop"
assert first({[0]: Para("mapped")}, false) == "mapped"
assert ignored(Para("unused"), 1)
assert !ignored(Stop, "marker")
assert dynamic(Para("dynamic"))
assert !dynamic(11)
assert stopped(Stop, 1)
assert !stopped(Para("other"), "marker")

pure dynamic_predicate(block: Any, marker) -> Bool {
  let _ = marker
  block is Para(_)
}

assert dynamic_predicate(Para("dynamic predicate"), 1)
assert !dynamic_predicate(17, "marker")
