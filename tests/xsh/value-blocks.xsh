enum ValueChoice { ValueEmpty, ValueNumber(Int) }

pure value_label(code: Int) -> Str {
  match code {
    0 => "ok"
    _ => {
      let detail = f"exit $code"
      detail
    }
  }
}

pure value_choice(choose: Bool) -> Bool {
  if choose {
    let result = false
    result
  } else {
    true
  }
}

pure value_optional(name: Str?) -> Str {
  if name == null {
    "default"
  } else {
    name.trim()
  }
}

test test_value_blocks_and_tails [error] {
  let result = if true {
    let first = 40
    let second = 2
    first + second
  } else {
    0
  }
  test.eq(result, 42)?
  let matched = match result {
    42 => {
      let detail = "selected"
      detail
    }
    _ => "other"
  }
  test.eq(matched, "selected")?
  test.eq(value_label(0), "ok")?
  test.eq(value_label(3), "exit 3")?
  test.eq(value_choice(true), false)?
  test.eq(value_choice(false), true)?
  test.eq(value_optional("  ready  "), "ready")?
  test.eq(value_optional(null), "default")?
}

test test_value_match_preserves_record_literals [error] {
  let field = 9
  let empty = match 1 { _ => {} }
  let shorthand = match 1 { _ => {field} }
  let named = match 1 { _ => {field: 10} }
  let quoted = match 1 { _ => {"run": 11} }
  let keyword = match 1 { _ => {run: 12} }
  test.eq(empty, {})?
  test.eq(shorthand.field, 9)?
  test.eq(named.field, 10)?
  test.eq(quoted["run"], 11)?
  test.eq(keyword.run, 12)?
}

proc value_block_return_keeps_function_target() [] -> Int {
  let ignored = if true {
    return 7
  } else {
    4
  }
  ignored + 1
}

test test_value_blocks_preserve_lexical_control [error] {
  test.eq(value_block_return_keeps_function_target(), 7)?
  var visits = 0
  for number in [1, 2, 3] {
    let chosen = if number == 2 {
      continue
    } else {
      number
    }
    visits += chosen
  }
  test.eq(visits, 4)?
}

test test_bool_value_callbacks_and_retry [error] {
  let filtered = [1, 2, 3] |> where { |number|
    if number == 2 {
      false
    } else {
      true
    }
  } |> collect()
  test.eq(filtered, [1, 3])?
  let result = retry [] {
    let marker = false
    if marker {
      true
    } else {
      false
    }
  }?
  test.eq(result, false)?
  let wrapped = retry [] { Ok(false) }?
  test.eq(wrapped, false)?
}

test test_bool_statement_assertions [error] { |ctx|
  let failed = test.run_script(ctx, """proc assertion() {
  if true {
    false
  }
}
assertion()
""")?
  test.eq(failed.success, false)?
  test.contains(failed.stderr, "assertion-failed")?
  let passed = test.run_script(ctx, """proc assertion() {
  if true {
    true
  }
}
assertion()
""")?
  test.eq(passed.success, true)?
}

pure value_result_bool(choose: Bool) -> Result[Bool] {
  if choose {
    false
  } else {
    true
  }
}

test test_result_bool_tails_and_nested_predicates [error] {
  test.eq(value_result_bool(true)?, false)?
  let mapped = [1, 2] |> map { |number|
    match number {
      1 => {
        let selected = false
        selected
      }
      _ => true
    }
  }
  test.eq(mapped, [false, true])?
}

test test_value_branch_rejections_have_cli_witnesses [error] { |ctx|
  let inconsistent = test.run_script(ctx, "let bad = if true { 1 } else { \"wrong\" }\n")?
  test.eq(inconsistent.success, false)?
  test.contains(inconsistent.stderr, "check.type-mismatch")?
  let incomplete = test.run_script(ctx, "let bad = match 1 { 1 => 2 }\n")?
  test.eq(incomplete.success, false)?
  test.contains(incomplete.stderr, "check.match-value-exhaustive")?
  let guarded = test.run_script(ctx, "let bad = match 1 { _ if false => 2 }\n")?
  test.eq(guarded.success, false)?
  test.contains(guarded.stderr, "check.match-value-exhaustive")?
}

