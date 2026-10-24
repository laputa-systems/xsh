use left_handler
use right_handler

pure dispatch_render(choose_left: Bool, value) -> Str {
  let selected = if choose_left { left_handler.render } else { right_handler.render }
  selected(value)
}

pure dispatch_relay(choose_left: Bool, value) -> Str {
  let selected = if choose_left { left_handler.relay } else { right_handler.relay }
  selected(value)
}

pure dispatch_parent(choose_left: Bool, value) {
  let selected = if choose_left { left_handler.parent } else { right_handler.parent }
  selected(value)
}

proc verify_guarded_correspondence() [error] -> Result[Unit] {
  let present_number: Int? = 13
  let missing_number: Int? = null
  let present_text: Str? = "present"
  let missing_text: Str? = null
  let relative: RelPath = p"relative/leaf"
  let present_path: Path? = p"/present/leaf"
  let missing_path: Path? = null
  for choose_left in [true, false] {
    let selected = if choose_left { left_handler.render } else { right_handler.render }
    let prefix = if choose_left { "left:" } else { "right:" }
    assert selected(7) == prefix + "7"
    assert selected("text") == prefix + "text"
    assert selected(present_number) == prefix + "13"
    assert selected(missing_number) == prefix + "null"
    assert selected(present_text) == prefix + "present"
    assert selected(missing_text) == prefix + "null"
    assert selected(null) == prefix + "null"
    let relay = if choose_left { left_handler.relay } else { right_handler.relay }
    assert relay(19) == prefix + "19"
    assert relay("forwarded") == prefix + "forwarded"
    assert relay(present_number) == prefix + "13"
    assert relay(missing_text) == prefix + "null"
    assert relay(null) == prefix + "null"
    assert dispatch_render(choose_left, 23) == prefix + "23"
    assert dispatch_render(choose_left, "dispatch") == prefix + "dispatch"
    assert dispatch_render(choose_left, present_number) == prefix + "13"
    assert dispatch_render(choose_left, missing_text) == prefix + "null"
    assert dispatch_render(choose_left, null) == prefix + "null"
    assert dispatch_relay(choose_left, 29) == prefix + "29"
    assert dispatch_relay(choose_left, "relay") == prefix + "relay"
    assert dispatch_relay(choose_left, present_text) == prefix + "present"
    assert dispatch_relay(choose_left, missing_number) == prefix + "null"
    assert dispatch_relay(choose_left, null) == prefix + "null"
    let project = if choose_left { left_handler.parent } else { right_handler.parent }
    let exact_parent: Path = project(p"/plain/leaf")
    let relative_parent: RelPath = project(relative)
    let optional_parent: Path? = project(present_path)
    let missing_parent: Path? = project(missing_path)
    let absent_parent: Null = project(null)
    assert exact_parent == p"/plain"
    assert relative_parent == p"relative"
    assert optional_parent == p"/present"
    assert missing_parent == null
    assert absent_parent == null
    let relay_project = if choose_left { left_handler.relay_parent } else { right_handler.relay_parent }
    let forwarded_relative: RelPath = relay_project(relative)
    let forwarded_absent: Null = relay_project(null)
    let dispatched_relative: RelPath = dispatch_parent(choose_left, relative)
    let dispatched_absent: Null = dispatch_parent(choose_left, null)
    assert forwarded_relative == p"relative"
    assert forwarded_absent == null
    assert dispatched_relative == p"relative"
    assert dispatched_absent == null
  }
  let handlers = [left_handler.render, right_handler.render]
  assert handlers[0](17) == "left:17"
  assert handlers[1]("list") == "right:list"
  assert handlers[0](present_text) == "left:present"
  assert handlers[1](null) == "right:null"
  Ok()
}
