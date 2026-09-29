let root_words = ["a", "b"]
let root_separator = ","

pure joined_words(words: List[Str], separator: Str) -> Str {
  let answer = words.join(separator: separator)
  return answer
}

print (joined_words(words: root_words, separator: root_separator))
