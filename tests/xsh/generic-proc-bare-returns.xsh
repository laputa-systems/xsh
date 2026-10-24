proc bare_early(marker, done: Bool) {
  return when done
  let _ = marker
}

proc bare_branch(marker, done: Bool) {
  if done { return }
  let _ = marker
}

proc bare_terminal(marker) {
  let _ = marker
  return
}

proc bare_authored(marker, done: Bool) {
  return Ok() when done
  let _ = marker
}

proc bare_result_contract(marker, done: Bool) -> Result[Unit] {
  return when done
  let _ = marker
}

test generic_proc_bare_returns_preserve_unit {
  assert bare_early(7, true) == Ok()
  assert bare_early("word", false) == Ok()
  assert bare_branch(7, true) == Ok()
  assert bare_branch("word", false) == Ok()
  assert bare_terminal(7) == Ok()
  assert bare_terminal("word") == Ok()
  assert bare_authored(7, true) == Ok()
  assert bare_authored("word", false) == Ok()
  assert bare_result_contract(7, true) == Ok()
  assert bare_result_contract("word", false) == Ok()
}
