error LocalError = Bad(message: Str)
pure nested() -> Result[Result[Int, LocalError]] {
  let data: Result[Int, LocalError] = Err(LocalError.Bad("inner"))
  Ok(data)
}
print ${nested()? is Err(LocalError.Bad)}
