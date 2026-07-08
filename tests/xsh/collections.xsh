type Entry = {name: Str, score: Int}

proc test_list_comprehension_basic_transform() [error] {
  let nums = [1, 2, 3]
  let doubled = [x * 2 for x in nums]
  test.eq(doubled, [2, 4, 6])?
}

proc test_list_comprehension_with_guard_filters_elements() [error] {
  let nums = [1, 2, 3, 4, 5]
  let evens = [x for x in nums if x % 2 == 0]
  test.eq(evens, [2, 4])?
}

proc test_list_comprehension_guard_can_produce_empty_list() [error] {
  let nums = [1, 3, 5]
  let evens = [x for x in nums if x % 2 == 0]
  test.eq(evens |> count(), 0)?
}

proc test_list_comprehension_with_record_destructuring() [error] {
  let entries: List[Entry] = [{name: "alice", score: 90}, {name: "bob", score: 55}, {name: "carol", score: 80}]
  let passing = [name for {name, score} in entries if score >= 60]
  test.eq(passing, ["alice", "carol"])?
}
