proc early() [io] -> Int {
  defer { print "cleanup" }
  let _ = { return 7; 9 }
  11
}
print ${early()}
