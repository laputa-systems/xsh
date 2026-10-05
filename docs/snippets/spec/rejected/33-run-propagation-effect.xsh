proc revision() [process] -> Result[Str] {
  let text = run.text sh -c "echo abc123" # error: check.effect-violation
  Ok(text.trim())
}

print ${revision()?}
