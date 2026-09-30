pure updated() -> Int {
  var rows = [{count: 1, values: [2, 3]}]
  let earlier = rows
  rows[0].count += 4
  rows[0].values[1] = 8
  return rows[0].count + rows[0].values[1] + earlier[0].values[1]
}
