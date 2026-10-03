enum ValueChoice { ValueEmpty, ValueNumber(Int) }

pure value_label(code: Int) -> Str {
  match code {
    0 => "ok"
    _ => {
      let detail = f"exit ${code}"
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

test test_value_blocks_and_tails {
  let result = if true {
    let first = 40
    let second = 2
    first + second
  } else {
    0
  }
  assert result == 42
  let matched = match result {
    42 => {
      let detail = "selected"
      detail
    },
    _ => "other",
  }
  assert matched == "selected"
  assert value_label(0) == "ok"
  assert value_label(3) == "exit 3"
  assert value_choice(true) == false
  assert value_choice(false) == true
  assert value_optional("  ready  ") == "ready"
  assert value_optional(null) == "default"
}

test test_value_match_preserves_record_literals { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc witness() [error] {
  let field = 9
  let empty = match 1 { _ => {} }
  let shorthand = match 1 { _ => {field} }
  let named = match 1 { _ => {field: 10} }
  let quoted = match 1 { _ => {"run": 11} }
  let keyword = match 1 { _ => {run: 12} }
  assert (empty) == ({})
  assert (shorthand.field) == (9)
  assert (named.field) == (10)
  assert (quoted["run"]) == (11)
  assert (keyword.run) == (12)
}
witness()
""",
  )?
  let {success: assertion_condition, stderr: assertion_message, ..} = output
  assert assertion_condition, assertion_message
  assert output.stdout == ""
}

proc value_block_return_keeps_function_target() [] -> Int {
  let ignored = if true {
    return 7
  } else {
    4
  }
  ignored + 1
}

test test_value_blocks_preserve_lexical_control {
  assert value_block_return_keeps_function_target() == 7
  var visits = 0
  for number in [1, 2, 3] {
    let chosen = if number == 2 {
      continue
    } else {
      number
    }
    visits += chosen
  }

  assert visits == 4
}

test test_bool_value_callbacks_and_retry {
  let filtered = [1, 2, 3]
    |> where { |number|
      if number == 2 {
        false
      } else {
        true
      }
    }
    |> collect()
  assert filtered == [1, 3]
  let result = retry [] {
    let marker = false
    if marker {
      true
    } else {
      false
    }
  }?
  assert result == false
  let wrapped = retry [] {
    Ok(false)
  }?
  assert wrapped == false
}

test test_bool_statement_branch_tails_are_rejected { |ctx|
  let rejected = test.run_script(
    ctx,
    """proc assertion() {
  if true {
    true
  }
}
assertion()
""",
  )?
  assert rejected.status == 2
  assert "check.bool-statement" in rejected.stderr, rejected.stderr
  let failed = test.run_script(
    ctx,
    """proc assertion() {
  if true {
    assert false
  }
}
assertion()
""",
  )?
  assert failed.success == false
  assert "AssertionError" in failed.stderr
}

pure value_result_bool(choose: Bool) -> Result[Bool] {
  if choose {
    false
  } else {
    true
  }
}

test test_result_bool_tails_and_nested_predicates {
  assert value_result_bool(true)? == false
  let mapped = [1, 2]
    |> map { |number|
      match number {
        1 => {
          let selected = false
          selected
        }
        _ => true
      }
    }
  assert mapped == [false, true]
}

test test_value_branch_rejections_have_cli_witnesses { |ctx|
  let inconsistent = test.run_script(
    ctx,
    """let bad = if true { 1 } else { "wrong" }
""",
  )?
  assert inconsistent.success == false
  assert "check.type-mismatch" in inconsistent.stderr
  let incomplete = test.run_script(
    ctx,
    """let bad = match 1 { 1 => 2 }
""",
  )?
  assert incomplete.success == false
  assert "check.match-value-exhaustive" in incomplete.stderr
  let guarded = test.run_script(
    ctx,
    """let bad = match 1 { _ if false => 2 }
""",
  )?
  assert guarded.success == false
  assert "check.match-value-exhaustive" in guarded.stderr
}

test test_value_tails_reject_missing_else_and_incomplete_match { |ctx|
  for fixture in [
    {
      source: """let bad = if true { 1 }
""",
      diagnostic: "parse.if-expression-else",
    },
    {
      source: """pure pick(flag: Bool) -> Int {
  if flag {
    1
  }
}
""",
      diagnostic: "check.if-value-else",
    },
    {
      source: """pure pick(code: Int) -> Str {
  match code {
    0 => "ok"
  }
}
""",
      diagnostic: "check.match-value-exhaustive",
    },
    {
      source: """pure pick(flag: Bool) -> Int {
  if flag {
    1
  } else {
    "wrong"
  }
}
""",
      diagnostic: "check.type-mismatch",
    },
  ] {
    let output = test.run_script(ctx, fixture.source)?
    assert ! output.success, output.stderr
    assert fixture.diagnostic in output.stderr
  }
}

test test_value_top_level_control_flow_keeps_statement_semantics { |ctx|
  let branch = test.run_script(
    ctx,
    """if true {
  3
} else {
  4
}
""",
  )?
  assert branch.success, branch.stderr
  let matched = test.run_script(
    ctx,
    """match 1 {
  1 => 5
  _ => 6
}
""",
  )?
  assert matched.success, matched.stderr
  let final = test.run_script(
    ctx,
    """3
""",
  )?
  assert final.status == 3
}

test test_value_blocks_evaluate_before_scope_cleanup { |ctx|
  let output = test.run_script(
    ctx,
    """proc mark(message: Str) [] { print $message }
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
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """value
cleanup
after
7
"""
}

test test_value_blocks_return_through_loop_and_retry { |ctx|
  let output = test.run_script(
    ctx,
    """proc choose() [] -> Int {
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
""",
  )?
  assert output.success == true
  assert output.stdout == """7 9
"""
}

test test_value_callback_return_survives_retry_and_stream_cleanup { |ctx|
  let output = test.run_script(
    ctx,
    """proc mark(message: Str) [] { print $message }
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
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """branch cleanup
source cleanup
7
"""
}

test test_value_callbacks_keep_enclosing_loop_targets {
  var visits = 0
  for number in [1, 2, 3] {
    [number]
      |> each { |item|
        let selected = if item == 2 {
          continue
        } else {
          item
        }
        let _ = selected
      }
    visits += number
  }

  assert visits == 4
  visits = 0
  for number in [1, 2, 3] {
    [number]
      |> each { |item|
        let selected = if item == 2 {
          break
        } else {
          item
        }
        let _ = selected
      }
    visits += number
  }

  assert visits == 1
}

test test_value_callback_tail_precedes_cleanup { |ctx|
  let output = test.run_script(
    ctx,
    """proc mark(message: Str) [] { print $message }
proc result(number: Int) [] -> Int { print "value"; number }
let mapped = [7] |> map { |number|
  defer mark("cleanup")
  result(number)
} |> collect
print \${mapped[0]}
""",
  )?
  assert output.success == true
  assert output.stdout == """value
cleanup
7
"""
}

test test_value_parallel_callback_keeps_lexical_return { |ctx|
  let output = test.run_script(
    ctx,
    """proc choose() [] -> Int {
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
""",
  )?
  assert output.success == true
  assert output.stdout == """7 9 11
"""
}

test test_value_parallel_callback_failure_is_propagation { |ctx|
  let output = test.run_script(
    ctx,
    """error WorkerError = failed(message: Str)
pure outcome() -> Result[Int] { Err(WorkerError.failed(message: "worker failed")) }
let values = [1, 2] |> par-map(jobs: 2) { |number|
  let value = outcome()?
  value
} |> collect
print "unreachable"
""",
  )?
  assert output.status == 3
  assert "WorkerError.failed" in output.stderr
  assert ("return-outside-function" in output.stderr) == false
  assert output.stdout == ""
}

test test_value_fold_and_key_callbacks_have_ordinary_scopes {
  let total = [1, 2]
    |> fold(0) { |acc, number|
      let added = acc + number
      match number {
        1 | _ => {
          let result = added
          result
        }
      }
    }
  assert total == 3
  let grouped = [1, 2]
    |> reduce-by(sum: true) { |number|
      let key = "all"
      if number == 1 {
        {key, value: number}
      } else {
        {key, value: number}
      }
    }
  assert (grouped.get("all") ?? 0) == 3
  let sorted = [2, 1]
    |> sort-by { |number|
      let key = number
      key
    }
    |> collect()
  assert sorted == [1, 2]
}

test test_value_branches_preserve_tags_dotted_pipelines_and_tee {
  let options = {items: [1, 2]}
  let selected = if false { options.items } else { options.items |> drop(1) }
  assert selected == [2]
  let chosen: ValueChoice = if true { ValueEmpty } else { ValueNumber(1) }
  assert chosen == ValueEmpty
  let rows = [1]
    |> tee { |number|
      if false {
        print $number
      }
    }
    |> collect()
  assert rows == [1]
}

pure value_block_subtract(depth: Int) -> Int {
  if depth > 0 {
    let value = depth
    value - 1
  } else {
    0
  }
}

test test_value_branch_identifier_subtraction_is_a_value {
  assert value_block_subtract(3) == 2
  assert value_block_subtract(0) == 0
}
