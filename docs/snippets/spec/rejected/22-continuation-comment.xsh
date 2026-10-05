proc unused() [process, error] {
  # The backslash must end its line: a comment after it is not a line break.
  run make \ # error: lex.unexpected-character
  run make Image
}
