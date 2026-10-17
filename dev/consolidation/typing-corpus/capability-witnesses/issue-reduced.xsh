type Row = {value: Int}
pure item(value: Int) -> Int { value }
pure append(items: List[Int], row) -> List[Int] {
  items.push(item(row.value))
}
let result: List[Int] = append([1], {value: 2})
