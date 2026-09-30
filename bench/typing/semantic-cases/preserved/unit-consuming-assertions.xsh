proc check() [error] -> Unit { false }
proc wrapped() [error] -> Result[Unit] { false }
let direct: Result[Unit] = try { check() }
let result: Result[Unit] = try { wrapped() }
let tail: Result[Unit] = try { false }
print ${direct is Err(_)} ${result is Err(_)} ${tail is Err(_)}
