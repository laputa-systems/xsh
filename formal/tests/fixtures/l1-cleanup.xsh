{
  defer { print "first" }
  errdefer { print "error" }
  defer { print "last" }
  print "body"
}
