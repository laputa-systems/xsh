# begin example
proc head_commit() -> Result[Str] {
  let text = run.text sh -c "echo abc123"
  Ok(text.trim())
}

# end example

print ${head_commit()?}
