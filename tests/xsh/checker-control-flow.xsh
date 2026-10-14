# Observable return and loop flow at the checker boundary.
type FlowCase = {name: Str, source: Str, diagnostics: List[Str]}

# Checks each source separately so a missing diagnostic cannot be hidden by
# another case in the batch.
proc expect_flow_cases(ctx: TestContext, cases: List[FlowCase]) [fs, process, error] {
  var failures: List[Str] = []
  let root = test.temp_dir(ctx, name: "flow-cases")?
  for item in cases {
    let file = fp"{root}/{item.name}.xsh"
    file.write(item.source)
  }

  for item in cases {
    let file = fp"{root}/{item.name}.xsh"
    let checked = run.capture --text "xsht" check --summary $file
    let diagnostic_output = checked.stderr.split("xsht check summary:")[0]
    let diagnostic_blocks = diagnostic_output.split("err[")
    let filename = f"{item.name}.xsh"
    if item.diagnostics.len() > 0 {
      if !checked.status.exited_with(2) {
        failures += [f"{filename}: expected status 2: {checked.stderr}"]
      }
    } else {
      if !checked.status.exited_with(0) {
        failures += [f"{filename}: expected status 0: {checked.stderr}"]
      }
    }
    if diagnostic_blocks.len() - 1 != item.diagnostics.len() {
      failures += [f"{filename}: expected {item.diagnostics.len()} diagnostics, got {diagnostic_blocks.len() - 1}: {checked.stderr}"]
    }
    for code in item.diagnostics {
      let actual_code = diagnostic_blocks |> where { |block| f"{code}]" in block } |> count()
      let expected_count = item.diagnostics |> where { |expected| expected == code } |> count()
      if actual_code != expected_count {
        failures += [f"{filename}: expected {expected_count} {code} diagnostics, got {actual_code}: {checked.stderr}"]
      }
    }
  }
  assert failures.is_empty(), failures.join("\n")
}

test test_loop_completion_and_missing_returns { |ctx|
  expect_flow_cases(
    ctx,
    [
      {
        name: "infinite-loop-returns",
        source: r"""pure choose() -> Int {
  loop { return 1 }
}
""",
        diagnostics: [],
      },
      {
        name: "reachable-break-falls-through",
        source: r"""pure choose(stop: Bool) -> Int {
  loop {
    if stop { return 1 }
    break
  }
}
""",
        diagnostics: ["check.missing-return"],
      },
      {
        name: "nested-break-is-local",
        source: r"""pure choose() -> Int {
  loop {
    loop { break }
    return 1
  }
}
""",
        diagnostics: [],
      },
      {
        name: "break-in-return-value",
        source: r"""pure choose(stop: Bool) -> Int {
  while true {
    return if stop { break } else { 1 }
  }
}
""",
        diagnostics: ["check.missing-return"],
      },
      {
        name: "whitespace-json-lines-can-be-empty",
        source: r"""pure choose() -> Int { for row in " \n" |> json.lines { return 1 } }
""",
        diagnostics: ["check.missing-return"],
      },
      {
        name: "dynamic-breaks-in-call-and-binary-operands",
        source: r"""pure identity(value: Int) -> Int { value }
pure choose_call(stop: Bool) -> Int { while true { return identity(if stop { break } else { 1 }) } }
pure choose_binary(stop: Bool) -> Int { while true { return 1 + (if stop { break } else { 2 }) } }
""",
        diagnostics: ["check.missing-return", "check.missing-return"],
      },
      {
        name: "collection-conversions-and-nonempty-list-spreads",
        source: r"""pure choose_set() -> Int { for item in [1].to_set() { return item } }
pure choose_list() -> Int { for item in [1].to_set().to_list() { return item } }
pure choose_spread() -> Int { for item in [@[1]] { return item } }
""",
        diagnostics: [],
      },
      {
        name: "pattern-branch-return-does-not-hide-fallback-completion",
        source: r"""pure choose(value: Result[Int, Error]) -> Int {
  let ignored = if let Ok(number) = value { return number } else { 0 }
}
""",
        diagnostics: ["check.missing-return"],
      },
      {
        name: "break-in-match-subject",
        source: r"""pure choose(stop: Bool) -> Int {
  loop {
    match if stop { break } else { 0 } {
      0 => { return 1 }
      _ => { return 2 }
    }
  }
}
""",
        diagnostics: ["check.missing-return"],
      },
      {
        name: "break-in-match-arm",
        source: r"""pure choose(stop: Bool) -> Int {
  loop {
    match stop {
      true => { break }
      false => { return 1 }
    }
  }
}
""",
        diagnostics: ["check.missing-return"],
      },
      {
        name: "break-in-while-condition",
        source: "pure choose(result: Result[Bool, Error]) -> Int { loop { while result ?? { |failure| break } { return 1 } } }\n",
        diagnostics: ["check.missing-return"],
      },
      {
        name: "break-in-try-body",
        source: r"""proc choose() [error] -> Int {
  loop {
    let ignored: Result[Unit, Error] = try { break; Ok() }
  }
}
""",
        diagnostics: ["check.missing-return"],
      },
      {
        name: "all-boolean-match-arms-return",
        source: r"""pure choose(value: Bool) -> Int {
  match value {
    true => { return 1 }
    false => { return 2 }
  }
}
pure choose_binding(value: Int) -> Int { match value { item => { return item } }
}
""",
        diagnostics: [],
      },
    ],
  )
}

