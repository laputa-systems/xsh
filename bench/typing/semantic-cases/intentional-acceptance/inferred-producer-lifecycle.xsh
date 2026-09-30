proc mark(label, value) { let _ = time.now(); print $label; value }
stream child(item = mark("default", 7)) {
  defer { print "child-close" }
  yield item
  yield 8
}
stream parent() { defer { print "parent-close" }; yield @child() }
let _ = child()
let source = parent()
let _ = child(mark("supplied", 9))
print created
for item in source { print $item; break }
print after
