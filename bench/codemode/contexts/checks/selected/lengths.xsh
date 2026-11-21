let word = "héllo"
let xs = [10, 20, 30, 40]
var total = 0
for i in range(xs.len()) {
  total += xs[i]
}
print f"{word.byte_len()} {word.count_chars()} {xs.len()} {total} {json.encode(xs[1..3])?}"
