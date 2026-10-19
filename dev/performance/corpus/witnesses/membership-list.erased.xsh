enum DependencyKind { Runtime, BuildHost }

pure kind_is_selected(kind: DependencyKind, kinds) -> Bool {
  kind in kinds
}

if kind_is_selected(Runtime, [BuildHost, Runtime]) {
  print "selected"
} else {
  print "missing"
}
