proc main() [io, error] {
  # Nested get/set/remove over records, maps, and lists, including a deep path.
  var value: Any = {a: {b: [{c: 1}, {c: 2}]}, list: [1, 2, 3], flat: 0}
  var sink = 0
  var round = 0
  while round < 400 {
    # The read result is an `Any` payload: it is bound and counted through a
    # second call that accepts `Any`, because neither an `Ok`/`Err` arm nor a
    # `null` comparison can be checked against `Any`.
    let found = json.get(value, ["a", "b", 1, "c"])?
    let updated = json.set(value, ["list", 0], round)?
    let removed = json.remove(value, ["a", "b", 0])?
    # An `Any` payload supports type-name tests but not method calls, so each
    # result is consumed by matching the shapes this workload produces.
    match found {
      item is Int => { sink = sink + item }
      items is List[Any] => { sink = sink + items.len() }
      fields is Map[Any] => { sink = sink + fields.len() }
      _ => { sink = sink + 1 }
    }
    match removed {
      item is Int => { sink = sink + item }
      items is List[Any] => { sink = sink + items.len() }
      fields is Map[Any] => { sink = sink + fields.len() }
      _ => { sink = sink + 1 }
    }
    value = updated
    round = round + 1
  }
  print f"${sink}"
}
