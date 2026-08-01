proc main(...argv: List[Str]) [io] {
  let name = argv.get(0, "world")
  print "hello" $name
}
