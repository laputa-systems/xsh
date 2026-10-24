proc consume(value: Int) [io] -> Unit { print $value }

proc execute(step) {
  let preload = step.preload
  if preload == null {
    consume(step.spec)
  } else {
    env ({LD_PRELOAD: preload}) { consume(step.spec) }
  }
}

proc verify_guarded_row_null_input() [env, io, error] -> Result[Unit] {
  let before = env.get_or("LD_PRELOAD", "")?
  execute({spec: 1, preload: null})?
  execute({spec: 2, preload: "library.so"})?
  let retained: Result[Int, Str] = Err("opaque")
  let entry = {spec: 3, preload: null, retained}
  execute(entry)?
  assert entry.retained == retained
  let maybe: Str? = null
  execute({spec: 4, preload: maybe})?
  assert env.get_or("LD_PRELOAD", "")? == before
  Ok()
}
