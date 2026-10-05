pure port(raw: Int) -> Result[Int] {
  # begin example
  guard raw > 0 else fail f"port {raw} is not positive"
  # end example
  Ok(raw)
}

print ${port(8080)?}
