# What `xsht check` reported for one program.
type Checked = {status: Status, stderr: Str}

# Runs only the checker over `source`.
proc check(ctx: TestContext, source: Str) [fs, process, error] -> Result[Checked] {
  let file = test.temp_file(ctx, name: "checked.xsh", contents: bytes.from_text(source))?
  let checked = run.capture --text "xsht" check $file
  {status: checked.status, stderr: checked.stderr}
}

# Requires the checker to reject `source` with every diagnostic in `codes`.
proc expect_rejected(ctx: TestContext, source: Str, codes: List[Str]) [fs, process, error] {
  let checked = check(ctx, source)?
  assert checked.status.exited_with(2), f"{source}: {checked.stderr}"
  for code in codes {
    assert f"[{code}]" in checked.stderr, f"expected {code} for {source}: {checked.stderr}"
  }
}

# Requires the checker to accept `source` without any diagnostic.
proc expect_clean(ctx: TestContext, source: Str) [fs, process, error] {
  let checked = check(ctx, source)?
  assert checked.status.exited_with(0), f"{source}: {checked.stderr}"
  assert "[check." not in checked.stderr, f"{source}: {checked.stderr}"
}

# Requires the checker to report none of `codes` for `source`, which other
# diagnostics may still reject.
proc expect_free_of(ctx: TestContext, source: Str, codes: List[Str]) [fs, process, error] {
  let checked = check(ctx, source)?
  for code in codes {
    assert f"[{code}]" not in checked.stderr, f"unexpected {code} for {source}: {checked.stderr}"
  }
}

test test_checker_handles_batch_stream_stage { |ctx|
  expect_free_of(
    ctx,
    "\nlet chunks = [Path(\"a\"), Path(\"b\")] |> batch(count: 1, max_argv: true)\n",
    ["check.stream-batch", "check.arity"],
  )
  expect_rejected(ctx, "[1, 2] |> batch\n", ["check.stream-batch"])
  expect_rejected(ctx, "[{ name: \"a\" }] |> batch(max_argv: true)\n", ["check.stream-batch"])
}

test test_checker_handles_adapter_stream_stages { |ctx|
  expect_free_of(
    ctx,
    r"""
let paths = "a.txt\nb.log\n" |> text.lines() |> map { |line| Path(line) }
let chunks = b"abcd" |> bytes.chunks(2)
let rows = "{\"name\":\"a\"}\n" |> json.lines()
let streamed = "{\"name\":\"b\"}\n" |> json.stream()
let words = "one two".words()
let split = "a,b".split(",")
let fields = "a::b".fields(delimiter: ":")
let joined = fields.join(separator: "|")
let replaced = joined.replace("|", with: ",")
let reversed = replaced.reverse()
let wrapped = "alpha beta".wrap(8)
let slug = "alpha beta".translate(" ", "-")
let deleted = "a-b".delete("-")
let squeezed = "nooo".squeeze(chars: "o")
let line_count = "a\nb\n".count_lines()
let word_count = "a b".count_words()
let char_count = "hé".count_chars()
let byte_count = "hé".byte_len()
""",
    ["check.stream-input", "check.type-mismatch"],
  )

  for case in [
    {source: "b\"abc\" |> text.lines()\n", code: "check.type-mismatch"},
    {source: "[\"a\"] |> map { . } |> text.lines()\n", code: "check.stream-adapter"},
    {source: "b\"abc\" |> bytes.chunks()\n", code: "check.named-arg"},
    {source: "1 |> json.stream()\n", code: "check.type-mismatch"},
  ] {
    expect_rejected(ctx, case.source, [case.code])
  }
}

