use helper

print (helper.greet("x"))
match helper.fail() {
  Ok(_) => print "ok"
  Err(helper.HelperError.Bad {..}) => print "qualified match"
  Err(_) => print "other"
}
