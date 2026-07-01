let files = ["alpha.txt", "beta.txt", "gamma.txt"]

for item in files |> enumerate() {
  let marker = if item.index == 0 { "keep" } else { "dup " }
  print f"  [${marker}] ${item.value}"
}

for item in ["line one", "line two"] |> enumerate() {
  print f"${item.index + 1}: ${item.value}"
}
