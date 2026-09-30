error LocalError = Bad(message: Str)
proc fail() [] -> Result[Int, LocalError] { Err(LocalError.Bad("kept")) }
proc discarded() [io, error] -> Unit {
  defer { print "cleanup" }
  let _ = fail()?
  print unreachable
}
let result: Result[Unit] = try { discarded() }
print ${result is Err(LocalError.Bad)}