test test_loop_no_break_uses_only_reachable_breaks { |ctx|
  expect_flow_cases(
    ctx,
    [
      {
        name: "reachable-break-satisfies-loop",
        source: "proc spin() [] { loop { break } }\n",
        diagnostics: [],
      },
      {
        name: "empty-string-loop-body-return-is-unreachable",
        source: "pure never_returns() -> Int { loop { for _ in \"\" { return 1 } } }\n",
        diagnostics: ["check.loop-no-break"],
      },
      {
        name: "false-branch-break-is-unreachable",
        source: "proc spin() [] { loop { if false { break } } }\n",
        diagnostics: ["check.loop-no-break"],
      },
      {
        name: "false-guard-break-is-unreachable",
        source: "proc spin(value: Int) [] { loop { match value { _ if false => { break } _ => {} } } }\n",
        diagnostics: ["check.loop-no-break"],
      },
      {
        name: "break-after-continue-is-unreachable",
        source: "proc spin() [] { loop { continue; break } }\n",
        diagnostics: ["check.loop-no-break"],
      },
      {
        name: "break-in-unreachable-else-after-return-does-not-exit-loop",
        source: "pure choose() -> Int { while true { if true { return 1 } else { break } } }\n",
        diagnostics: [],
      },
      {
        name: "impossible-literal-arm-does-not-break-loop",
        source: "proc spin() [] { loop { match 0 { 1 => { break } _ => {} } } }\n",
        diagnostics: ["check.loop-no-break"],
      },
      {
        name: "unmatched-statement-match-keeps-path",
        source: "proc spin() [] { loop { match 0 { 1 => { break } } } }\n",
        diagnostics: ["check.loop-no-break"],
      },
      {
        name: "short-circuit-prunes-breaks",
        source: r"""pure side_effect() -> Bool { true }
proc spin() [] {
  loop {
    if false and side_effect() { break }
    if !true { break }
    if false or false { break }
    if true or (if true { break } else { false }) {}
  }
}
""",
        diagnostics: ["check.loop-no-break"],
      },
      {
        name: "deferred-break-does-not-break-loop",
        source: "proc choose() [io] -> Int { loop { defer { break } } }\n",
        diagnostics: ["check.loop-no-break", "check.defer-control-flow"],
      },
    ],
  )
}

