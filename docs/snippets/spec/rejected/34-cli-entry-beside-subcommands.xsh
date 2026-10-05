cli main version() {
  print "1.0"
}

cli main(root: Path) { # error: check.cli-entry
  print f"{root}"
}
