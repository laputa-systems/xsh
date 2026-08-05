proc load(file_path: Path) [fs, error] -> Result[Str] {
  return file_path.read_text()?
}
