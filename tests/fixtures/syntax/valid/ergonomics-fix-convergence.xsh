pure joined_words(words: List[Str], separator: Str) -> Str {
  let answer = words.join(separator: separator)
  return answer
}

let words = ["a", "b"]
let separator = ","
print (joined_words(words: words, separator: separator))
