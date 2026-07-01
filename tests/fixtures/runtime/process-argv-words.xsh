let words = process.argv_words("cmd 'two words' \"double quoted\" escaped\\ space")?
print words.len() words[0] words[1] words[2] words[3]

match process.argv_words("echo hi | wc") {
  Err(e) => print $e.message
}
