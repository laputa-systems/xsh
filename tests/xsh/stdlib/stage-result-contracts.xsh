test test_predicate_stages_require_direct_bool_callbacks [error] { |ctx|
  for stage in ["where", "any", "all"] {
    let rejected = test.run_script(ctx, "let value = [1, 2] |> " + stage + " { |item| Ok(item > 0) }\n")?
    test.ok(!rejected.success, stage)?
    test.contains(rejected.stderr, "check.type-mismatch", rejected.stderr)?
    let accepted = test.run_script(ctx, "let value = [1, 2] |> " + stage + " { |item| Ok(item > 0)? }\nprint \"accepted\"\n")?
    test.ok(accepted.success, accepted.stderr)?
    test.eq(accepted.stdout, "accepted\n")?
  }
}

test test_predicate_callable_requires_direct_bool_return [error] { |ctx|
  let output = test.run_script(ctx, r"""pure keep(item: Int) -> Result[Bool] { Ok(item > 0) }
let values = [1, 2] |> where(keep)
""")?
  test.ok(!output.success, output.stderr)?
  test.contains(output.stderr, "check.type-mismatch", output.stderr)?
}

test test_count_and_sort_by_require_direct_supported_keys [error] { |ctx|
  let counted = test.run_script(ctx, r"""let counts = [1, 2] |> count { |item| Ok(item) }
""")?
  test.ok(!counted.success, counted.stderr)?
  test.contains(counted.stderr, "check.stream-count-key", counted.stderr)?
  let sorted = test.run_script(ctx, r"""let values = [2, 1] |> sort-by { |item| Ok(item) }
""")?
  test.ok(!sorted.success, sorted.stderr)?
  test.contains(sorted.stderr, "check.stream-sort", sorted.stderr)?
  let accepted = test.run_script(ctx, r"""let counts = [1, 2, 1] |> count { |item| Ok(item)? }
let values = [2, 1] |> sort-by { |item| Ok(item)? }
print ${counts.get("1")?} ${values[0]}
""")?
  test.ok(accepted.success, accepted.stderr)?
  test.eq(accepted.stdout, "2 1\n")?
}

test test_map_and_par_map_preserve_complete_result_values [error] { |ctx|
  for stage in ["map", "par-map(jobs: 1)", "par-map(jobs: 2)"] {
    let source = r"""error ItemError = Stop(item: Int)
pure classify(item: Int) -> Result[Int, ItemError] {
  if item == 2 { Err(ItemError.Stop(item)) } else { item }
}
let values = [1, 2, 3] |> """ + stage + r""" { |item| classify(item) }
print ${values.len()} ${values[0] is Ok(1)} ${values[1] is Err(ItemError.Stop {item: 2})} ${values[2] is Ok(3)}
"""
    let output = test.run_script(ctx, source)?
    test.ok(output.success, output.stderr)?
    test.eq(output.stdout, "3 true true true\n")?
  }
}

test test_group_by_and_unique_by_preserve_result_keys_as_data [error] { |ctx|
  let output = test.run_script(ctx, r"""error KeyError = Missing(code: Int)
pure key(item: Int) -> Result[Int, KeyError] {
  if item == 2 { Err(KeyError.Missing(code: 7)) } else { item % 2 }
}
let groups = [1, 2, 3] |> group-by { |item| key(item) }
let unique = [1, 2, 3] |> unique-by { |item| key(item) }
print ${groups[0].key is Ok(1)} ${groups[0].items.len()} ${groups[1].key is Err(KeyError.Missing {code: 7})} ${unique.len()}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "true 2 true 2\n")?
}

test test_flat_map_preserves_existing_result_collection_boundary [error] { |ctx|
  let accepted = test.run_script(ctx, r"""let values = [1, 2] |> flat-map { |item| Ok([item, item]) }
print ${values.len()} ${values[0]} ${values[3]}
""")?
  test.ok(accepted.success, accepted.stderr)?
  test.eq(accepted.stdout, "4 1 2\n")?
  let failed = test.run_script(ctx, r"""error ExpansionError = Stop(message: Str)
pure expand(item: Int) -> Result[List[Int], ExpansionError] {
  if item == 2 { Err(ExpansionError.Stop(message: "stop expanding")) } else { [item] }
}
let values = [1, 2, 3] |> flat-map { |item| expand(item) }
""")?
  test.ok(!failed.success, failed.stdout)?
  test.contains(failed.stderr, "stop expanding", failed.stderr)?
}

test test_par_map_explicit_callback_propagation_retains_nominal_error [error] { |ctx|
  for jobs in ["1", "2"] {
    let source = r"""error ItemError = Stop(item: Int)
pure classify(item: Int) -> Result[Int, ItemError] {
  if item == 2 { Err(ItemError.Stop(item)) } else { item }
}
let outcome = try { [1, 2, 3] |> par-map(jobs: """ + jobs + r""") { |item| classify(item)? } }
print ${outcome is Err(ItemError.Stop {item: 2})}
"""
    let output = test.run_script(ctx, source)?
    test.ok(output.success, output.stderr)?
    test.eq(output.stdout, "true\n")?
  }
}

test test_par_map_result_data_retains_materialization_and_cleanup_order [error] { |ctx|
  let output = test.run_script(ctx, r"""error ItemError = Stop(item: Int)
stream numbers() [io] -> Stream[Int] {
  defer { print "source closed" }
  for item in [1, 2, 3] { print f"pull ${item}"; yield item }
}
proc classify(item: Int) [io] -> Result[Int, ItemError] {
  defer { print f"callback closed ${item}" }
  print f"callback ${item}"
  if item == 2 { Err(ItemError.Stop(item)) } else { item }
}
let values = numbers() |> par-map(jobs: 1) { |item| classify(item) }
print ${values[1] is Err(ItemError.Stop {item: 2})}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "pull 1\npull 2\npull 3\nsource closed\ncallback 1\ncallback closed 1\ncallback 2\ncallback closed 2\ncallback 3\ncallback closed 3\ntrue\n")?
}

test test_par_map_runtime_faults_remain_outside_local_capture [error] { |ctx|
  for jobs in ["1", "2"] {
    let source = "let captured = try { [1, 2, 3] |> par-map(jobs: " + jobs + ") { |item| 10 / (item - 2) } }\nprint \"captured\"\n"
    let output = test.run_script(ctx, source)?
    test.ok(!output.success, output.stdout)?
    test.eq(output.stdout, "")?
  }
}
