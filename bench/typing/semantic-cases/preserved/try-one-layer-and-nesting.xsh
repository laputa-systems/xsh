error LocalError = Bad(message: Str)
let data: Result[Int, LocalError] = Err(LocalError.Bad("inner"))
let outer = try { data }
let nested = outer?
let captured: Result[Int, LocalError] = try { data? }
print ${nested is Err(LocalError.Bad)} ${captured is Err(LocalError.Bad)}
let no = try { false }?
print $no
