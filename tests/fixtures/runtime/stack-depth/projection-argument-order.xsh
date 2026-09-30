type Leaf = { value: Int }
proc rows(fail: Bool) [error] -> Result[List[Leaf]] {
  defer { print "base cleanup" }
  print "base"
  if fail { error.fail("base failed")? }
  Ok([{value: 7}])
}
proc position() [] -> Int { print "index"; 0 }
proc expected() [] -> Int { print "expected"; 7 }
test.eq(rows(false)?[position()].value, expected())?
let captured: Result[Unit] = try {
  defer { print "outer cleanup" }
  test.eq(rows(true)?[position()].value, expected())?
}
print (captured is Err(_))
