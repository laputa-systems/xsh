pure port(value: Int) -> Result[Int] {
  if value < 1 or value > 65535 {
    # begin example
    fail f"port {value} is out of range"
    # end example
  }

  Ok(value)
}

match port(70000) {
  Ok(value) => print $value
  Err(problem) => print $problem.message
}
