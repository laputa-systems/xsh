proc main() [error] -> Result[Unit] {
  let value = Ok({child: {answer: 42}})
  let matched = value is Ok({child: {answer: 42}})
  test.eq(matched, true)?
}
