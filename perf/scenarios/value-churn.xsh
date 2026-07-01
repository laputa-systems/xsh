type Row = {id: Int, name: Str, group: Str, weight: Int, enabled: Bool, tags: List[Str]}
type Scored = {id: Int, group: Str, score: Int, label: Str}

pure row_for(id: Int) -> Row {
  let category = if id % 5 == 0 {
    "core"
  } else if id % 5 == 1 {
    "net"
  } else if id % 5 == 2 {
    "fs"
  } else if id % 5 == 3 {
    "proc"
  } else {
    "ui"
  }

  return {
    id,
    name: f"pkg-${id % 97}",
    group: category,
    weight: id % 113,
    enabled: id % 7 != 0,
    tags: [category, f"bucket-${id % 11}", if id % 2 == 0 { "even" } else { "odd" }],
  }
}

pure score(row: Row) -> Scored {
  return {
    id: row.id,
    group: row.group,
    score: row.weight * 3 + row.tags.len() + if row.enabled { 17 } else { 0 },
    label: f"${row.name}:${row.group}:${row.tags[1]}",
  }
}

let rows: List[Row] = range(0, 9000)
  |> map { |id|
    row_for(id)
  }
  |> collect()

let selected: List[Scored] = rows
  |> where .enabled or .weight > 90
  |> map { |row|
    score(row)
  }
  |> sort-by .score
  |> collect()

var totals: Map[Int] = {}
var checksum = 0

for item in selected {
  totals[item.group] = totals.get(item.group, 0) + item.score
  checksum += item.id % 31
  checksum += item.label.byte_len()
}

let grouped = selected
  |> group-by .group
  |> map { |bucket|
    {key: bucket.key, count: bucket.items.len(), total: totals.get(bucket.key, 0)}
  }
  |> sort-by .key
  |> collect()

for row in grouped {
  checksum += row.count * 13 + row.total % 997
}

print ${checksum % 100000}
