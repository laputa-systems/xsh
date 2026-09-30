error LocalError = Bad(message: Str)
let result = try { Err(LocalError.Bad("unknown"))? }
let _ = result
