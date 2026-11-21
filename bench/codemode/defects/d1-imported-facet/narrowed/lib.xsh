##! Errors for the probe.

## A failure.
export error LibError = Down : Transient | Gone

## Fails inside the declaring module.
export pure fail_here() -> Result[Int, LibError] {
  Err(LibError.Down("inside"))
}

## Whether an error has the facet, tested inside the declaring module.
export pure has_facet(error: Error) -> Bool {
  error is Transient
}
