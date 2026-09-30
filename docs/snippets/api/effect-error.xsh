proc load(file_path: Path) [fs, error] -> Result[Str] {
  file_path.read_text()?
}
