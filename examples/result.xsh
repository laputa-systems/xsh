pure label(value: Str) -> Result[Str] {
  value
}

let value = label("ok")?
print $value
