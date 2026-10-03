test test_defer_blocks_register_lexically_and_read_values_at_cleanup { |ctx|
  let output = test.run_script(ctx, r"""
proc log(message: Str) [] { print $message }
proc exercise() [error] {
  var value = "registered"
  let snapshot = value
  defer log("first")
  defer {
    let local = "block"
    print f"${local}:${value}:${snapshot}"
    true
  }
  value = "cleanup"
  if true {
    defer { print "inner" }
    print "body"
  }
  print "outside"
}
exercise()
""")?
  assert output.success, output.stderr
  output.stdout == "body\ninner\noutside\nblock:cleanup:registered\nfirst\n"
}

test test_defer_blocks_keep_loop_cleanup_local_and_nested_defers_lifo { |ctx|
  let output = test.run_script(ctx, r"""
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
      print f"cleanup:${item}:${count}"
    }
    print f"body:${item}"
    continue when item == 1
    break
  }
  print "done"
}
exercise()
""")?
  assert output.success, output.stderr
  output.stdout == "body:1\ncleanup:1:2\nnested\nbody:2\ncleanup:2:2\nnested\ndone\n"
}

test test_defer_block_failure_stops_its_body_and_keeps_other_actions { |ctx|
  let output = test.run_script(ctx, r"""
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
""")?
  assert ! output.success, output.stderr
  output.stdout == "body\nfailing\nremaining\n"
  "cleanup failure" in output.stderr
}

test test_defer_blocks_preserve_primary_failure_and_report_secondary_cleanup { |ctx|
  let output = test.run_script(ctx, r"""
proc exercise() [error] {
  defer { print "remaining" }
  defer {
    print "failing"
    let _ = "cleanup failure".parse_int()?
  }
  let _ = "primary failure".parse_int()?
}
exercise()?
""")?
  assert ! output.success, output.stderr
  output.stdout == "failing\nremaining\n"
  "primary failure" in output.stderr
  "cleanup failure" in output.stderr
  "cleanup error [" in output.stderr
}

test test_defer_blocks_run_bare_assertions_and_implicit_result_unit { |ctx|
  let output = test.run_script(ctx, r"""
proc failure() [error] {
  let _ = "result cleanup failure".parse_int()?
}
proc exercise() [error] {
  defer { print "remaining" }
  defer { failure() }
  defer {
    print "assertion"
    let failed = false
    failed
  }
}
exercise()?
""")?
  assert ! output.success, output.stderr
  output.stdout == "assertion\nremaining\n"
  "assertion" in output.stderr
  "result cleanup failure" in output.stderr
}

test test_defer_blocks_unwind_started_stream_on_early_consumer_exit { |ctx|
  let output = test.run_script(ctx, r"""
stream values() [] -> Stream[Int] {
  defer { print "outer" }
  for item in [1, 2] {
    defer { print f"inner:${item}" }
    yield item
  }
}
for value in values() {
  print $value
  break
}
print "done"
""")?
  assert output.success, output.stderr
  output.stdout == "1\ninner:1\nouter\ndone\n"
}

test test_defer_blocks_reject_escaping_control_and_check_unselected_effects { |ctx|
  for source in [
    "proc bad() [] { defer { return } }\n",
    "proc bad() [] { while true { defer { break }; break } }\n",
    "proc bad() [] { while true { defer { continue }; break } }\n",
    "stream bad() [] -> Stream[Int] { defer { yield 1 }; yield 2 }\n",
    "proc bad() [] { defer { 1 } }\n",
    "proc bad() [] { defer { let inside = 1 }; print $inside }\n",
    "proc bad() [] { defer { print $later }; let later = 1 }\n",
    "proc bad() [] { if false { defer { fs.remove(p\"unused\")? } } }\n",
  ] {
    let output = test.run_script(ctx, source)?
    assert ! output.success, source
    assert output.stderr != "", source
  }
}

test test_defer_blocks_skip_unregistered_actions_and_continue_expression_failures { |ctx|
  let output = test.run_script(ctx, r"""
proc failure(message: Str) [error] { let _ = message.parse_int()? }
proc exercise() [error] {
  if false { defer { print "unregistered" } }
  defer { print "last" }
  defer failure("second cleanup")
  defer failure("first cleanup")
  print "body"
}
exercise()?
""")?
  assert ! output.success, output.stderr
  output.stdout == "body\nlast\n"
  "first cleanup" in output.stderr
  "second cleanup" in output.stderr
}

test test_defer_blocks_unwind_failed_module_procedure { |ctx|
  let root = test.temp_dir(ctx, name: "defer-module")?
  fp"${root}/cleanup.xsh".write(r"""
##! Cleanup module witness.
## Runs lexical cleanup after failure.
export proc exercise() [error] {
  defer { print "module cleanup" }
  let _ = "module failure".parse_int()?
}
""")?
  let output = test.run_script(ctx, "use cleanup\ncleanup.exercise()?\n", [], {XSH_MODULE_PATH: root.display()})?
  assert ! output.success, output.stderr
  assert output.stdout != "", output.stderr
  output.stdout == "module cleanup\n"
  "module failure" in output.stderr
}

test test_defer_blocks_unwind_return_and_keep_return_value { |ctx|
  let output = test.run_script(ctx, r"""
proc value() [] -> Int {
  defer { print "outer" }
  if true {
    defer { print "inner" }
    return 7
  }
  return 8
}
print ${value()}
""")?
  assert output.success, output.stderr
  output.stdout == "inner\nouter\n7\n"
}

test test_defer_block_force_abort_skips_remaining_actions { |ctx|
  let output = test.run_script(ctx, r"""
proc exercise() [] {
  defer { print "skipped" }
  defer { abort(9, force: true) }
}
exercise()
""")?
  output.status == 9
  output.stdout == ""
}

test test_defer_block_force_abort_during_failure_keeps_force_status { |ctx|
  let output = test.run_script(ctx, r"""
proc exercise() [error] {
  defer { print "skipped" }
  defer { abort(9, force: true) }
  let _ = "primary".parse_int()?
}
exercise()?
""")?
  output.status == 9
  output.stdout == ""
}

test test_defer_block_top_level_force_abort_during_failure { |ctx|
  let output = test.run_script(ctx, r"""
defer { print "skipped" }
defer { abort(9, force: true) }
let _ = "primary".parse_int()?
""")?
  output.status == 9
  output.stdout == ""
}

test test_defer_blocks_reject_delegated_yield_during_checking { |ctx|
  let output = test.run_script(ctx, "stream bad() [] -> Stream[Int] { if false { defer { yield @[1] } }; yield 2 }\nlet _ = bad() |> collect\n")?
  assert ! output.success, output.stderr
  "check.defer-control-flow" in output.stderr
}
