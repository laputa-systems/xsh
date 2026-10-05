proc number(names: List[Str]) [io] {
  # begin example
  for i, name in names {
    print f"{i + 1}. {name}"
  }
  # end example
}

number(["build", "test"])
