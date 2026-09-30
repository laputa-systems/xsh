pure gather() {
  var entries = []
  for destination in [p"first", p"second"] {
    entries += [destination]
  }
  entries
}
print ${gather()[0].display()}
