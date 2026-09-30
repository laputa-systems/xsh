error LocalError = Bad(message: Str)
pure unit() -> Unit {}
let _ = false
let failed: Result[Int, LocalError] = Err(LocalError.Bad("discarded"))
let _ = failed
let _ = unit()
print done