test test_error_capture_retry_and_cleanup_keep_exit_paths { |ctx|
  expect_flow_cases(
    ctx,
    [
      {
        name: "try-captures-fallible-call",
        source: "proc fallible() [error] {}\nproc choose() -> Int { let ignored: Result[Int, Error] = try { fallible(); return 1 } }\n",
        diagnostics: ["check.missing-return"],
      },
      {
        name: "try-captures-assertion",
        source: "pure choose(flag: Bool) -> Int { let ignored: Result[Int, Error] = try { assert flag; return 1 } }\n",
        diagnostics: ["check.missing-return"],
      },
      {
        name: "try-captures-checked-conversion",
        source: "pure parse(value: Str) -> Int { let ignored: Result[Int, Error] = try { let n = value as Int; return n } }\n",
        diagnostics: ["check.missing-return"],
      },
      {
        name: "retry-captures-attempt-propagation",
        source: "pure choose(value: Result[Int, Error]) -> Int { let ignored: Result[Int, Error] = retry [] { let parsed = value?; return parsed } }\n",
        diagnostics: ["check.missing-return"],
      },
      {
        name: "unreachable-captured-error-does-not-fall-through",
        source: "proc fallible() [error] {}\nproc choose() -> Int { let ignored: Result[Int, Error] = try { return 1; fallible(); 2 } }\n",
        diagnostics: [],
      },
      {
        name: "unreachable-retry-error-does-not-fall-through",
        source: "proc fallible() [error] {}\nproc choose() -> Int { let ignored: Result[Int, Error] = retry [] { return 1; fallible(); 2 } }\n",
        diagnostics: [],
      },
      {
        name: "retry-failure-continues-to-later-loop-break",
        source: "proc spin() { loop { let attempt = retry [] { assert false }; break } }\n",
        diagnostics: [],
      },
      {
        name: "explicit-return-through-resource-scope",
        source: "proc choose(root: FsRoot) [fs] -> Int { let ignored = with held = root { return 1 } }\n",
        diagnostics: [],
      },
      {
        name: "tempdir-setup-error-can-fall-through-captured-body",
        source: "proc choose() [fs, error] -> Int { let ignored: Result[Int, Error] = try { tempdir dir { return 1 }? } }\n",
        diagnostics: ["check.missing-return"],
      },
      {
        name: "captured-with-propagation-does-not-exit-loop",
        source: "pure choose(result: Result[Int, Error]) -> Result[Int, Error] { loop { with value = result? {} else { |_| {} } } }\n",
        diagnostics: ["check.ignored-result", "check.loop-no-break"],
      },
      {
        name: "return-through-error-boundaries-remains-an-exit",
        source: "pure choose() -> Int { loop { let value: Result[Int, Error] = try { return 1; 2 } } }\nproc choose_with(result: Result[Int, Error]) [error] -> Int { loop { with value = result? { return value } else { |_| return 0 } } }\nproc leave() [] { loop { exit 0 } }\n",
        diagnostics: [],
      },
      {
        name: "deferred-result-error-exits-continued-loop",
        source: "proc fallible_unit() -> Result[Unit, Error] { return Ok() }\nproc spin() [error] { loop { defer fallible_unit(); continue } }\n",
        diagnostics: [],
      },
      {
        name: "deferred-effect-error-exits-continued-loop",
        source: "proc fallible() [error] {}\nproc spin() [error] { loop { defer fallible(); continue } }\n",
        diagnostics: [],
      },
      {
        name: "captured-propagation-does-not-exit-loop",
        source: "pure choose(result: Result[Int, Error]) -> Result[Int, Error] { loop { let value = try { result? } } }\n",
        diagnostics: ["check.loop-no-break"],
      },
      {
        name: "unhandled-propagation-exits-loop",
        source: "pure choose(result: Result[Int, Error]) -> Result[Int, Error] { loop { let value = result? } }\n",
        diagnostics: [],
      },
      {
        name: "result-fallback-return-does-not-hide-success-fallthrough",
        source: "pure choose(result: Result[Int, Error]) -> Int { let ignored = result ?? { |failure| return 0 } }\n",
        diagnostics: ["check.missing-return"],
      },
    ],
  )
}

