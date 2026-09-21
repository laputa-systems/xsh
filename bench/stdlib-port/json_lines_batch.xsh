proc main() [io, error] {
  # Ten thousand small records encoded as JSON Lines in one bounded call.
  var records: List[Any] = []
  var index = 0
  while index < 10000 {
    records = records.push({id: index, name: f"row${index}", tags: ["a", "b"]})
    index = index + 1
  }
  let text = json.encode_lines(records)?
  print f"${text.byte_len()}"
}
