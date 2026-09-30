pure discard(value) { let _ = value; 1 }
let result: Result[Int] = Ok(7)
print ${discard(false)} ${discard(result)} ${discard(null)}
