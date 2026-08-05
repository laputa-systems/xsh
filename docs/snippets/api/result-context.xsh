pure read_config() -> Result[Str] {
  Ok("ready")
}

proc main() [error] {
  let value = read_config().context("config")?
  print $value
}
