pure compound_append(items, value) {
  var output = items
  output += [value]
  output
}

pure compound_append_issue(base, value) {
  var issues = base.issues
  issues += [value]
  issues
}

test inferred_compound_append_preserves_item_and_row_relationships {
  let numbers: List[Int] = compound_append([1], 2)
  let words: List[Str] = compound_append(["a"], "b")
  let row_numbers: List[Int] = compound_append_issue({issues: [1], label: "numbers"}, 2)
  let row_words: List[Str] = compound_append_issue({issues: ["a"], label: 7}, "b")
  assert numbers == [1, 2]
  assert words == ["a", "b"]
  assert row_numbers == numbers
  assert row_words == words
}
