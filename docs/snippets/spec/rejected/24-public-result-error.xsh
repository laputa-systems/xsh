##! Configuration loading.

# begin example
## Why a configuration was refused.
export error ConfigError = Empty(file: Path)

## Reads a configuration file.
export proc read(file: Path) [fs, error] -> Result[Str] {  # error: check.public-result-error
  first_line(file)
}

## Reads a configuration file, failing broadly.
export proc read_any(file: Path) [fs, error] -> Result[Str, Error] {
  first_line(file)
}

## Refuses an empty configuration.
export pure checked(file: Path, text: Str) -> Result[Str, ConfigError] {
  return Err(.Empty(file:)) when text == ""
  text
}

proc first_line(file: Path) [fs, error] -> Result[Str] {
  file.read_lines()?.get(0)
}
# end example
