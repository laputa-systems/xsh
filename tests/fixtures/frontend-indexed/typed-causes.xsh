error OuterCauseError = Failed(message: Str)
error InnerCauseError = Failed(message: Str)

pure translated_cause() -> Result[Int, OuterCauseError] {
  let unrelated: Any = 9
  Err(OuterCauseError.Failed(message: "outer"), cause: InnerCauseError.Failed(message: "inner"))
}
