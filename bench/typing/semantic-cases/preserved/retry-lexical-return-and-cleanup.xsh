proc escape() [io, error] -> Result[Str] {
  let captured: Result[Int] = retry [] {
    defer { print "cleanup" }
    return Ok("outer")
  }
  Ok("after")
}
print ${escape()?}
