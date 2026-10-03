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

test test_value_blocks_and_tails {
  let result = if true {
    let first = 40
    let second = 2
    first + second
  } else {
    0
  }
  (result) == (42)
  let matched = match result {
    42 => {
      let detail = "selected"
      detail
    }
    _ => "other"
  }
  (matched) == ("selected")
  (value_label(0)) == ("ok")
  (value_label(3)) == ("exit 3")
  (value_choice(true)) == (false)
  (value_choice(false)) == (true)
  (value_optional("  ready  ")) == ("ready")
  (value_optional(null)) == ("default")
}

test test_value_match_preserves_record_literals { |ctx|
  let output = test.run_script(ctx, r"""
proc witness() [error] {
  let field = 9
  let empty = match 1 { _ => {} }
  let shorthand = match 1 { _ => {field} }
  let named = match 1 { _ => {field: 10} }
  let quoted = match 1 { _ => {"run": 11} }
  let keyword = match 1 { _ => {run: 12} }
  (empty) == ({})
  (shorthand.field) == (9)
  (named.field) == (10)
  (quoted["run"]) == (11)
  (keyword.run) == (12)
}
witness()
""")?
  let {success: assertion_condition, stderr: assertion_message, ..} = output
  assert assertion_condition, assertion_message
  output.stdout == ""
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
  (value_block_return_keeps_function_target()) == (7)
  var visits = 0
  for number in [1, 2, 3] {
    let chosen = if number == 2 {
      continue
    } else {
      number
    }
    visits += chosen
  }
  (visits) == (4)
}

test test_bool_value_callbacks_and_retry {
  let filtered = [1, 2, 3] |> where { |number|
    if number == 2 {
      false
    } else {
      true
    }
  } |> collect()
  (filtered) == ([1, 3])
  let result = retry [] {
    let marker = false
    if marker {
      true
    } else {
      false
    }
  }?
  (result) == (false)
  let wrapped = retry [] { Ok(false) }?
  (wrapped) == (false)
}

test test_bool_statement_assertions { |ctx|
  let failed = test.run_script(ctx, """proc assertion() {
  if true {
    false
  }
}
assertion()
""")?
  (failed.success) == (false)
  ("AssertionError" in failed.stderr)
  let passed = test.run_script(ctx, """proc assertion() {
  if true {
    true
  }
}
assertion()
""")?
  (passed.success) == (true)
}

pure value_result_bool(choose: Bool) -> Result[Bool] {
  if choose {
    false
  } else {
    true
  }
}

test test_result_bool_tails_and_nested_predicates {
  (value_result_bool(true)?) == (false)
  let mapped = [1, 2] |> map { |number|
    match number {
      1 => {
        let selected = false
        selected
      }
      _ => true
    }
  }
  (mapped) == ([false, true])
}

test test_value_branch_rejections_have_cli_witnesses { |ctx|
  let inconsistent = test.run_script(ctx, "let bad = if true { 1 } else { \"wrong\" }\n")?
  (inconsistent.success) == (false)
  ("check.type-mismatch" in inconsistent.stderr)
  let incomplete = test.run_script(ctx, "let bad = match 1 { 1 => 2 }\n")?
  (incomplete.success) == (false)
  ("check.match-value-exhaustive" in incomplete.stderr)
  let guarded = test.run_script(ctx, "let bad = match 1 { _ if false => 2 }\n")?
  (guarded.success) == (false)
  ("check.match-value-exhaustive" in guarded.stderr)
}

test test_value_tails_reject_missing_else_and_incomplete_match { |ctx|
  for fixture in [
    {source: "let bad = if true { 1 }\n", diagnostic: "parse.if-expression-else"},
    {source: "pure pick(flag: Bool) -> Int {\n  if flag {\n    1\n  }\n}\n", diagnostic: "check.if-value-else"},
    {source: "pure pick(code: Int) -> Str {\n  match code {\n    0 => \"ok\"\n  }\n}\n", diagnostic: "check.match-value-exhaustive"},
    {source: "pure pick(flag: Bool) -> Int {\n  if flag {\n    1\n  } else {\n    \"wrong\"\n  }\n}\n", diagnostic: "check.type-mismatch"},
  ] {
    let output = test.run_script(ctx, fixture.source)?
    assert ! output.success, output.stderr
    fixture.diagnostic in output.stderr
  }
}

test test_value_top_level_control_flow_keeps_statement_semantics { |ctx|
  let branch = test.run_script(ctx, "if true {\n  3\n} else {\n  4\n}\n")?
  assert branch.success, branch.stderr
  let matched = test.run_script(ctx, "match 1 {\n  1 => 5\n  _ => 6\n}\n")?
  assert matched.success, matched.stderr
  let final = test.run_script(ctx, "3\n")?
  final.status == 3
}

test test_value_blocks_evaluate_before_scope_cleanup { |ctx|
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
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  (output.stdout) == ("value\ncleanup\nafter\n7\n")
}

test test_value_blocks_return_through_loop_and_retry { |ctx|
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
  (output.success) == (true)
  (output.stdout) == ("7 9\n")
}

test test_value_callback_return_survives_retry_and_stream_cleanup { |ctx|
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
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  (output.stdout) == ("branch cleanup\nsource cleanup\n7\n")
}

test test_value_callbacks_keep_enclosing_loop_targets {
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
  (visits) == (4)
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
  (visits) == (1)
}

test test_value_callback_tail_precedes_cleanup { |ctx|
  let output = test.run_script(ctx, """proc mark(message: Str) [] { print $message }
proc result(number: Int) [] -> Int { print "value"; number }
let mapped = [7] |> map { |number|
  defer mark("cleanup")
  result(number)
} |> collect
print \${mapped[0]}
""")?
  (output.success) == (true)
  (output.stdout) == ("value\ncleanup\n7\n")
}

test test_value_parallel_callback_keeps_lexical_return { |ctx|
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
  (output.success) == (true)
  (output.stdout) == ("7 9 11\n")
}

test test_value_parallel_callback_failure_is_propagation { |ctx|
  let output = test.run_script(ctx, """error WorkerError = failed(message: Str)
pure outcome() -> Result[Int] { Err(WorkerError.failed(message: "worker failed")) }
let values = [1, 2] |> par-map(jobs: 2) { |number|
  let value = outcome()?
  value
} |> collect
print "unreachable"
""")?
  (output.status) == (3)
  ("WorkerError.failed" in output.stderr)
  (("return-outside-function" in output.stderr)) == (false)
  (output.stdout) == ("")
}

test test_value_fold_and_key_callbacks_have_ordinary_scopes {
  let total = [1, 2] |> fold(0) { |acc, number|
    let added = acc + number
    match number {
      1 | _ => { let result = added; result }
    }
  }
  (total) == (3)
  let grouped = [1, 2] |> reduce-by(sum: true) { |number|
    let key = "all"
    if number == 1 { {key, value: number} } else { {key, value: number} }
  }
  ((grouped.get("all") ?? 0)) == (3)
  let sorted = [2, 1] |> sort-by { |number| let key = number; key } |> collect
  (sorted) == ([1, 2])
}

test test_value_branches_preserve_tags_dotted_pipelines_and_tee {
  let options = {items: [1, 2]}
  let selected = if false { options.items } else { options.items |> drop(1) }
  (selected) == ([2])
  let chosen: ValueChoice = if true { ValueEmpty } else { ValueNumber(1) }
  (chosen) == (ValueEmpty)
  let rows = [1] |> tee { |number|
    if false { print $number }
  } |> collect
  (rows) == ([1])
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
  (value_block_subtract(3)) == (2)
  (value_block_subtract(0)) == (0)
}
