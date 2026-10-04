error DelegationError = Late(message: Str)

proc fail() [] -> Result[Unit, DelegationError] {
  return Err(DelegationError.Late(message: "late-delegation-error"))
}

proc close_depth(depth: Int) [io] {
  if depth == 0 or depth == 3000 { print f"closed {depth}" }
}

stream descend(depth: Int) [io, error] -> Stream[Int] {
  defer close_depth(depth)
  if depth > 0 {
    yield @descend(depth - 1)
  } else {
    yield 7
    fail()?
    yield 8
  }
}

proc main() [io, error] {
  for row in descend(3000) { print f"row {row}" }
}
