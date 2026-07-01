let xs: List[Int] = [1, 2, 3, 4, 5]
let text = "alpha beta gamma"
var i = 0
var total = 0

while i < 5000 {
  total += xs.len()
  let item: Int = xs.get(2, 0)
  total += item

  if xs.contains(4) {
    total += 1
  }

  total += text.count_words()
  total += text.count_chars()

  if text.contains("beta") {
    total += 1
  }

  i += 1
}

print $total % 256