test test_checker_handles_fold_accumulator_plus_item_blocks { |ctx|
  # The two-parameter accumulator form types the first param as the
  # accumulator (initial value) and the second as the stream item; the tail
  # must return the accumulator type.
  expect_free_of(
    ctx,
    r"""
let total = [1, 2, 3] |> fold(0) { |acc, it| acc + it }
let counted = ["a", "b", "a"] |> fold(map.empty()) { |acc, it| acc.set(it, (acc.get(it) ?? 0) + 1) }
""",
    ["check.stream-block-params", "check.type-mismatch", "check.arity"],
  )

  # A bare accumulator tail compiles and is no longer an IR-blocked form.
  expect_free_of(
    ctx,
    "let x = [1, 2] |> fold(0) { |acc| acc }\n",
    ["check.stream-block-params", "check.unresolved-name"],
  )

  # A three-parameter fold block is rejected with a fold-specific diagnostic.
  expect_rejected(
    ctx,
    "let x = [1, 2] |> fold(0) { |acc, it, extra| acc + it }\n",
    ["check.stream-block-params"],
  )

  # Non-fold stream stages still accept at most one parameter.
  expect_rejected(ctx, "let x = [1, 2] |> map { |a, b| a }\n", ["check.stream-block-params"])
}

test test_checker_accepts_pipeline_collect_terminal { |ctx|
  expect_free_of(
    ctx,
    "\nlet xs: List[Int] = [1, 2, 3] |> collect()\n",
    ["check.unresolved-call", "check.type-mismatch", "check.stream-terminal-stage"],
  )
}

test test_checker_rejects_table_and_sort_contract_errors { |ctx|
  for case in [
    {source: "[1] |> table.print()\n", code: "check.table-print"},
    {source: "[{ name: \"a\" }] |> table.print(columns: [1])\n", code: "check.type-mismatch"},
    {source: "[{ name: \"a\" }] |> sort-by { |r| [r.name] }\n", code: "check.stream-sort"},
    {source: "[{ name: \"a\" }] |> sort-by { |r| { scores: [r.name] } }\n", code: "check.stream-sort"},
  ] {
    expect_rejected(ctx, case.source, [case.code])
  }
}

test test_checker_accepts_sort_by_record_keys_and_record_sort_items { |ctx|
  for source in [
    "let _ = [{name: \"a\", count: 1}] |> sort-by { |r| {c: r.count, n: r.name} }\n",
    "let _ = [{id: 1}] |> sort-by { |r| {outer: {inner: r.id}} }\n",
    "let _ = [{name: \"b\", count: 2}, {name: \"a\", count: 1}] |> sort\n",
  ] {
    expect_clean(ctx, source)
  }
}

test test_checker_accepts_group_by_key_sort_by_for_scalar_keys { |ctx|
  for source in [
    "let _ = [3, 1, 2, 1] |> group-by { |x| x } |> sort-by { |g| g.key }\n",
    "let _ = [\"b\", \"a\"] |> group-by { |x| x } |> sort-by { |g| g.key }\n",
    "let _ = [true, false] |> group-by { |x| x } |> sort-by { |g| g.key }\n",
    "let _ = [Path(\"b\"), Path(\"a\")] |> group-by { |x| x } |> sort-by { |g| g.key }\n",
  ] {
    expect_clean(ctx, source)
  }
}

test test_checker_accepts_sort_by_desc_named_argument { |ctx|
  expect_clean(ctx, "let _ = [1, 2, 3] |> sort-by(desc: true) .\n")
  expect_rejected(ctx, "let _ = [1, 2, 3] |> sort-by(unknown: true) .\n", ["check.named-arg"])
}

# Map.empty() is Map[Any]; Map.get(k) ?? 0 therefore yields an Any-typed
# field. The map-accumulator pattern (and its list-comprehension
# equivalent) sorts by that field at runtime with a supported scalar, so
# the checker must accept it the same way the runtime does.
test test_checker_accepts_sort_by_any_typed_record_fields_from_map_get { |ctx|
  expect_free_of(
    ctx,
    r"""
let counts = map.empty()
let keys = ["b", "a"]
let acc = counts.set("a", 2).set("b", 1)
let by_count = keys
  |> map { |k| {count: (acc.get(k) ?? 0), ext: k} }
  |> sort-by .count
  |> collect()
let by_count_comp = [
  {count: (acc.get(k) ?? 0), ext: k}
  for k in keys
] |> sort-by .count |> collect()
""",
    ["check.stream-sort"],
  )
}

