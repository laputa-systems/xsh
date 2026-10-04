error ConfigError = Missing(file: Path) : NotFound | Invalid(file: Path, message: Str)

pure check(text: Str, file: Path) -> Result[Unit, ConfigError] {
  return Err(ConfigError.Invalid(file:, message: "empty")) when text == ""
}
