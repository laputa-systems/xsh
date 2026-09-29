proc close_depth(depth: Int) [io] {
  if depth == 0 or depth == 3000 { print f"closed ${depth}" }
}

stream descend(depth: Int) [io] -> Stream[Int] {
  defer close_depth(depth)
  if depth > 0 {
    yield @descend(depth - 1)
  } else {
    yield @[7, 8]
  }
}

proc main() [io] {
  for n in descend(3000) { print f"full ${n}" }
  for n in descend(3000) {
    print f"early ${n}"
    break
  }
}
