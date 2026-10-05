test test_defer_blocks_register_lexically_and_read_values_at_cleanup { |ctx|
  let output = test.expect(
    ctx,
    r"""
proc log(message: Str) [] { print $message }
proc exercise() [error] {
  var value = "registered"
  let snapshot = value
  defer log("first")
  defer {
    let local = "block"
    print f"{local}:{value}:{snapshot}"
    assert true
  }
  value = "cleanup"
  if true {
    defer { print "inner" }
    print "body"
  }
  print "outside"
}
exercise()
""",
    status: 0,
  )?
  assert output.stdout == """body
inner
outside
block:cleanup:registered
first
"""
}

test test_defer_blocks_keep_loop_cleanup_local_and_nested_defers_lifo { |ctx|
  let output = test.expect(
    ctx,
    r"""
proc exercise() [] {
  for item in [1, 2] {
    defer {
      defer { print "nested" }
      var count = 0
      while count < 2 {
        count += 1
        continue when count == 1
        break
      }
      print f"cleanup:{item}:{count}"
    }
    print f"body:{item}"
    continue when item == 1
    break
  }
  print "done"
}
exercise()
""",
    status: 0,
  )?
  assert output.stdout == """body:1
cleanup:1:2
nested
body:2
cleanup:2:2
nested
done
"""
}

test test_defer_block_failure_stops_its_body_and_keeps_other_actions { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc exercise() [error] {
  defer { print "remaining" }
  defer {
    print "failing"
    let _ = "cleanup failure".parse_int()?
    print "skipped"
  }
  print "body"
}
exercise()?
""",
  )?
  assert ! output.success, output.stderr
  assert output.stdout == """body
failing
remaining
"""
  assert "cleanup failure" in output.stderr
}

test test_defer_blocks_preserve_primary_failure_and_report_secondary_cleanup { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc exercise() [error] {
  defer { print "remaining" }
  defer {
    print "failing"
    let _ = "cleanup failure".parse_int()?
  }
  let _ = "primary failure".parse_int()?
}
exercise()?
""",
  )?
  assert ! output.success, output.stderr
  assert output.stdout == """failing
remaining
"""
  assert "primary failure" in output.stderr
  assert "cleanup failure" in output.stderr
  assert "cleanup error [" in output.stderr
}

test test_defer_blocks_run_assertions_and_implicit_result_unit { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc failure() [error] {
  let _ = "result cleanup failure".parse_int()?
}
proc exercise() [error] {
  defer { print "remaining" }
  defer { failure() }
  defer {
    print "assertion"
    let failed = false
    assert failed
  }
}
exercise()?
""",
  )?
  assert ! output.success, output.stderr
  assert output.stdout == """assertion
remaining
"""
  assert "assertion" in output.stderr
  assert "result cleanup failure" in output.stderr
}

test test_defer_blocks_unwind_started_stream_on_early_consumer_exit { |ctx|
  let output = test.expect(
    ctx,
    r"""
stream values() [] -> Stream[Int] {
  defer { print "outer" }
  for item in [1, 2] {
    defer { print f"inner:{item}" }
    yield item
  }
}
for value in values() {
  print $value
  break
}
print "done"
""",
    status: 0,
  )?
  assert output.stdout == """1
inner:1
outer
done
"""
}

test test_defer_blocks_reject_escaping_control_and_check_unselected_effects { |ctx|
  for source in [
    """proc bad() [] { defer { return } }
""",
    """proc bad() [] { while true { defer { break }; break } }
""",
    """proc bad() [] { while true { defer { continue }; break } }
""",
    """stream bad() [] -> Stream[Int] { defer { yield 1 }; yield 2 }
""",
    """proc bad() [] { defer { 1 } }
""",
    """proc bad() [] { defer { let inside = 1 }; print $inside }
""",
    """proc bad() [] { defer { print $later }; let later = 1 }
""",
    """proc bad() [] { if false { defer { fs.remove(p"unused")? } } }
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert ! output.success, source
    assert output.stderr != "", source
  }
}

