var attempts = 0

error RetryExampleError = Transient(message: Str)

proc fetch_index() -> Result[Str] {
  attempts += 1

  if attempts < 3 {
    return Err(RetryExampleError.Transient(message: f"attempt ${attempts}"))
  }

  "index"
}

let body = retry [0ms, 0ms] {
  fetch_index()?
}?

print f"${body} after ${attempts} attempts"
