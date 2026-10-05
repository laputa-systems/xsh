##! A semantic version parser imported by the native-test example.

## A parsed `MAJOR.MINOR.PATCH` version.
export type Version = {major: Int, minor: Int, patch: Int}

## Parses `MAJOR.MINOR.PATCH`.
export pure parse(text: Str) -> Result[Version, Error] {
  let parts = text.split(".")
  guard parts.len() == 3 else {
    error.fail(f"not a semantic version: {text}")
    return Version(major: 0, minor: 0, patch: 0)
  }

  Version(major: parts[0] as Int, minor: parts[1] as Int, patch: parts[2] as Int)
}
