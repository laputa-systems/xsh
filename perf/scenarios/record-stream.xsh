type Event = {
  service: Str,
  level: Str,
  shard: Int,
  duration_ms: Int,
  retried: Bool,
  payload: Record,
}

pure event_for(index: Int) -> Event {
  let service = if index % 4 == 0 {
    "api"
  } else if index % 4 == 1 {
    "worker"
  } else if index % 4 == 2 {
    "db"
  } else {
    "cache"
  }
  let level = if index % 13 == 0 { "warn" } else if index % 17 == 0 { "error" } else { "info" }

  return {
    service,
    level,
    shard: index % 16,
    duration_ms: 5 + index % 251,
    retried: index % 19 == 0,
    payload: {
      id: index,
      route: f"/v1/resource/${index % 37}",
      cold: index % 23 == 0,
    },
  }
}

let rollup = range(0, 16000)
  |> map { |index|
    event_for(index)
  }
  |> where .level != "info" or .retried
  |> group-by f"${.service}:${.level}:${.shard}"
  |> map { |bucket|
    {
      key: bucket.key,
      count: bucket.items.len(),
      total_ms: bucket.items |> map .duration_ms |> sum,
      first_id: bucket.items[0].payload.id,
    }
  }
  |> sort-by .key
  |> collect()

var checksum = 0

for row in rollup {
  checksum += row.key.byte_len()
  checksum += row.count * 17
  checksum += row.total_ms % 4099
  checksum += row.first_id % 101
}

print ${checksum % 100000}
