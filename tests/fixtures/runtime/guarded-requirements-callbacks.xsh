type GuardedObserver = proc(value: Int) [env, error] -> Result[Int, Error]

var guarded_argument_calls = 0
var guarded_callback_calls = 0
var guarded_callback_cleanup = 0

proc guarded_argument(value: Int) [] -> Result[Int, Error] {
  guarded_argument_calls += 1
  Ok(value)
}

proc guarded_record(value: Int) [env, error] -> Result[Int, Error] {
  let _ = env.get_or("XSH_GUARDED_CALLBACK_TEST", "absent")?
  guarded_callback_calls += 1
  defer { guarded_callback_cleanup += 1 }
  Ok(value + 10)
}

proc guarded_fixed_callback(action, value: Int) [env, error] -> Result[Unit, Error] {
  if action != null { let _ = action(value)? }
  Ok()
}

proc guarded_inferred_callback(action, value: Int) -> Result[Unit, Error] {
  if action != null { let _ = action(value)? }
  Ok()
}

proc guarded_callback_absence() [] -> Result[Unit, Error] {
  guarded_inferred_callback(null, 0)
}

proc verify_guarded_callbacks() [env, error] -> Result[Unit] {
  let action: GuardedObserver = guarded_record
  let present: GuardedObserver? = action
  let missing: GuardedObserver? = null
  let absent = guarded_callback_absence()
  assert absent is Ok(_)
  assert guarded_callback_calls == 0
  let _ = guarded_fixed_callback(action, guarded_argument(1)?)?
  let _ = guarded_fixed_callback(present, guarded_argument(2)?)?
  let _ = guarded_fixed_callback(missing, guarded_argument(3)?)?
  let _ = guarded_fixed_callback(null, guarded_argument(4)?)?
  let _ = guarded_inferred_callback(action, guarded_argument(5)?)?
  let _ = guarded_inferred_callback(present, guarded_argument(6)?)?
  let _ = guarded_inferred_callback(missing, guarded_argument(7)?)?
  let _ = guarded_inferred_callback(null, guarded_argument(8)?)?
  assert guarded_argument_calls == 8
  assert guarded_callback_calls == 4
  assert guarded_callback_cleanup == 4
  Ok()
}
