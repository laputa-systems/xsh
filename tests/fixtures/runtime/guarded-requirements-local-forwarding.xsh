use guarded_forwarding

pure keep(value) { value }

pure guarded_identity(value) -> Str {
  if value == null { "" } else {
    let copy = keep(value)
    f"{copy}"
  }
}

pure guarded_relay(value) -> Str {
  if value == null { "" } else { guarded_forwarding.render(value) }
}

pure guarded_nested(value, other) -> Str {
  if value == null { "" } else {
    if other == null {
      guarded_forwarding.render(value)
    } else {
      guarded_forwarding.render(value) + guarded_forwarding.render(other)
    }
  }
}

proc verify_guarded_local_forwarding() -> Result[Unit] {
  let present: Str? = "present"
  let missing: Str? = null
  assert guarded_identity("plain") == "plain"
  assert guarded_identity(present) == "present"
  assert guarded_identity(missing) == ""
  assert guarded_identity(null) == ""
  assert guarded_relay("plain") == "plain"
  assert guarded_relay(present) == "present"
  assert guarded_relay(missing) == ""
  assert guarded_relay(null) == ""
  assert guarded_nested("left", "right") == "leftright"
  assert guarded_nested(present, null) == "present"
  assert guarded_nested(present, present) == "presentpresent"
  assert guarded_nested(null, "right") == ""
  Ok()
}
