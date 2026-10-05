proc load(file: Path) [fs, error] -> Result[Str] {
  without error { # error: check.without-effect
    print "bounded"
  }
  file.read_text()
}
