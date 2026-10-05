proc publish(build: Path, release: Path, current: Path, notes: Str) [fs, error] -> Result[Str] {
  # begin example
  build.copy(to: release)
  current.symlink(to: release)
  let text = notes.replace("DRAFT", with: "FINAL")
  # end example
  Ok(text)
}
