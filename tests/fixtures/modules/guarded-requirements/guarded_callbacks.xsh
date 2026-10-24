##! Invoke present callbacks with conditional effects.

## Invokes a present callback within a written effect bound.
export proc fixed(action, value: Int) [env, error] -> Result[Unit, Error] {
  if action != null { let _ = action(value)? }
  Ok()
}

## Invokes a present callback and preserves exact null absence.
export proc invoke(action, value: Int) -> Result[Unit, Error] {
  if action != null { let _ = action(value)? }
  Ok()
}

## Forwards the callback requirement and its conditional effects.
export proc relay(action, value: Int) -> Result[Unit, Error] {
  invoke(action, value)
}

## Omits callback invocation when no callback is supplied.
export proc absent() [] -> Result[Unit, Error] {
  relay(null, 0)
}
