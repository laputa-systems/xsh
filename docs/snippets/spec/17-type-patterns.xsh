const input = "3"

# begin example
match json.decode(input)? {
  i is Int => print i.float()
  f is Float => print ${f}
  _ is Null => print "null"
  else => print "other"
}
# end example
