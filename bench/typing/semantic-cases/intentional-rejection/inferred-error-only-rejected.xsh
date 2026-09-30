error LocalError = Bad(message: Str)
pure failed() { Err(LocalError.Bad("unknown success")) }
let _ = failed()