test test_checker_accepts_list_comprehensions { |ctx|
  expect_free_of(
    ctx,
    r"""
let nums: List[Int] = [1, 2, 3]
let doubled = [x * 2 for x in nums]
let filtered = [x for x in nums if x > 1]
let strs = [f"{{x}}" for x in nums]
""",
    ["check.type-mismatch", "check.listcomp-iterator", "check.listcomp-condition"],
  )
  expect_free_of(
    ctx,
    r"""
type Entry = {name: Str, score: Int}
let entries: List[Entry] = []
let names = [name for {name, score} in entries]
let passing = [name for {name, score} in entries if score >= 60]
""",
    [
      "check.type-mismatch",
      "check.destructure-type",
      "check.destructure-field",
      "check.listcomp-iterator",
      "check.listcomp-condition",
    ],
  )
}

test test_checker_rejects_list_comprehension_domains_and_conditions { |ctx|
  expect_rejected(ctx, "let x = [v for v in 42]\n", ["check.listcomp-iterator"])
  expect_rejected(
    ctx,
    "let nums: List[Int] = [1, 2]\nlet x = [v for v in nums if v]\n",
    ["check.listcomp-condition"],
  )
}

test test_checker_multi_clause_comprehensions_retain_nested_item_types { |ctx|
  expect_clean(
    ctx,
    r"""type Entry = {key: Str, values: List[Int]}
let entries: List[Entry] = []
let values: List[Int] = [number for entry in entries if entry.key != "" for number in entry.values if number > 0]
let by_key: Map[Int] = {entry.key: number for entry in entries for number in entry.values if number > 0}
""",
  )
}

test test_checker_multi_clause_comprehensions_reject_invalid_inner_domains_and_filters { |ctx|
  for case in [
    {
      source: "let values = [inner for outer in [1] for inner in outer]\n",
      code: "check.listcomp-iterator",
    },
    {
      source: "let values = [inner for outer in [1] for inner in [outer] if inner]\n",
      code: "check.listcomp-condition",
    },
    {
      source: "let by_key = {entry.key: inner for entry in [{key: \"a\"}] for inner in 1}\n",
      code: "check.mapcomp-iterator",
    },
  ] {
    expect_rejected(ctx, case.source, [case.code])
  }
}

test test_checker_map_iteration_preserves_entry_types_and_error_boundaries { |ctx|
  for case in [
    {
      source: "pure wrong(values: Map[Int]) -> Unit { for entry in values { let bad: Int = entry.key } }\n",
      code: "check.type-mismatch",
    },
    {
      source: "pure wrong(values: Map[Int]) -> Unit { for entry in values { let bad: Str = entry.value } }\n",
      code: "check.type-mismatch",
    },
    {
      source: "pure wrong(values: Map[Int]) -> Unit { for {missing} in values {} }\n",
      code: "check.destructure-field",
    },
    {
      source: "proc wrong(values: Result[Map[Int]]) [] { for entry in values {} }\n",
      code: "check.effect-violation",
    },
    {
      source: "error MapError = Missing(code: Int)\nerror OtherError = Failed(code: Int)\npure wrong(values: Result[Map[Int], MapError]) -> Result[List[Str], OtherError] { [entry.key for entry in values] }\n",
      code: "check.try-error",
    },
  ] {
    expect_rejected(ctx, case.source, [case.code])
  }
}

# The diagnostic covers the whole splice, `@"wrong"`, not only its operand.
test test_checker_list_splice_errors_cover_the_original_splice_span { |ctx|
  let checked = check(ctx, "let values = [@\"wrong\"]\n")?
  assert "err[check.list-splice-type]" in checked.stderr, checked.stderr
  assert ":1:15\n" in checked.stderr, checked.stderr
  assert "\n                ^^^^^^^^ " in checked.stderr, checked.stderr
}
