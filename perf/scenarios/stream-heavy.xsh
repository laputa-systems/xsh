pure expand(seed: Int) -> List[Int] {
  return [seed, seed + 1, seed * 2, seed * 2 + 1]
}

pure adjust(value: Int) -> Int {
  if value % 11 == 0 {
    return value / 11
  }

  if value % 5 == 0 {
    return value * 3
  }

  return value + 7
}

let rows = range(0, 6000)
  |> flat-map { |seed|
    expand(seed)
  }
  |> where . % 3 != 1
  |> map { |value|
    {value, adjusted: adjust(value), group: f"g${value % 23}"}
  }
  |> where .adjusted % 7 != 0
  |> enumerate()
  |> map { |item|
    {
      index: item.index,
      group: item.value.group,
      score: item.value.adjusted + item.index % 19,
    }
  }
  |> group-by .group
  |> map { |bucket|
    {
      key: bucket.key,
      count: bucket.items.len(),
      total: bucket.items |> map .score |> sum,
      first_index: bucket.items[0].index,
    }
  }
  |> sort-by .key
  |> collect()

var checksum = 0

for row in rows {
  checksum += row.key.byte_len()
  checksum += row.count * 31
  checksum += row.total % 8191
  checksum += row.first_index % 127
}

print ${checksum % 100000}