test test_result_and_stream_iteration_propagates_when_demanded { |ctx|
  expect_flow_cases(
    ctx,
    [
      {
        name: "result-iteration-and-comprehensions",
        source: r"""proc spin_map(source: Result[Map[Str, Int], Error]) [error] { loop { for {key, value} in source {} } }
proc spin_text(source: Result[Str, Error]) [error] { loop { for item in source {} } }
proc spin_bytes(source: Result[Bytes, Error]) [error] { loop { for item in source {} } }
proc spin_list(source: Result[List[Int], Error]) [error] { loop { for item in source { continue } } }
proc spin_set(source: Result[Set[Int], Error]) [error] { loop { for item in source { continue } } }
proc spin_stream(source: Result[Stream[Int], Error]) [error] { loop { for item in source { continue } } }
proc comp_map(source: Result[Map[Str, Int], Error]) [error] { loop { let ignored = [value for {key, value} in source] } }
proc comp_text(source: Result[Str, Error]) [error] { loop { let ignored = [item for item in source] } }
proc comp_bytes(source: Result[Bytes, Error]) [error] { loop { let ignored = [item for item in source] } }
proc comp_list(source: Result[List[Int], Error]) [error] { loop { let ignored = [item for item in source] } }
proc comp_set(source: Result[Set[Int], Error]) [error] { loop { let ignored = {item for item in source} } }
proc comp_stream(source: Result[Stream[Int], Error]) [error] { loop { let ignored = [item for item in source] } }
""",
        diagnostics: [],
      },
      {
        name: "pulled-stream-errors-are-captured",
        source: "stream failing_rows() [error] -> Stream[Int] { yield 1; let ignored = \"late\".parse_int()? }\nproc choose() [error] -> Int { let ignored: Result[Unit, Error] = try { let values = failing_rows().collect(); return values.len() } }\n",
        diagnostics: ["check.missing-return"],
      },
      {
        name: "zero-take-skips-lazy-producer-error",
        source: "stream failing_rows() [error] -> Stream[Int] { let ignored = \"late\".parse_int()?; yield 1 }\nproc choose() [error] -> Int { let ignored: Result[Unit, Error] = try { let _ = failing_rows() |> take(0); return 1 } }\n",
        diagnostics: [],
      },
      {
        name: "demanded-stage-error-adds-capture-exit",
        source: "proc fail_item(item: Int) [error] -> Int { return item }\nproc choose() [error] -> Int { let ignored: Result[Unit, Error] = try { let _ = [1] |> map(fail_item); return 1 } }\n",
        diagnostics: ["check.missing-return"],
      },
      {
        name: "zero-pull-skips-stage-error",
        source: "proc fail_item(item: Int) [error] -> Int { return item }\nproc choose() [error] -> Int { let ignored: Result[Unit, Error] = try { let _ = [1] |> map(fail_item) |> take(0); return 1 } }\n",
        diagnostics: [],
      },
      {
        name: "result-unit-stage-propagates-only-when-pulled",
        source: "proc maybe_fail(item: Int) -> Result[Unit, Error] { return Ok() }\nproc demanded() [error] -> Int { let ignored: Result[Unit, Error] = try { let _ = [1] |> tee(maybe_fail); return 1 } }\nproc skipped() [error] -> Int { let ignored: Result[Unit, Error] = try { let _ = [1] |> tee(maybe_fail) |> take(0); return 1 } }\n",
        diagnostics: ["check.missing-return"],
      },
      {
        name: "range-and-repeat-capacity-errors-precede-callback-returns",
        source: "proc choose_range() [error] -> Int { let ignored: Result[Unit, Error] = try { let _ = [] |> range(0, 9223372036854775807) |> map { |item| return 1 } } }\nproc choose_repeat() [error] -> Int { let ignored: Result[Unit, Error] = try { let _ = [1] |> repeat(9223372036854775807) |> map { |item| return 1 } } }\n",
        diagnostics: ["check.missing-return", "check.missing-return"],
      },
      {
        name: "empty-zip-does-not-pull-a-latent-left-source",
        source: "stream child() [error] -> Stream[Int] { let _ = \"late\".parse_int()?; yield 1 }\nproc choose_direct() [error] -> Int { let ignored: Result[Unit, Error] = try { let _ = child() |> zip([]); return 1 } }\nproc choose_alias() [error] -> Int { let rows = child(); let ignored: Result[Unit, Error] = try { let _ = rows |> zip([]); return 1 } }\n",
        diagnostics: [],
      },
      {
        name: "zip-consumes-direct-and-aliased-latent-right-sources",
        source: "stream child() [error] -> Stream[Int] { let _ = \"late\".parse_int()?; yield 1 }\nproc spin_direct() [error] { loop { let _ = [1] |> zip(child()) } }\nproc spin_alias() [error] { let rows = child(); loop { let _ = [1] |> zip(rows) } }\nproc choose_direct() [error] -> Int { let ignored: Result[Unit, Error] = try { let _ = [1] |> zip(child()); return 1 } }\nproc choose_alias() [error] -> Int { let rows = child(); let ignored: Result[Unit, Error] = try { let _ = [1] |> zip(rows); return 1 } }\n",
        diagnostics: ["check.missing-return", "check.missing-return"],
      },
      {
        name: "latent-stream-error-provenance-survives-composition-and-reassignment",
        source: r"""stream ready() -> Stream[Int] { yield 1 }
stream child() [error] -> Stream[Int] { let _ = "late".parse_int()?; yield 1 }
proc composite(flag: Bool) [error] { loop { let rows = if flag { child() } else { ready() }; for item in rows { continue } } }
proc reassigned() [error] { var rows = ready(); rows = child(); loop { for item in rows { continue } } }
proc branch(flag: Bool) [error] { var rows = ready(); if flag { rows = child() } else { rows = ready() }; loop { for item in rows { continue } } }
proc branch_reversed(flag: Bool) [error] { var rows = ready(); if flag { rows = ready() } else { rows = child() }; loop { for item in rows { continue } } }
""",
        diagnostics: [],
      },
      {
        name: "safe-stream-reassignment-does-not-create-an-exit",
        source: "stream ready() -> Stream[Int] { yield 1 }\nproc spin() [] { var rows = ready(); rows = ready(); loop { for item in rows {} } }\n",
        diagnostics: ["check.loop-no-break"],
      },
      {
        name: "stream-producer-effects-are-checked-at-creation",
        source: "stream child() [error] -> Stream[Int] { yield 1 }\nproc choose() [] { let ignored = child() |> take(0) }\n",
        diagnostics: ["check.effect-violation"],
      },
      {
        name: "builtin-range-collection-can-fail-inside-capture",
        source: "proc choose() [error] -> Int { let ignored: Result[Unit, Error] = try { let values = range(9223372036854775807).collect(); return values.len() } }\n",
        diagnostics: ["check.missing-return"],
      },
    ],
  )
}

