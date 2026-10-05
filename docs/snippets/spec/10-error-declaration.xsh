error ConfigError = Missing(file: Path) : NotFound | Invalid(file: Path, message: Str)

error FetchError {
    Usage
    Offline : Timeout
    Rejected(url: Str, status: Int)
}

pure check(text: Str, file: Path) -> Result[Unit, ConfigError] {
  return Err(ConfigError.Invalid(file, "empty")) when text == ""
}

pure parse_url(url: Str) -> Result[Str, FetchError] {
  return Err(FetchError.Usage(f"not a URL: {url}")) unless url.starts_with("https://")
  Ok(url)
}
