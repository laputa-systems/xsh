var guarded_known_calls = 0
var guarded_known_cleanup = 0

proc guarded_known_observe(value: Int) -> Result[Int, Error] {
  let _ = env.get_or("XSH_GUARDED_DECLARATION_TEST", "absent")?
  guarded_known_calls += 1
  defer { guarded_known_cleanup += 1 }
  Ok(value + 1)
}

proc guarded_known_target(marker, value: Int) -> Result[Unit, Error] {
  if marker != null { let _ = guarded_known_observe(value)? }
  Ok()
}

proc guarded_known_absence() [] -> Result[Unit, Error] {
  guarded_known_target(null, 0)
}

proc verify_guarded_known_targets() [env, error] -> Result[Unit, Error] {
  let present: Bool? = true
  let missing: Bool? = null
  let absent = guarded_known_absence()
  assert absent is Ok(_)
  assert guarded_known_calls == 0
  let _ = guarded_known_target(true, 1)?
  let _ = guarded_known_target(present, 2)?
  let _ = guarded_known_target(missing, 3)?
  let _ = guarded_known_target(null, 4)?
  assert guarded_known_calls == 2
  assert guarded_known_cleanup == 2
  Ok()
}
