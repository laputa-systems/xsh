test guarded_return_preserves_the_reaching_branch_proof { |ctx|
  let before = r"""pure read(raw: Str?) -> Str {
  if raw == null { return "missing" }
  let selected: Str = raw
  selected.trim()
}
print ${read(null)}
print ${read(" ready ")}
"""
  let after = r"""pure read(raw: Str?) -> Str {
  return "missing" when raw == null
  let selected: Str = raw
  selected.trim()
}
print ${read(null)}
print ${read(" ready ")}
"""
  for source in [before, after] {
    let file = test.temp_file(ctx, name: "guarded-proof.xsh", contents: bytes.from_text(source))?
    let checked = run.capture --text "xsht" check $file
    let {status: checked_status, stderr: checked_diagnostics, ..} = checked
    assert checked_status.exited_with(0), checked_diagnostics
    let result = test.run_script(ctx, source)?
    let {success: succeeded, stderr: diagnostics, stdout: output, ..} = result
    assert succeeded, diagnostics
    assert output == """missing
ready
"""
  }
}

test guarded_return_preserves_boolean_alias_and_unless_proofs { |ctx|
  let source = r"""type Item = {raw: Str?}
pure read(item: Item) -> Str {
  let available = item.raw != null
  let retained = available
  return "missing" unless retained
  let selected: Str = item.raw
  selected.trim()
}
print ${read({raw: null})}
print ${read({raw: " ready "})}
"""
  let file = test.temp_file(ctx, name: "guarded-proof.xsh", contents: bytes.from_text(source))?
  let checked = run.capture --text "xsht" check $file
  let {status: checked_status, stderr: checked_diagnostics, ..} = checked
  assert checked_status.exited_with(0), checked_diagnostics
  let result = test.run_script(ctx, source)?
  let {success: succeeded, stderr: diagnostics, stdout: output, ..} = result
  assert succeeded, diagnostics
  assert output == """missing
ready
"""
}

test guarded_loop_exits_preserve_only_the_reaching_iteration_proof { |ctx|
  let source = r"""let continued: List[Str?] = [null, " first ", " last "]
for raw in continued {
  continue when raw == null
  let selected: Str = raw
  print ${selected.trim()}
}
let stopped: List[Str?] = [" first ", null, " unreachable "]
for raw in stopped {
  break unless raw != null
  let selected: Str = raw
  print ${selected.trim()}
}
"""
  let file = test.temp_file(ctx, name: "guarded-proof.xsh", contents: bytes.from_text(source))?
  let checked = run.capture --text "xsht" check $file
  let {status: checked_status, stderr: checked_diagnostics, ..} = checked
  assert checked_status.exited_with(0), checked_diagnostics
  let result = test.run_script(ctx, source)?
  let {success: succeeded, stderr: diagnostics, stdout: output, ..} = result
  assert succeeded, diagnostics
  assert output == """first
last
first
"""
}

test guarded_yield_and_invalid_targets_do_not_establish_continuation_proofs { |ctx|
  for source in [
    """stream values(raw: Str?) [] -> Stream[Str] { yield "missing" when raw == null; yield raw }
""",
    """let raw: Str? = null
return "missing" when raw == null
let selected: Str = raw
""",
    """let raw: Str? = null
break when raw == null
let selected: Str = raw
""",
    """let raw: Str? = null
continue when raw == null
let selected: Str = raw
""",
  ] {
    let file = test.temp_file(ctx, name: "invalid-guarded-proof.xsh", contents: bytes.from_text(source))?
    let checked = run.capture --text "xsht" check $file
    let {status: checked_status, stderr: diagnostics, ..} = checked
    assert ! checked_status.exited_with(0), source
    let rejected_receiver = "Str?" in diagnostics
    assert rejected_receiver, diagnostics
  }
}

test guarded_return_keeps_the_continuation_binding_after_exiting_mutation { |ctx|
  let source = r"""var raw: Str? = " ready "
proc mutate() [] -> Str { raw = null; "changed" }
proc read() [] -> Str {
  return mutate() when raw == null
  let selected: Str = raw
  selected.trim()
}
print ${read()}
print ${raw ?? "missing"}
"""
  let file = test.temp_file(ctx, name: "guarded-proof.xsh", contents: bytes.from_text(source))?
  let checked = run.capture --text "xsht" check $file
  let {status: checked_status, stderr: checked_diagnostics, ..} = checked
  assert checked_status.exited_with(0), checked_diagnostics
  let result = test.run_script(ctx, source)?
  let {success: succeeded, stderr: diagnostics, stdout: output, ..} = result
  assert succeeded, diagnostics
  assert output == """ready
 ready 
"""
}

test guarded_expression_break_analysis_preserves_valid_loop_execution { |ctx|
  let source = r"""pure identity(value: Int) -> Int { value }
pure choose() -> Int {
  while true {
    return identity(if false { break } else { 17 })
  }
}
print ${choose()}
"""
  let file = test.temp_file(ctx, name: "guarded-expression-flow.xsh", contents: bytes.from_text(source))?
  let checked = run.capture --text "xsht" check $file
  let {status: checked_status, stderr: checked_diagnostics, ..} = checked
  assert checked_status.exited_with(0), checked_diagnostics
  let result = test.run_script(ctx, source)?
  let {success: succeeded, stderr: diagnostics, stdout: output, ..} = result
  assert succeeded, diagnostics
  assert output == """17
"""
}

test guarded_exhaustive_result_and_error_family_returns_lower_and_run { |ctx|
  let source = r"""error FlowFailure {
  Missing
  Changed
}
pure choose_result(value: Result[Int, Error]) -> Int {
  match value {
    Ok(number) => { return number }
    Err(_) => { return 0 }
  }
}
pure choose_failure(failure: FlowFailure) -> Int {
  match failure {
    FlowFailure.Missing => { return 1 }
    FlowFailure.Changed => { return 2 }
  }
}
print ${choose_result(Ok(17))}
print ${choose_failure(FlowFailure.Changed())}
"""
  let file = test.temp_file(ctx, name: "guarded-match-flow.xsh", contents: bytes.from_text(source))?
  let checked = run.capture --text "xsht" check $file
  let {status: checked_status, stderr: checked_diagnostics, ..} = checked
  assert checked_status.exited_with(0), checked_diagnostics
  let result = test.run_script(ctx, source)?
  let {success: succeeded, stderr: diagnostics, stdout: output, ..} = result
  assert succeeded, diagnostics
  assert output == """17
2
"""
}

test guarded_user_defined_range_call_keeps_its_resolved_target { |ctx|
  let source = r"""pure range(end: Int) -> List[Int] { [7] }
print ${range(1)[0]}
"""
  let file = test.temp_file(ctx, name: "user-range-call.xsh", contents: bytes.from_text(source))?
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
  let result = test.run_script(ctx, source)?
  let {success: succeeded, stderr: diagnostics, stdout: output, ..} = result
  assert succeeded, diagnostics
  assert output == """7
"""
}

test guarded_materialized_pipeline_callback_return_runs { |ctx|
  let source = r"""pure choose() -> Int {
  let ignored = [17] |> map { |item| if true { return item }; item } |> collect()
}
print ${choose()}
"""
  let file = test.temp_file(ctx, name: "pipeline-callback-return.xsh", contents: bytes.from_text(source))?
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
  let result = test.run_script(ctx, source)?
  let {success: succeeded, stderr: diagnostics, stdout: output, ..} = result
  assert succeeded, diagnostics
  assert output == """17
"""
}
