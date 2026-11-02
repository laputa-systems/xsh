test test_error_handler_headers_bind_nominal_errors { |ctx|
  let output = test.expect(
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
""",
    status: 0,
  )?
  assert output.stdout == """nominal
"""
}

test test_error_headers_reject_missing_inputs_and_invalid_bindings { |ctx|
  for {source, code} in [
    {
      source: """proc bad() [] { guard let value = Ok(1) else { |a, b| return }; print $value }
""",
      code: "check.handler-block-params",
    },
    {
      source: """proc bad() [] { guard let value = Ok(1) else { |a, a| return }; print $value }
""",
      code: "check.duplicate-name",
    },
    {
      source: """proc bad() [] { guard let value = Ok(1) else { |failure| failure = 2; return }; print $value }
""",
      code: "check.assign-let",
    },
    {
      source: """proc bad() [] { guard let value = Ok(1) else { |failure| let failure = 2; return }; print $value }
""",
      code: "check.duplicate-name",
    },
    {
      source: """proc bad() [] { guard let value = Ok(1) else { |failure| print $failure; return }; print $failure }
""",
      code: "check.unresolved-name",
    },
    {
      source: """proc bad() [] { guard let value = Ok(1) else { |failure| print $value; return } }
""",
      code: "check.unresolved-name",
    },
    {
      source: """proc bad() [] { guard let value = Ok(1) else { |fs| print $fs; return } }
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
  ] {
    let output = test.run_script(ctx, source)?
    assert ! output.success, source
    assert code in output.stderr
  }
}

test test_outside_error_headers_are_rejected_before_execution { |ctx|
  for source in [
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

test test_block_header_migration_preserves_comments_and_converges { |ctx|
  # The source is already what the default rules leave alone apart from the
  # header: the file has no project config above it, so every default applies.
  let source = r"""proc recover() -> Int {
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
  let guidance = run.capture --text "xsht" lint $candidate
  assert "lint.block-header" in guidance.stderr
  let fixed = run.capture --text "xsht" lint --fix $candidate
  assert fixed.status.exited_with(0), fixed.stderr
  let rewritten = candidate.read_text()?
  assert rewritten == source.replace("else |failure| {", with: "else { |failure|")
  let again = run.capture --text "xsht" lint --fix $candidate
  assert again.status.exited_with(0), again.stderr
  assert candidate.read_text()? == rewritten
  let checked = run.capture --text "xsht" check $candidate
  assert checked.status.exited_with(0), checked.stderr
  let formatted = run.capture --text "xsht" fmt $candidate
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
    """proc bad() [] { guard let value = Ok(1) else |failure| { print $failure; return }; print $value }
let broken = )
""",
    """proc bad() [] { guard let value = Ok(1) else |failure| { print $failure; return }; print $value }
let broken: Int = "wrong"
""",
  ] {
    let candidate = test.temp_file(ctx, name: "unsafe-header.xsh", contents: bytes.from_text(source))?
    let attempted = run.capture --text "xsht" lint --fix $candidate
    assert ! attempted.status.exited_with(0), attempted.stderr
    assert candidate.read_text()? == source
  }
}

test test_error_headers_suspend_in_streams_and_cleanup_on_cancellation { |ctx|
  let output = test.expect(
    ctx,
    r"""
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
    status: 0,
  )?
  assert output.stdout == """guarded_closed
7
"""
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
  shared.write(shared_source)
  for name in ["first", "second"] {
    fp"{directory}/{name}.xsh".write(r"""use shared as shared
proc recover() [] -> Int {
  guard let value = "bad".parse_int() else |_| { return shared.recover() }
  value
}
print recover()
""")
  }

  let fixed = run.capture --text "xsht" lint --fix $directory
  assert fixed.status.exited_with(0), fixed.stderr
  let rewritten = shared.read_text()?
  assert rewritten == shared_source.replace("else |failure| {", with: "else { |failure|")
  let again = run.capture --text "xsht" lint --fix $directory
  assert again.status.exited_with(0), again.stderr
  assert shared.read_text()? == rewritten
}

test test_with_handler_is_rejected_with_the_match_to_write { |ctx|
  for source in [
    """with value = Ok(1) {} else { |failure| print $failure }
""",
    """with value = Ok(1) {
  print $value
}
else { print omitted }
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert ! output.success, source
    assert "parse.with-handler" in output.stderr
    assert "match try" in output.stderr
  }
}
