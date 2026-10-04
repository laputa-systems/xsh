test test_accept_scoped_delegation_restores_consumer_and_cancels_child { |ctx|
  let root = test.temp_dir(ctx, name: "accept-scoped-stream")?
  let output = test.run_script(
    ctx,
    r"""
stream scoped(root: Path) [fs, env, process, error] -> Stream[Str] {
  let overlay = env ({XSH_ACCEPT_SCOPE: "inner"}) {
    let directory = cd (root) {
      defer {
        test.eq(fs.cwd()?, root)?
        print f"cleanup:{env.get("XSH_ACCEPT_SCOPE")?}"
      }
      let rows = run.stream --text --accept=[0] sh -c "printf yes > entered; printenv XSH_ACCEPT_SCOPE; sleep 1; printf leaked > leaked" ?
      yield @ rows
      7
    }?
    7
  }?
}
proc main(...argv: List[Str]) [fs, env, process, time, error] {
  let root = Path(argv[0])
  let original = fs.cwd()?
  for row in scoped(root) {
    test.eq(fs.cwd()?, original)?
    test.eq(env.get("XSH_ACCEPT_SCOPE")?, "consumer")?
    print $row
    break
  }
  test.eq(fs.cwd()?, original)?
  test.eq(env.get("XSH_ACCEPT_SCOPE")?, "consumer")?
  time.sleep(1200ms)?
  test.eq(fp"{root}/entered".read_text()?, "yes")?
  print ${fp"{root}/leaked".exists()?}
}
""",
    [root.display()],
    env: {XSH_ACCEPT_SCOPE: "consumer"},
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """inner
cleanup:inner
false
"""
}

test test_accept_scoped_rejected_eof_runs_cleanup_once_before_restoration { |ctx|
  let root = test.temp_dir(ctx, name: "accept-scoped-rejection")?
  let output = test.run_script(
    ctx,
    r"""
stream scoped(root: Path) [fs, env, process, error] -> Stream[Str] {
  let overlay = env ({XSH_ACCEPT_SCOPE: "inner"}) {
    let directory = cd (root) {
      defer {
        test.eq(fs.cwd()?, root)?
        print f"cleanup:{env.get("XSH_ACCEPT_SCOPE")?}"
      }
      let rows = run.stream --text --accept=[1] sh -c "printenv XSH_ACCEPT_SCOPE; exit 0" ?
      yield @ rows
      7
    }?
    7
  }?
}
proc main(...argv: List[Str]) [fs, env, process, error] {
  let root = Path(argv[0])
  let original = fs.cwd()?
  let outcome = try {
    for row in scoped(root) {
      test.eq(fs.cwd()?, original)?
      test.eq(env.get("XSH_ACCEPT_SCOPE")?, "consumer")?
      print $row
    }
  }
  match outcome {
    Err(ProcessError.UnexpectedExit {status: child_status}) => {
      guard child_status != null else { abort(99) }
      test.ok(child_status.ok)?
      test.eq(child_status.exit_code()?, 0)?
      print "rejected zero"
    }
    _ => abort(98)
  }
  test.eq(fs.cwd()?, original)?
  test.eq(env.get("XSH_ACCEPT_SCOPE")?, "consumer")?
}
""",
    [root.display()],
    env: {XSH_ACCEPT_SCOPE: "consumer"},
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """inner
cleanup:inner
rejected zero
"""
}

test test_accept_named_stage_direct_rejection_stops_before_next_item { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc validate(item: Int) [process, error] -> Int {
  print f"seen:{item}"
  let status = run.status --accept=[1] sh -c "exit 0"
  item
}
let outcome = try { let values = [1, 2] |> map(validate) }
match outcome {
  Err(ProcessError.UnexpectedExit {status: child_status}) => {
    guard child_status != null else { abort(99) }
    test.ok(child_status.ok)?
    test.eq(child_status.exit_code()?, 0)?
    print "rejected zero"
  }
  _ => abort(98)
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """seen:1
rejected zero
"""
}

test test_accept_named_stage_result_data_and_sink_propagation_stay_distinct { |ctx|
  let callback = r"""
proc validate(item: Int) [process] -> Result[Unit, ProcessError] {
  print f"seen:{item}"
  try { let status = run.status --accept=[1] sh -c "exit 0" }
}
"""
  let mapped = test.run_script(
    ctx,
    callback + r"""
let values = [1, 2] |> map(validate)
print ${values.len()}
print ${values[0] is Err(ProcessError.UnexpectedExit)}
print ${values[1] is Err(ProcessError.UnexpectedExit)}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = mapped
    assert assertion_condition, assertion_message
  }
  assert mapped.stdout == """seen:1
seen:2
2
true
true
"""
  for stage in ["each", "tee"] {
    let output = test.run_script(
      ctx,
      callback + r"""
stream items() [] -> Stream[Int] {
  defer { print "cleanup" }
  yield 1
  yield 2
}
let ignored = items() |> """ + stage + """(validate)
print unreachable
""",
    )?
    {
      let assertion_condition = ! output.success
      let assertion_message = output.stderr
      assert assertion_condition, assertion_message
    }
    assert output.stdout == """seen:1
cleanup
"""
    assert "unexpected-exit" in output.stderr
  }
}

test test_accept_typed_causes_preserve_rejected_zero_status_and_spans { |ctx|
  for body in [
    """let status = run.status --accept=[1] sh -c "exit 0"
""",
    """let rows = run.stream --text --accept=[1] sh -c "printf 'row\\n'; exit 0" ?
for row in rows { print $row }
""",
  ] {
    let output = test.run_xsht_trace(
      ctx,
      r"""
error OuterError = Failed(message: Str)
let original: Result[Unit, ProcessError] = try {
  ctx "rejected input" {
""" + body + r"""
  }
}
print "captured"
let translated: Result[Unit, OuterError] = match original {
  Err(failure) => {
    match failure {
      ProcessError.UnexpectedExit {status: child_status} => {
        guard child_status != null else { abort(99) }
        test.ok(child_status.ok)?
        test.eq(child_status.exit_code()?, 0)?
      }
      _ => abort(98)
    }
    Err(OuterError.Failed(message: "translated"), cause: failure)
  }
  _ => Err(OuterError.Failed(message: "completion unexpectedly accepted"))
}
let transported: Result[Unit, OuterError] = try { ctx "transport" { translated? } }
match transported {
  Err(failure) => ctx "publish" { Err(failure)? }
  _ => abort(96)
}
""",
      ["--raw", "--trace-format", "jsonl"],
    )?
    {
      let assertion_condition = output.status == 3
      let assertion_message = output.stderr
      assert assertion_condition, assertion_message
    }
    let expected = if "run.stream" in body {
      """row
captured
"""
    } else {
      """captured
"""
    }
    assert output.stdout == expected
    assert "\"family\":\"OuterError\"" in output.stderr
    assert "\"family\":\"ProcessError\"" in output.stderr
    assert "\"variant\":\"UnexpectedExit\"" in output.stderr
    assert "\"code\":0" in output.stderr
    assert "\"success\":true" in output.stderr
    assert "\"message\":\"rejected input\"" in output.stderr
    assert "\"message\":\"transport\"" in output.stderr
    assert "\"message\":\"publish\"" in output.stderr
    assert "\"start_line\"" in output.stderr
    assert "\"causes_truncated\":false" in output.stderr
  }
}

test test_accept_callable_alias_defaults_evaluate_once_before_each_child { |ctx|
  let root = test.temp_dir(ctx, name: "accept-alias-policy-order")?
  let output = test.run_script(
    ctx,
    r"""
proc codes(marker: Path, accepted: List[Int] = [0, 1]) [fs, error] -> List[Int] {
  marker.write("child")?
  var copy = accepted
  copy += [2]
  print f"policy:{copy.len()}"
  copy
}
let select = codes
let again = select
proc main(...argv: List[Str]) [fs, process, error] {
  let root = Path(argv[0])
  let one = fp"{root}/one"
  let two = fp"{root}/two"
  let three = fp"{root}/three"
  print "created"
  let first = run.text --accept=again(marker: one) cat (one) ?
  print $first
  let second = run.text --accept=again(marker: two, accepted: [0, 1]) cat (two) ?
  print $second
  let third = run.text --accept=select(marker: three) cat (three) ?
  print $third
}
""",
    [root.display()],
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """created
policy:3
child
policy:3
child
policy:3
child
"""
}

test test_accept_callable_alias_retains_inferred_error_and_local_capture { |ctx|
  let declaration = r"""
proc validate() {
  let status = run.status --accept=[1] sh -c "exit 0"
}
let selected = validate
"""
  let denied = test.run_script(
    ctx,
    declaration + """\nproc main() [process] { selected() }
""",
  )?
  assert ! denied.success
  assert "check.effect-violation" in denied.stderr
  let captured = test.run_script(
    ctx,
    declaration + "\n" + r"""
proc main() [process] -> Result[Unit] {
  try { selected() }
}
""",
  )?
  {
    let assertion_condition = ! captured.success
    let assertion_message = captured.stderr
    assert assertion_condition, assertion_message
  }
  assert "unexpected-exit" in captured.stderr
  {
    let assertion_condition = "check.effect-violation" not in captured.stderr
    let assertion_message = captured.stderr
    assert assertion_condition, assertion_message
  }
}
