##! Shared text input for the core line-oriented applets.
## Read operands in order, treating an empty list or `-` as stdin.
export proc read_text(paths: List[Str]) [fs, error, io] -> Result[Str, Error] {
  var out = ""

  return io.stdin_text()? when paths.is_empty()

  for item in paths {
    if item == "-" {
      out = f"{out}{io.stdin_text()?}"
    } else {
      out = f"{out}{fp"{item}".read_text()?}"
    }
  }

  out
}
