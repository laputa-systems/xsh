use qualified_helper

pure option_message(option: Str, index: Int) -> Str {
  if index % 3 == 0 {
    return qualified_helper.invalid_option(option)
  }

  return qualified_helper.missing_option_value(option)
}

pure score(option: Str, index: Int) -> Int {
  let message = option_message(option, index).lower()
  let words = message.split(" ")
  return message.count_chars() + words.len() + words.get(words.len() - 1, "").count_chars()
}

var i = 0
var total = 0

while i < 5000 {
  let option = if i % 2 == 0 { "root" } else { "shell" }
  total += score(option, i)
  i += 1
}

print $total % 256