test test_defer_blocks_skip_unregistered_actions_and_continue_expression_failures { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc failure(message: Str) [error] { let _ = message.parse_int()? }
proc exercise() [error] {
  if false { defer { print "unregistered" } }
  defer { print "last" }
  defer failure("second cleanup")
  defer failure("first cleanup")
  print "body"
}
exercise()?
""",
  )?
  assert ! output.success, output.stderr
  assert output.stdout == """body
last
"""
  assert "first cleanup" in output.stderr
  assert "second cleanup" in output.stderr
}

test test_defer_blocks_unwind_failed_module_procedure { |ctx|
  let root = test.temp_dir(ctx, name: "defer-module")?
  fp"{root}/cleanup.xsh".write(r"""
##! Cleanup module witness.
## Runs lexical cleanup after failure.
export proc exercise() [error] {
  defer { print "module cleanup" }
  let _ = "module failure".parse_int()?
}
""")
  let output = test.run_script(
    ctx,
    """use cleanup
cleanup.exercise()?
""",
    [],
    {XSH_MODULE_PATH: root},
  )?
  assert ! output.success, output.stderr
  assert output.stdout != "", output.stderr
  assert output.stdout == """module cleanup
"""
  assert "module failure" in output.stderr
}

# A script's top level is a scope like any other: however it fails, the
# actions it registered run, last first, and later ones were never registered.
# An imported module has no top-level actions to run (`check.module-top-level`).
test test_defer_blocks_at_the_top_level_run_however_the_script_fails { |ctx|
  let root = test.temp_dir(ctx, name: "defer-top-level")?
  fp"{root}/helper.xsh".write(r"""
##! Top-level cleanup witness.
## Fails after its caller registered cleanup.
export proc boom() [error] {
  let _ = "helper failure".parse_int()?
}
""")
  let failures = [
    "error.fail(\"statement\")",
    "error.fail(\"propagated\")?",
    "run false",
    "assert 1 + 1 == 3",
    "let items = [1]\nlet index = items.len() + 2\nprint f\"{items[index]}\"",
    "proc local() [error] {\n  error.fail(\"in a proc\")\n}\nlocal()",
    "helper.boom()",
    "for _ in [1] {\n  defer { print \"inner\" }\n  error.fail(\"in a loop\")\n}",
    "exit 7",
  ]
  for failure in failures {
    let output = test.run_script(
      ctx,
      f"""use helper
defer {{ print "first" }}
errdefer {{ print "on error" }}
defer {{ print "last" }}
print "body"
{failure}
defer {{ print "never registered" }}
print "never"
""",
      [],
      {XSH_MODULE_PATH: root},
    )?
    assert output.status != 0, failure
    let inner = if "in a loop" in failure { "inner\n" } else { "" }
    assert output.stdout == f"body\n{inner}last\non error\nfirst\n", f"{failure}: {output.stdout}{output.stderr}"
  }

  fp"{root}/registers.xsh".write(r"""
##! A module that tries to register top-level cleanup.
defer { print "module cleanup" }

## Does nothing.
export proc noop() {}
""")
  let imported = test.run_script(
    ctx,
    """use registers
registers.noop()
""",
    [],
    {XSH_MODULE_PATH: root},
  )?
  assert imported.status != 0
  assert "check.module-top-level" in imported.stderr, imported.stderr
  assert imported.stdout == ""
}

test test_defer_blocks_at_the_top_level_run_when_cli_main_fails { |ctx|
  let output = test.run_script(
    ctx,
    """defer { print "first" }
errdefer { print "on error" }
print "body"

cli main() [error] {
  defer { print "main" }
  error.fail("main failed")
}
""",
  )?
  assert output.status != 0
  assert output.stdout == "body\nmain\non error\nfirst\n", output.stdout + output.stderr
}

test test_defer_blocks_unwind_return_and_keep_return_value { |ctx|
  let output = test.expect(
    ctx,
    r"""
proc value() [] -> Int {
  defer { print "outer" }
  if true {
    defer { print "inner" }
    return 7
  }
  return 8
}
print ${value()}
""",
    status: 0,
  )?
  assert output.stdout == """inner
outer
7
"""
}

test test_defer_blocks_reject_delegated_yield_during_checking { |ctx|
  let output = test.run_script(
    ctx,
    """stream bad() [] -> Stream[Int] { if false { defer { yield @[1] } }; yield 2 }
let _ = bad() |> collect
""",
  )?
  assert ! output.success, output.stderr
  assert "check.defer-control-flow" in output.stderr
}

test test_defer_block_restores_mutable_capture_types { |ctx|
  # A refinement of a mutable binding cannot survive registration.
  let _ = test.expect(
    ctx,
    r"""proc work(input: Str?) [] { var value: Str? = input; if value != null { defer { let text: Str = value; print $text } } }
""",
    status: 2,
    stderr: ["[check."],
  )?

  let immutable = test.expect(
    ctx,
    r"""proc work(input: Str?) [] { let value: Str? = input; if value != null { defer { let text: Str = value; print $text } } }
""",
    status: 0,
  )?
  assert immutable.stderr == "", immutable.stderr
}

test test_defer_block_rejects_yield_delegation { |ctx|
  let _ = test.expect(
    ctx,
    "stream values() [] -> Stream[Int] { if false { defer { yield @[1] } }; yield 2 }\n",
    status: 2,
    stderr: ["[check.defer-control-flow]"],
  )?
}
