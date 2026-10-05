type Word = Union[Str, Path]

pure label(word: Word, words: List[Str]) -> Str {
  let argv: List[Word] = words # error: check.type-mismatch
  let text: Str = word # error: check.type-mismatch
  let shown = f"{word}" # error: check.union-narrow
  let name = word.name() # error: check.union-narrow
  if word is Int { # error: check.pattern-type
    return "never"
  }

  match word { # error: check.match-value-exhaustive
    file is Path => file.name()
  }
}

print ${label("a", [])}
