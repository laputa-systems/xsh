##! Closed effect contract for a stage boundary.
## Prints the requested integer and returns successful completion.
export proc execute(value: Int) [io, error] -> Result[Unit, Error] {
  print $value
  Ok()
}
