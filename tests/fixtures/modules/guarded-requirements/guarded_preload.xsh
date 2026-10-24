##! Observe environment values within a temporary overlay.

## Observes a present string inside an overlay and reports absence for null.
export proc typed_preload(value: Str?) [env, error] -> Result[Str, Error] {
  if value == null { "absent" } else {
    env ({XSH_GUARDED_REQUIREMENT_TEST: value}) { env.get("XSH_GUARDED_REQUIREMENT_TEST")? }?
  }
}

## Observes a present value inside an overlay and reports absence for null.
export proc preload(value) [env, error] -> Result[Str, Error] {
  if value == null { "absent" } else {
    env ({XSH_GUARDED_REQUIREMENT_TEST: value}) { env.get("XSH_GUARDED_REQUIREMENT_TEST")? }?
  }
}
