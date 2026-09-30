type UnsignedCount = UInt
type UnsignedRow = {count: UnsignedCount}
pure scalar_failure() -> Int {
  var value: UInt = 1
  value = -1
  return value
}
pure compound_failure() -> Int {
  var value: UInt = 1
  value -= 2
  return value
}
pure record_failure() -> Int {
  var value: UnsignedRow = {count: 1}
  value.count = -1
  return value.count
}
pure list_failure() -> Int {
  var value: List[UInt] = [1]
  value[0] -= 2
  return value[0]
}
pure map_failure() -> Int {
  var value: Map[Str, UInt] = {a: 1}
  value["a"] = -1
  return value.get("a") ?? 0
}
pure append_failure() -> Int {
  var value: List[UInt] = [1]
  value += [-1]
  return value[0]
}
pure valid_updates() -> Int {
  var value: UInt = 3
  value -= 3
  var rows: List[UnsignedRow] = [{count: 2}]
  let alias = rows
  rows[0].count *= 3
  return value + rows[0].count + alias[0].count
}

pure accept(n: UInt) -> Int { return n }
pure argument_failure(n: Int) -> Int { return accept(n) }
pure negative_return(n: Int) -> UInt { return n }
pure return_failure(n: Int) -> Int { return negative_return(n) }
pure negative_tail() -> UInt { -1 }
pure tail_failure() -> Int { return negative_tail() }
pure defaulted(n: UInt = -1) -> Int { return n }
pure default_failure() -> Int { return defaulted() }
pure list_defaulted(n: List[UInt] = [-1]) -> Int { return n[0] }
pure list_default_failure() -> Int { return list_defaulted() }
pure list_return(n: Int) -> List[UInt] { return [n] }
pure list_return_failure(n: Int) -> Int { return list_return(n)[0] }
pure map_return(n: Int) -> Map[Str, UInt] { return {a: n} }
pure map_return_failure(n: Int) -> Int { return map_return(n).get("a") ?? 0 }
pure record_return(n: Int) -> UnsignedRow { return {count: n} }
pure record_return_failure(n: Int) -> Int { return record_return(n).count }

pure accept_list(n: List[UInt]) -> Int { return n[0] }
pure list_argument_failure(n: Int) -> Int { return accept_list([n]) }
pure accept_map(n: Map[Str, UInt]) -> Int { return n.get("a") ?? 0 }
pure map_argument_failure(n: Int) -> Int { return accept_map({a: n}) }
pure accept_record(n: UnsignedRow) -> Int { return n.count }
pure record_argument_failure(n: Int) -> Int { return accept_record({count: n}) }
pure map_defaulted(n: Map[Str, UInt] = {a: -1}) -> Int { return n.get("a") ?? 0 }
pure map_default_failure() -> Int { return map_defaulted() }
pure record_defaulted(n: UnsignedRow = {count: -1}) -> Int { return n.count }
pure record_default_failure() -> Int { return record_defaulted() }
pure result_return(n: Int) -> Result[UInt] { return Ok(n) }
pure result_return_failure(n: Int) -> Int { return result_return(n) ?? 0 }
stream raw_rows(n: Int) [] -> Stream[Int] { yield n }
stream checked_rows(n: Int) [] -> Stream[UInt] { yield @raw_rows(n) }
stream nested_rows(n: Int) [] -> Stream[List[UInt]] { yield @[[n]] }
proc producer_failure(n: Int) [error] -> Int {
  for value in checked_rows(n) {}
  return 0
}
proc nested_producer_failure(n: Int) [error] -> Int {
  for value in nested_rows(n) {}
  return 0
}

enum Count { Counted(UInt), Empty }
error CountError = Bad(count: List[UInt])
pure tag_failure(n: Int) -> Int {
  let value = Counted(n)
  return 0
}
pure error_failure(n: Int) -> Int {
  let value = CountError.Bad(count: [n])
  return 0
}
pure method_list_failure(n: Int) -> Int {
  let values: List[UInt] = [1]
  let changed = values.push(n)
  return changed[0]
}
pure method_map_failure(n: Int) -> Int {
  let values: Map[Str, UInt] = {a: 1}
  let changed = values.set("b", n)
  return changed.get("b") ?? 0
}
pure method_fallback_failure(n: Int) -> Int {
  let values: List[UInt] = [1]
  let rejected = values.get(2) ?? n
  return rejected
}
pure method_map_push_failure(n: Int) -> Int {
  let values: Map[Str, List[UInt]] = {a: [1]}
  let changed = values.push("a", n)
  return (changed.get("a") ?? [0])[0]
}
pure inferred_if_failure(n: Int) -> Int {
  let good: UInt = 1
  let rejected = if false { good } else { n }
  return rejected
}
pure inferred_match_failure(n: Int) -> Int {
  let good: UInt = 1
  let rejected = match false { true => good, false => n }
  return rejected
}
pure builtin_creation_failure(n: Int) -> Int {
  let good: UInt = 1
  return (([good, n] |> min()) ?? 0)
}
pure branch_creation_failure(n: Int) -> Str {
  let good: UInt = 1
  return f"${if false { good } else { n }}"
}
