proc empty() [] {}
let value: Result[Unit] = empty()
print ${value is Ok(_)}
