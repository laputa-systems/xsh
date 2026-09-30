pure nested_templates() -> Int {
  let values: List[List[Int]] = [[1], [2]]
  let selected = values.get(index: 1) ?? [9]
  let table: Map[List[Int]] = {left: [4]}
  let updated = table.set(value: selected, key: "right")
  let arguments = {key: "right"}
  let output = updated.get(...arguments) ?? [0]
  output[0] + updated.values().len() + values.push(item: [3]).len()
}

pure fresh_templates() -> Int {
  let values = map.empty().set(key: 1, value: [7])
  let labels = map.empty().set(value: "two", key: "second")
  (values.get(1) ?? [0])[0] + labels.len()
}

pure absent_templates() -> Bool {
  let absent: Map[Int]? = null
  let skipped = absent?.set(value: 1 / 0, key: "absent")
  skipped == null
}

pure materialized_templates() -> Int {
  "one\ntwo\n".lines().collect().len() + b"one\n".lines().collect().len() + range(3).collect().len()
}

pure discarded_templates() -> Unit {
  let _ = map.empty()
  map.empty()
}
