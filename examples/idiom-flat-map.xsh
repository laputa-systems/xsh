let lines = ["the quick", "brown fox"]

for word in lines
  |> flat-map { |line|
    line.words()
  } {
  print $word
}
