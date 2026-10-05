proc save(out: Path, text: Str) [fs, error] {
  fs.write(out, text) # error: check.removed-fs-function
}