test test_comprehension_cardinality_and_clause_order { |ctx|
  expect_flow_cases(
    ctx,
    [
      {
        name: "nonempty-list-set-map-comprehensions",
        source: r"""pure choose_list() -> Int { for item in [item for item in [1]] { return item } }
pure choose_set() -> Int { for item in {item for item in [1]} { return item } }
pure choose_map() -> Int { for item in {key: key for key in [1]} { return item.value } }
pure choose_nonempty_bytes() -> Int { for byte in b"\x01" { return byte } }
""",
        diagnostics: [],
      },
      {
        name: "negative-range-is-nonempty",
        source: "pure choose() -> Int { for item in range(-2, 0) { return item } }\n",
        diagnostics: [],
      },
      {
        name: "empty-sources-do-not-run-loop-bodies",
        source: r"""pure choose_list() -> Int { for item in [item for item in []] { return item } }
pure choose_bytes() -> Int { for byte in b"" { return byte } }
""",
        diagnostics: ["check.missing-return", "check.missing-return"],
      },
      {
        name: "computed-key-map-literal-is-known-nonempty",
        source: "pure choose() -> Int { for {key, value} in {[1]: 2} { return value } }\n",
        diagnostics: [],
      },
      {
        name: "empty-source-and-filter-skip-projection",
        source: r"""pure empty_projection() -> Int { let ignored = [if true { return 1 } else { 0 } for item in []] }
pure false_filter() -> Int { let ignored = [if true { return 1 } else { 0 } for item in [1] if false] }
pure dynamic_filter(flag: Bool) -> Int { let ignored = [if true { return 1 } else { 0 } for item in [1] if flag] }
""",
        diagnostics: ["check.missing-return", "check.missing-return", "check.missing-return"],
      },
      {
        name: "empty-range-comprehension-skips-loop-break-projection",
        source: "proc spin() [] { loop { let ignored = [if true { break } else { 0 } for item in range(0, 0)] } }\n",
        diagnostics: ["check.loop-no-break"],
      },
      {
        name: "later-comprehension-source-can-break-enclosing-loop",
        source: "proc spin(stop: Bool) [] { loop { let ignored = [item for outer in [1] for item in if stop { break } else { [outer] }] } }\n",
        diagnostics: [],
      },
      {
        name: "conditional-return-before-for-continue-keeps-fallthrough",
        source: "pure choose(flag: Bool) -> Int { for item in [1] { if flag { return item }; continue } }\n",
        diagnostics: ["check.missing-return"],
      },
      {
        name: "empty-later-source-skips-projection",
        source: "pure choose() -> Int { let ignored = [if true { return 1 } else { 0 } for outer in [1] for inner in []] }\n",
        diagnostics: ["check.missing-return"],
      },
      {
        name: "later-source-expression-is-still-evaluated",
        source: "pure choose() -> Int { let ignored = [value for outer in [1] for value in [] |> take(if true { return 1 } else { 0 })] }\n",
        diagnostics: [],
      },
      {
        name: "dynamic-prefix-filter-can-skip-later-source",
        source: "pure choose(flag: Bool) -> Int { let ignored = [value for item in [1] if flag for value in if true { return 1 } else { [item] }] }\n",
        diagnostics: ["check.missing-return"],
      },
      {
        name: "comprehension-pulls-pipeline-source-callback",
        source: "pure choose() -> Int { let ignored = [item for item in [1] |> map { |item| return item }] }\n",
        diagnostics: [],
      },
      {
        name: "nonempty-comprehension-runs-returning-projection",
        source: "pure choose() -> Int { let ignored = [if true { return 1 } else { 0 } for item in [1]] }\n",
        diagnostics: [],
      },
      {
        name: "user-defined-range-does-not-have-built-in-cardinality",
        source: "pure range(start: Int, end: Int) -> List[Int] { return [] }\npure choose() -> Int { let ignored = range(0, 1) |> map { |item| return item } }\n",
        diagnostics: ["check.missing-return"],
      },
    ],
  )
}

