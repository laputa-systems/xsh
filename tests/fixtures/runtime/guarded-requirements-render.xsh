use guarded_render

proc verify_guarded_render() -> Result[Unit] {
  let present: Str? = "kept"
  assert guarded_render.typed_render("plain") == "plain"
  assert guarded_render.typed_render(present) == "kept"
  assert guarded_render.typed_render(null) == ""
  assert guarded_render.render("plain") == "plain"
  assert guarded_render.render(present) == "kept"
  assert guarded_render.render(null) == ""
}
