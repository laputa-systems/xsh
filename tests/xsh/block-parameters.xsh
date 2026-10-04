test test_error_handler_headers_bind_nominal_errors { |ctx|
  let output = test.run_script(
    ctx,
    r"""
error HeaderError = failed(message: Str)
pure fail() -> Result[Int, HeaderError] { Err(HeaderError.failed(message: "nominal")) }
pure message(failure: HeaderError) -> Str { failure.message }
proc guarded() [] -> Str {
  guard let value = fail() else { |failure|
    return message(failure)
  }
  f"{value}"
}
print ${guarded()}
with value = fail() {} else { |failure|
  print ${message(failure)}
}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """nominal
nominal
"""
}

test test_with_bindings_are_sequential_and_stop_at_the_first_error { |ctx|
  let output = test.run_script(
    ctx,
    r"""
error HeaderError = failed(message: Str)
proc reached(value: Int) [] -> Result[Int] { print $value; Ok(value) }
pure failed() -> Result[Int, HeaderError] { Err(HeaderError.failed(message: "stop")) }
with first = reached(1), second = reached(first + 1)? {
  print $second
} else { |_| print unreachable }
with first = reached(3), second = failed()?, third = reached(4) {
  print unreachable
} else { |failure| print ${failure.message} }
print after
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """1
2
2
3
stop
after
"""
}

test test_error_headers_accept_comments_newlines_omission_and_discard { |ctx|
  let output = test.run_script(
    ctx,
    r"""
error HeaderError = failed(message: Str)
pure failed() -> Result[Int, HeaderError] { Err(HeaderError.failed(message: "inside")) }
let failure = "outside"
with value = failed() {} else {
  # The header remains the first block construct.
  |failure|
  print ${failure.message}
}
with value = failed() {} else { print omitted }
with value = failed() {} else { |_| print discarded }
with direct = 4, next = direct + 1 { print $next } else { print unreachable }
print $failure
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """inside
omitted
discarded
5
outside
"""
}

test test_error_headers_reject_missing_inputs_and_invalid_bindings { |ctx|
  for {source, code} in [
    {
      source: """with value = Ok(1) {} else { |a, b| print $a $b }
""",
      code: "check.handler-block-params",
    },
    {
      source: """proc bad() [] { guard let value = Ok(1) else { |a, a| return }; print $value }
""",
      code: "check.duplicate-name",
    },
    {
      source: """with value = Ok(1) {} else { |failure| failure = 2 }
""",
      code: "check.assign-let",
    },
    {
      source: """with value = Ok(1) {} else { |failure| let failure = 2 }
""",
      code: "check.duplicate-name",
    },
    {
      source: """with value = Ok(1) {} else { |failure| print $failure }
print $failure
""",
      code: "check.unresolved-name",
    },
    {
      source: """with value = Ok(1) {} else { |failure| print $value }
""",
      code: "check.unresolved-name",
    },
    {
      source: """with value = Ok(1) {} else { |fs| print $fs }
""",
      code: "check.standard-module-shadow",
    },
    {
      source: """if true { |failure| print $failure }
""",
      code: "check.block-params",
    },
    {
      source: """if false {} else { |_| print bad }
""",
      code: "check.block-params",
    },
    {
      source: """proc bad() [] { defer { |_| print bad } }
""",
      code: "check.block-params",
    },
    {
      source: """with value = Ok(1) { |_| print bad } else {}
""",
      code: "check.block-params",
    },
  ] {
    let output = test.run_script(ctx, source)?
    assert ! output.success, source
    assert code in output.stderr
  }
}

test test_outside_error_headers_are_rejected_before_execution { |ctx|
  for source in [
    """print reached
with value = Ok(1) {} else |failure| { print $failure }
""",
    """print reached
proc bad() [] { guard let value = Ok(1) else |_| { return }; print $value }
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert ! output.success, source
    assert output.stdout == ""
    assert "parse.block-header-migration" in output.stderr
    assert "inside the block" in output.stderr
  }
}

test test_with_headers_preserve_cleanup_and_lexical_transfers { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc exercise() [] -> Int {
  defer { print outer }
  for item in [1, 2] {
    with value = Ok(item) {
      defer { print f"cleanup:{value}" }
      print $value
      continue when value == 1
      return value
    } else { |_| print unreachable }
  }
  0
}
proc escape() [] -> Int {
  let result: Result[Int] = "bad".parse_int()
  with value = result ?? { |_| return 8 } { print unreachable } else {}
  0
}
print ${exercise()} ${escape()}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """1
cleanup:1
2
cleanup:2
outer
2 8
"""
}

test test_with_handler_errors_and_body_propagation_keep_their_identity { |ctx|
  for {body, expected} in [
    {
      body: "with value = failed() {} else { |_| defer { print cleanup }; Err(HeaderError.handler(message: \"handler\"))? }",
      expected: "handler",
    },
    {
      body: "with value = Ok(1) { defer { print cleanup }; failed()? } else { |_| print wrongly_caught }",
      expected: "primary",
    },
  ] {
    let source = """error HeaderError = primary(message: Str) | handler(message: Str)
pure failed() -> Result[Unit, HeaderError] { Err(HeaderError.primary(message: "primary")) }
proc exercise() [error] {
""" + body + """\n}
exercise()?
"""
    let output = test.run_script(ctx, source)?
    assert ! output.success, output.stderr
    assert output.stdout == """cleanup
"""
    assert expected in output.stderr
  }
}

test test_block_header_migration_preserves_comments_and_converges { |ctx|
  let source = r"""proc recover() [] -> Int {
  guard let value = "invalid".parse_int() else |failure| {
    # Keep the handler body and its comment.
    print ${failure.message}
    return 7
  }
  value
}
print recover()
"""
  let candidate = test.temp_file(ctx, name: "legacy-header.xsh", contents: bytes.from_text(source))?
  let guidance = run.capture --text "xsht" lint $candidate ?
  assert "lint.block-header" in guidance.stderr
  let fixed = run.capture --text "xsht" lint --fix $candidate ?
  assert fixed.status.exited_with(0), fixed.stderr
  let rewritten = candidate.read_text()?
  assert rewritten == source.replace("else |failure| {", "else { |failure|")
  let again = run.capture --text "xsht" lint --fix $candidate ?
  assert again.status.exited_with(0), again.stderr
  assert candidate.read_text()? == rewritten
  let checked = run.capture --text "xsht" check $candidate ?
  assert checked.status.exited_with(0), checked.stderr
  let formatted = run.capture --text "xsht" fmt $candidate ?
  assert formatted.status.exited_with(0), formatted.stderr
  assert "else { |failure|" in candidate.read_text()?
}

test test_block_header_migration_refuses_header_comments_and_unrelated_errors { |ctx|
  for source in [
    r"""proc recover() [] -> Int {
 guard let value = "bad".parse_int() else |failure| # header comment
 { print ${failure.message}; return 7 }
 value
}
print ${recover()}
""",
    """with value = Ok(1) {} else |failure| { print $failure }
let broken = )
""",
    """with value = Ok(1) {} else |failure| { print $failure }
let broken: Int = "wrong"
""",
  ] {
    let candidate = test.temp_file(ctx, name: "unsafe-header.xsh", contents: bytes.from_text(source))?
    let attempted = run.capture --text "xsht" lint --fix $candidate ?
    assert ! attempted.status.exited_with(0), attempted.stderr
    assert candidate.read_text()? == source
  }
}

test test_error_headers_suspend_in_streams_and_cleanup_on_cancellation { |ctx|
  let output = test.run_script(
    ctx,
    r"""
stream values() [] -> Stream[Int] {
  with value = Ok(1) {
    defer { print closed }
    yield value
    print unread
  } else { |_| yield 0 }
}
for value in values() { print $value; break }
stream guarded() [] -> Stream[Int] {
  guard let value = "bad".parse_int() else { |_|
    defer { print guarded_closed }
    yield 7
    return
  }
  yield value
}
let result = guarded() |> collect
print ${result[0]}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """1
closed
guarded_closed
7
"""
}

proc header_open_root(root_path: Path) [fs, error] -> Result[FsRoot] {
  with
    root = fs.open_root(root_path)
  {
    return root
  } else { |failure|
    return Err(failure)
  }
}

test test_with_headers_preserve_escaping_owned_prefix_values { |ctx|
  let root_path = test.temp_dir(ctx, name: "header-root")?
  let root = header_open_root(root_path)?
  root.write(p"value", "retained")?
  assert root.read_text(p"value")? == "retained"
  root.close()?
}

test test_block_header_migration_rechecks_imports_and_deduplicates_edits { |ctx|
  let directory = test.temp_dir(ctx, name: "header-imports")?
  let shared = fp"{directory}/shared.xsh"
  let shared_source = r"""##! Shared header fixture.
## Recover an integer.
export proc recover() [] -> Int {
  guard let value = "bad".parse_int() else |failure| { print ${failure.message}; return 7 }
  value
}
"""
  shared.write(shared_source)?
  for name in ["first", "second"] {
    fp"{directory}/{name}.xsh".write(r"""use shared as shared
proc recover() [] -> Int {
  guard let value = "bad".parse_int() else |_| { return shared.recover() }
  value
}
print recover()
""")?
  }

  let fixed = run.capture --text "xsht" lint --fix $directory ?
  assert fixed.status.exited_with(0), fixed.stderr
  let rewritten = shared.read_text()?
  assert rewritten == shared_source.replace("else |failure| {", "else { |failure|")
  let again = run.capture --text "xsht" lint --fix $directory ?
  assert again.status.exited_with(0), again.stderr
  assert shared.read_text()? == rewritten
}

test test_with_initializers_use_the_existing_heap_frame_stack { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc count(depth: Int) [] -> Int {
  if depth == 0 { return 0 }
  with next = count(depth - 1) { return next + 1 } else { |_| return 0 }
}
print count(2000)
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """2000
"""
}

test test_with_compound_initializers_keep_nominal_errors { |ctx|
  let output = test.run_script(
    ctx,
    r"""
error HeaderError = failed(message: Str)
pure failed() -> Result[Int, HeaderError] { Err(HeaderError.failed(message: "compound")) }
pure message(failure: HeaderError) -> Str { failure.message }
proc selected() [error] -> Str {
  with value = 1 + failed()? { return f"{value}" } else { |failure| return message(failure) }
}
print ${selected()}
with value = 1 + failed()? { print unreachable } else { |failure| print ${message(failure)} }
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """compound
compound
"""
}
