error FetchError = Transient(message: Str) : Transient | Fatal(message: Str)

var attempts = 0

proc fetch_index() [error] -> Result[Str] {
  attempts += 1
  if attempts < 3 {
    return Err(FetchError.Transient(message: f"attempt ${attempts}: connection reset"))
  }

  "index-v42"
}

let index = retry [100ms, 200ms, 400ms] on (is Transient) {
  fetch_index()?
}?

print f"${index} after ${attempts} attempts"
