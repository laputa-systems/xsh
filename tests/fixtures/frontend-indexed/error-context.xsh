proc contextual_value() [] -> Int { ctx "value" { 7 } }
proc contextual_failure() [error] -> Result[Unit] {
  ctx "outer" { ctx "inner" { error.fail("base")? } }
}