test test_pipeline_demand_and_materialization { |ctx|
  expect_flow_cases(
    ctx,
    [
      {
        name: "materialized-callbacks-run-in-expression-contexts",
        source: r"""pure consume(values: List[Int]) -> Unit { return }
pure argument() -> Int { let ignored = consume([1] |> map { |item| if true { return item }; item }) }
pure method() -> Int { let ignored = ([1] |> map { |item| if true { return item }; item }).len() }
pure collect_method() -> Int { let ignored = ([1] |> map { |item| if true { return item }; item }).collect().len() }
pure collect_statement() -> Int { let _ = ([1] |> map { |item| if true { return item }; item }).collect(); let ignored = 0 }
pure collect_condition() -> Int { if ([1] |> map { |item| if true { return item }; item }).collect().len() > 0 { return 1 } else {} }
""",
        diagnostics: [],
      },
      {
        name: "maybe-empty-and-filtered-pipelines-may-skip-callbacks",
        source: r"""pure dynamic_source(values: List[Int]) -> Int { let ignored = values |> map { |item| return item } }
pure filtered_source() -> Int { let ignored = [1] |> where { |item| false } |> map { |item| return item } }
pure user_range() -> Int { let ignored = range(0, 1) |> map { |item| return item } }
""",
        diagnostics: ["check.missing-return", "check.missing-return"],
      },
      {
        name: "zero-demand-and-empty-stages-skip-callbacks",
        source: r"""pure zero_take() -> Int { let ignored = [1] |> map { |item| return item } |> take(0) }
pure dynamic_take(count: Int) -> Int { let ignored = [1] |> map { |item| return item } |> take(count) }
pure zero_repeat() -> Int { let ignored = [1] |> repeat(0) |> map { |item| return item } }
pure empty_range_stage() -> Int { let ignored = [1] |> range(0, 0) |> map { |item| return item } }
pure empty_range_source() -> Int { let ignored = range(0, 0) |> map { |item| return item } }
""",
        diagnostics: ["check.missing-return", "check.missing-return", "check.missing-return", "check.missing-return", "check.missing-return"],
      },
      {
        name: "positive-cardinality-stages-demand-callbacks",
        source: r"""pure positive_take() -> Int { let ignored = [1] |> take(1) |> map { |item| if true { return item }; item } }
pure positive_repeat() -> Int { let ignored = [1] |> repeat(2) |> map { |item| if true { return item }; item } }
pure positive_range() -> Int { let ignored = [1] |> range(0, 1) |> map { |item| if true { return item }; item } }
pure reduce_by() -> Int { let ignored = [1] |> reduce-by(sum: true) { |item| if true { return item }; {key: "n", value: item} } }
""",
        diagnostics: [],
      },
      {
        name: "negative-repeat-count-does-not-create-a-captured-error-path",
        source: "proc choose() [error] -> Int { let ignored: Result[Unit, Error] = try { let _ = [1] |> repeat(-1); return 1 } }\n",
        diagnostics: [],
      },
      {
        name: "fold-result-cardinality-is-not-the-input-cardinality",
        source: "pure empty_accumulator() -> Int { for item in [1] |> fold([]) { |acc, item| acc } { return 1 } }\npure empty_input_nonempty_seed() -> Int { let source: List[Int] = []; for item in source |> fold([1]) { |acc, item| acc } { return 1 } }\n",
        diagnostics: ["check.missing-return", "check.missing-return"],
      },
      {
        name: "zip-cardinality-and-right-side-demand",
        source: r"""pure nonempty_zip() -> Int { let ignored = [1] |> zip([2]) |> map { |pair| return 1 } }
pure empty_left_drains_right() -> Int { let ignored = [] |> zip([2] |> map { |item| return item }) }
pure empty_zip_drains_left_group() -> Int { let ignored = [1] |> map { |item| return item } |> zip([]) }
pure empty_range_drains_input_group() -> Int { let ignored = [1] |> map { |item| return item } |> range(0, 0) }
pure zero_repeat_drains_input_group() -> Int { let ignored = [1] |> map { |item| return item } |> repeat(0) }
pure unique_materializes() -> Int { let ignored = [1] |> map { |item| return item } |> unique-by { |item| item } |> take(0) }
pure sort_materializes() -> Int { let ignored = [1] |> map { |item| return item } |> sort |> take(0) }
pure sort_by_materializes() -> Int { let ignored = [1] |> sort-by { |item| if true { return item } else { item } } |> take(0) }
pure group_by_preserves_cardinality() -> Int { let ignored = [1] |> group-by . |> map { |bucket| return 1 } }
""",
        diagnostics: [],
      },
      {
        name: "dynamic-zip-right-may-skip-left-callbacks",
        source: "pure choose(values: List[Int]) -> Int { let ignored = [1] |> map { |item| return item } |> zip(values) }\npure choose_downstream(values: List[Int]) -> Int { let ignored = [1] |> zip(values) |> map { |pair| return 1 } }\n",
        diagnostics: ["check.missing-return"],
      },
      {
        name: "unique-by-callback-runs-before-zero-take",
        source: "pure choose() -> Int { let ignored = [1] |> unique-by { |item| return item } |> take(0) }\n",
        diagnostics: [],
      },
      {
        name: "dynamic-repeat-materializes-input-before-zero-take",
        source: "pure choose(count: Int) -> Int { loop { let ignored = [1] |> map { |item| return 1 } |> repeat(count) |> take(0) } }\n",
        diagnostics: [],
      },
      {
        name: "empty-zip-can-skip-callbacks",
        source: r"""pure zip_empty_right() -> Int { let ignored = [1] |> map { |item| return item } |> zip([]) }
pure zip_empty_left_loop() -> Int { loop { let ignored = [] |> zip([2]) |> map { |pair| return 1 } } }
pure zip_empty_right_loop() -> Int { loop { let ignored = [1] |> zip([]) |> map { |pair| return 1 } } }
""",
        diagnostics: ["check.loop-no-break", "check.loop-no-break"],
      },
    ],
  )
}

