let entries = [{word: "apple"}, {word: "banana"}, {word: "apple"}, {word: "cherry"}, {word: "banana"}, {word: "apple"}]

let groups = entries
  |> group-by .word
  |> sort-by .key

for g in groups {
  print f"${g.key} ${g.items.len()}"
}

let total = entries
  |> map .word.count_chars()
  |> fold(0) { |acc|
    acc + .
  }

print $total
var counts: Map[Int] = {}

for entry in entries {
  counts[entry.word] = counts.get(entry.word, 0) + 1
}

let apple_count = counts.get("apple", 0)
let banana_count = counts.get("banana", 0)
print f"apple=${apple_count} banana=${banana_count}"

var buckets: Map[List[Str]] = {}

for entry in entries {
  buckets = buckets.push(entry.word, entry.word.upper())
}

let apple_bucket = buckets.get("apple")?
print apple_bucket[1]
