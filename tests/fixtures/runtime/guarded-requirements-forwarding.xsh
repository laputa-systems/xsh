use guarded_forwarding

proc verify_guarded_forwarding() -> Result[Unit] {
  let present: Str? = "present"
  let missing: Str? = null
  assert guarded_forwarding.render("plain") == "plain"
  assert guarded_forwarding.render(present) == "present"
  assert guarded_forwarding.render(missing) == ""
  assert guarded_forwarding.render(null) == ""
  assert guarded_forwarding.repeated_render("twice") == "twice"
  assert guarded_forwarding.repeated_render(present) == "present"
  assert guarded_forwarding.repeated_render(null) == ""
  assert guarded_forwarding.preload("plain")? == "plain"
  assert guarded_forwarding.preload(present)? == "present"
  assert guarded_forwarding.preload(missing)? == "absent"
  assert guarded_forwarding.preload(null)? == "absent"
  Ok()
}
