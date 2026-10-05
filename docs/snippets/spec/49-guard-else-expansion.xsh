pure label(name: Str?) -> Str {
  # begin example
  if name != null {} else {
    return "anonymous"
  }

  # end example

  name.trim()
}

print label(" ada ")
