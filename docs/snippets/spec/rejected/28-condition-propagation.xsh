pure known(name: Str) -> Result[Bool] {
  Ok(name != "")
}

pure label(name: Str) -> Str {
  if known(name) { # error: check.try-context
    return name
  }

  "anonymous"
}

pure exact(name: Str) -> Result[Str] {
  if known(name) == true { # error: check.type-mismatch
    return Ok(name)
  }

  Ok("anonymous")
}
