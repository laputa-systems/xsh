proc choose() -> Path? {
  var selected = null
  for destination in [p"release"] {
    selected = destination
  }
  selected
}
print ${choose()?.display() ?? ""}
