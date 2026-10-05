proc publish(build: Path, release: Path, notes: Str) [fs, error] -> Result[Str] {
  build.copy(release) # error: check.named-arg
  release.symlink(build) # error: check.named-arg
  build.rename(dest: release) # error: check.named-arg
  Ok(notes.replace("DRAFT", "FINAL")) # error: check.named-arg
}
