var kept: Path? = null
tempdir dir {
  kept = dir
}
if kept != null {
  print f"{kept.exists()?}"
}