test test_value_blocks_evaluate_before_scope_cleanup [error] { |ctx|
  let output = test.run_script(ctx, """proc mark(message: Str) [] { print $message }
proc choose() [error] -> Int {
  let value = if true {
    defer mark("cleanup")
    mark("value")
    7
  } else { 0 }
  mark("after")
  value
}
print \${choose()}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "value\ncleanup\nafter\n7\n")?
}

test test_value_blocks_return_through_loop_and_retry [error] { |ctx|
  let output = test.run_script(ctx, """proc choose() [] -> Int {
  let ignored = loop {
    let selected = if true { return 7 } else { 0 }
    break selected
  }
  ignored
}
proc attempted() [] -> Int {
  let ignored = retry [] {
    let selected = if true { return 9 } else { 0 }
    selected
  }
  0
}
print \${choose()} \${attempted()}
""")?
  test.eq(output.success, true)?
  test.eq(output.stdout, "7 9\n")?
}

test test_value_callback_return_survives_retry_and_stream_cleanup [error] { |ctx|
  let output = test.run_script(ctx, """proc mark(message: Str) [] { print $message }
stream rows() [error] -> Stream[Int] {
  defer mark("source cleanup")
  yield 1
  yield 2
}
proc choose() [error] -> Int {
  retry [] {
    rows() |> each { |number|
      let selected = if true {
        defer mark("branch cleanup")
        return 7
      } else { 0 }
      let _ = selected
    }
  }
  0
}
print \${choose()}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "branch cleanup\nsource cleanup\n7\n")?
}

test test_value_callbacks_keep_enclosing_loop_targets [error] {
  var visits = 0
  for number in [1, 2, 3] {
    [number] |> each { |item|
      let selected = if item == 2 {
        continue
      } else {
        item
      }
      let _ = selected
    }
    visits += number
  }
  test.eq(visits, 4)?
  visits = 0
  for number in [1, 2, 3] {
    [number] |> each { |item|
      let selected = if item == 2 {
        break
      } else {
        item
      }
      let _ = selected
    }
    visits += number
  }
  test.eq(visits, 1)?
}

test test_value_callback_tail_precedes_cleanup [error] { |ctx|
  let output = test.run_script(ctx, """proc mark(message: Str) [] { print $message }
proc result(number: Int) [] -> Int { print "value"; number }
let mapped = [7] |> map { |number|
  defer mark("cleanup")
  result(number)
} |> collect
print \${mapped[0]}
""")?
  test.eq(output.success, true)?
  test.eq(output.stdout, "value\ncleanup\n7\n")?
}

test test_value_parallel_callback_keeps_lexical_return [error] { |ctx|
  let output = test.run_script(ctx, """proc choose() [] -> Int {
  let ignored = [1, 2] |> par-map(jobs: 2) { |number|
    let selected = if true { return 7 } else { number }
    selected
  } |> collect
  0
}
proc fused() [] -> Int {
  let ignored = [1, 2] |> par-map(jobs: 2) { |number|
    let selected = if true { return 9 } else { number }
    selected
  } |> reduce-by(sum: true) { |number| {key: "all", value: number} }
  0
}
proc serial() [] -> Int {
  let ignored = [1, 2] |> par-map(jobs: 1) { |number|
    let selected = if true { return 11 } else { number }
    selected
  } |> collect
  0
}
print \${choose()} \${fused()} \${serial()}
""")?
  test.eq(output.success, true)?
  test.eq(output.stdout, "7 9 11\n")?
}

test test_value_parallel_callback_failure_is_propagation [error] { |ctx|
  let output = test.run_script(ctx, """error WorkerError = failed(message: Str)
pure outcome() -> Result[Int] { Err(WorkerError.failed(message: "worker failed")) }
let values = [1, 2] |> par-map(jobs: 2) { |number|
  let value = outcome()?
  value
} |> collect
print "unreachable"
""")?
  test.eq(output.status, 3)?
  test.contains(output.stderr, "WorkerError.failed")?
  test.eq(output.stderr.contains("return-outside-function"), false)?
  test.eq(output.stdout, "")?
}

test test_value_fold_and_key_callbacks_have_ordinary_scopes [error] {
  let total = [1, 2] |> fold(0) { |acc, number|
    let added = acc + number
    match number {
      1 => { let result = added; result }
      _ => { let result = added; result }
    }
  }
  test.eq(total, 3)?
  let grouped = [1, 2] |> reduce-by(sum: true) { |number|
    let key = "all"
    if number == 1 { {key, value: number} } else { {key, value: number} }
  }
  test.eq((grouped.get("all") ?? 0), 3)?
  let sorted = [2, 1] |> sort-by { |number| let key = number; key } |> collect
  test.eq(sorted, [1, 2])?
}

test test_value_branches_preserve_tags_dotted_pipelines_and_tee [error] {
  let options = {items: [1, 2]}
  let selected = if false { options.items } else { options.items |> drop(1) }
  test.eq(selected, [2])?
  let chosen = if true { ValueEmpty } else { ValueNumber(1) }
  test.eq(chosen, ValueEmpty)?
  let rows = [1] |> tee { |number|
    if false { print $number }
  } |> collect
  test.eq(rows, [1])?
}

pure value_block_subtract(depth: Int) -> Int {
  if depth > 0 {
    let value = depth
    value - 1
  } else {
    0
  }
}

test test_value_branch_identifier_subtraction_is_a_value [error] {
  test.eq(value_block_subtract(3), 2)?
  test.eq(value_block_subtract(0), 0)?
}
