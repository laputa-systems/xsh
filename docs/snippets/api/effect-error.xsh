proc load(path: Path) [fs, error] -> Result[Str] {
  return path.read_text()?
}
