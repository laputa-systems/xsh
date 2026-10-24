use guarded_preload

proc verify_guarded_preload() [env, error] -> Result[Unit] {
  let present: Str? = "kept"
  let before = env.get_or("XSH_GUARDED_REQUIREMENT_TEST", "outer")?
  assert guarded_preload.typed_preload("plain")? == "plain"
  assert guarded_preload.typed_preload(present)? == "kept"
  assert guarded_preload.typed_preload(null)? == "absent"
  assert guarded_preload.preload("plain")? == "plain"
  assert guarded_preload.preload(present)? == "kept"
  assert guarded_preload.preload(null)? == "absent"
  assert env.get_or("XSH_GUARDED_REQUIREMENT_TEST", "outer")? == before
}