test test_expression_order_and_checked_exits { |ctx|
  expect_flow_cases(
    ctx,
    [
      {
        name: "evaluated-condition-and-guard-breaks-reach-loop",
        source: r"""pure choose_if(stop: Bool) -> Int {
  loop {
  if if stop { break } else { true } { return 1 }
  }
}
pure choose_guard(stop: Bool) -> Int {
  loop {
    match 0 {
      0 if if stop { break } else { true } => { return 1 }
      _ => { return 2 }
    }
  }
}
""",
        diagnostics: ["check.missing-return", "check.missing-return"],
      },
      {
        name: "comparison-chain-later-operand-is-conditional",
        source: "pure choose(a: Int, b: Int) -> Int { let ignored = a < b < (if true { return 1 } else { 0 }) }\n",
        diagnostics: ["check.missing-return"],
      },
      {
        name: "optional-index-can-skip-its-operand",
        source: "pure choose() -> Int { let values: List[Int]? = null; while true { let ignored = values?[if true { return 1 } else { 0 }]; break } }\n",
        diagnostics: ["check.missing-return"],
      },
      {
        name: "optional-null-safe-slice-bound-can-break-the-loop",
        source: "pure choose(values: List[Int]?) -> Int { while true { let ignored = values?[if true { break } else { 0 }..]; return 1 } }\n",
        diagnostics: ["check.missing-return"],
      },
      {
        name: "result-index-runs-operand-on-success",
        source: "proc choose(values: Result[List[Int], Error]) [error] -> Int { let ignored = values?[if true { return 1 } else { 0 }] }\n",
        diagnostics: [],
      },
      {
        name: "captured-result-index-error-can-fall-through",
        source: "pure choose(values: Result[List[Int], Error]) -> Int { let ignored: Result[Int?, Error] = try { values?[if true { return 1 } else { 0 }] } }\n",
        diagnostics: ["check.missing-return"],
      },
      {
        name: "result-receiver-propagates-before-method-argument",
        source: "pure choose(source: Result[Map[Str, Int], Error]) -> Int { let ignored: Result[Unit, Error] = try { let _ = source?.get(if true { return 1 } else { \"key\" }) } }\n",
        diagnostics: ["check.missing-return"],
      },
      {
        name: "result-null-safe-field-and-method-propagate-from-loops",
        source: "type Holder = { value: Int }\nproc spin_field(source: Result[Holder, Error]) [error] { loop { let ignored = source?.value } }\nproc spin_method(source: Result[List[Int], Error]) [error] { loop { let ignored = source?.len() } }\nproc spin_before_continue(source: Result[Map[Str, Int], Error]) [error] { loop { let _ = source?.get(if true { continue } else { \"key\" }) } }\n",
        diagnostics: [],
      },
      {
        name: "optional-null-safe-method-can-skip-arguments",
        source: "pure choose(source: Map[Str, Int]?) -> Int { let ignored = source?.get(if true { return 1 } else { \"key\" }) }\n",
        diagnostics: ["check.missing-return"],
      },
      {
        name: "result-guarded-slice-evaluates-bounds-on-success",
        source: "proc choose(values: Result[List[Int], Error]) [error] -> Int { let ignored = values?[if true { return 1 } else { 0 }..] }\n",
        diagnostics: [],
      },
      {
        name: "captured-result-slice-error-can-fall-through",
        source: "pure choose(values: Result[List[Int], Error]) -> Int { let ignored: Result[List[Int]?, Error] = try { values?[if true { return 1 } else { 0 }..] } }\n",
        diagnostics: ["check.missing-return"],
      },
      {
        name: "error-boundaries-and-unit-with-keep-explicit-exits",
        source: "pure choose() -> Int { with value = 1 { return value } else {} }\npure choose_captured_with() -> Int { let ignored: Result[Unit, Error] = try { with value = 1 { return 1 } else { assert false } } }\nproc spin_run() [process, error] { loop { run true ? } }\nproc spin_text() [process, error] { loop { let output = run.text printf \"ok\" } }\nproc unit_result() -> Result[Unit, Error] { return Ok() }\nproc spin_unit_result() [error] { loop { unit_result() } }\nproc spin_cd() [env, error] { loop { cd \".\" {} } }\n",
        diagnostics: [],
      },
      {
        name: "direct-fallible-call-exits-loop-on-error",
        source: "proc fallible() [error] {}\nproc spin() [error] { loop { fallible() } }\n",
        diagnostics: [],
      },
      {
        name: "unhandled-propagation-in-while-condition-exits-outer-loop",
        source: "proc spin(source: Result[Bool, Error]) [error] { loop { while source? {} } }\n",
        diagnostics: [],
      },
      {
        name: "unhandled-propagation-in-with-else-exits-outer-loop",
        source: "proc spin(source: Result[Int, Error]) [error] { loop { with item = source {} else { |_| let ignored = source? } } }\n",
        diagnostics: [],
      },
      {
        name: "builder-field-break-reaches-the-enclosing-loop",
        source: "proc choose(stop: Bool) [process, error] -> Int { loop { let command = process.command { timeout = if stop { break } else { 1s }; run true }; return 1 } }\n",
        diagnostics: ["check.missing-return"],
      },
      {
        name: "builder-task-break-does-not-leave-the-enclosing-loop",
        source: "proc spin() [process, error] { loop { let command = process.command { task worker() { break }; run true } } }\n",
        diagnostics: ["check.loop-no-break", "check.loop-control", "check.builder-entry"],
      },
      {
        name: "captured-cd_setup_failure-keeps_function_fallthrough",
        source: "proc choose(root_dir: Path) [env] -> Int { let ignored: Result[Int, Error] = try { cd (root_dir) { return 1 }? } }\n",
        diagnostics: ["check.missing-return"],
      },
    ],
  )
}

test test_json_lines_may_drop_whitespace_only_input { |ctx|
  var visited = 0
  for row in " \n" |> json.lines {
    visited += 1
  }
  assert visited == 0
}
