proc main(...argv: List[Str]) {
  let name = argv.get(0, "world")
  print "hello" $name
}
