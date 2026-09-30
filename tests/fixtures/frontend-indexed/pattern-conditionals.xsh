pure pattern_label(outcome: Result[Int]) -> Str {
  if let Ok(value) = outcome { f"${value}" } else { "missing" }
}

proc main() [error] {
  let outcome = Ok(7)
  if let Ok(value) = outcome { test.eq(value, 7)? }
  while let Ok(value) = outcome { test.eq(value, 7)?; break }
  let selected = if let Ok(value) = outcome { value + 1 } else { 0 }
  test.eq(selected, 8)?
  test.eq(pattern_label(outcome), "7")?
}
