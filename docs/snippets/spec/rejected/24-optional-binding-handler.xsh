# begin example
pure label(name: Str?) -> Str {
  guard let found = name else { |failure|  # error: check.block-params
    return "none"
  }

  found
}
# end example
