proc value(label: Str) [time] -> Int { let _ = time.now(); print $label; 7 }
stream child(item: Int = value("default")) [time, error] -> Stream[Int] {
  defer { print "child-close" }
  yield item
  print unreachable
  yield 8
}
stream parent() [time, error] -> Stream[Int] {
  defer { print "parent-close" }
  yield @child()
}
let _ = child()
let source = parent()
let _ = child(value("supplied"))
print created
for item in source { print $item; break }
print after
