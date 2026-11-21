use lib

proc fail() -> Result[Int] {
  Err(lib.LibError.Down("again"))
}

match fail() {
  Err(is lib.Transient) => print "facet pattern matched"
  Err(lib.LibError.Down {..}) => print "only the variant pattern matched"
  Err(e) => print f"nothing matched: {e.message}"
  Ok(_) => print "ok"
}

var attempts = 0

proc flaky() -> Result[Int] {
  attempts += 1
  if attempts < 3 {
    return Err(lib.LibError.Down("again"))
  }

  Ok(attempts)
}

let retried = retry [10ms, 20ms] on (is lib.Transient) { flaky()? }
print f"retry on the facet: {retried ?? -1} after {attempts} attempts"
