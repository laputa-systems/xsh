type Row = {index: Int, value: Int, label: Str}

cli main(input: Path) {
  let rows = json.decode(input.read_text()?)?.require(List[Row])?
  var total = 0
  var index = 0
  for row in rows {
    assert row.index == index
    assert row.label == f"row-{index}"
    assert row.value == index * 17 % rows.len()
    total += row.value
    index += 1
  }
  print f"{rows.len()} {total}"
}
