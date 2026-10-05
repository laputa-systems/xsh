proc describe(out: Path) [fs, error] -> Result[Str] {
  # begin example
  return Ok("link") when out.is_symlink()
  let kind = if out.is_dir() { "directory" } else if out.is_file() { "file" } else { "other" }
  # end example
  Ok(kind)
}
