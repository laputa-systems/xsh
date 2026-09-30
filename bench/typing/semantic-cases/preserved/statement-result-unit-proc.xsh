error LocalError = Bad(message: Str)
proc fail() [] -> Result[Unit, LocalError] { Err(LocalError.Bad("kept")) }
proc caller() [io, error] -> Unit { fail(); print unreachable }
let result: Result[Unit] = try { caller() }
print ${result is Err(LocalError.Bad)}
