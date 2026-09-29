error FetchError = Busy(message: Str) | Fatal(message: Str)

proc attempt() -> Result[Int, FetchError] { 42 }

proc main() -> Result[Unit] {
  let result = retry [0ms] on (FetchError.Busy) { attempt()? }
  match result {
    Ok(value) => print ${value}
    Err(_) => {}
  }
}
