test guarded_return_preserves_the_reaching_branch_proof [fs, process, error] { |ctx|
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
    let checked = run.capture --text "xsht" check $file ?
    let {status: checked_status, stderr: checked_diagnostics, ..} = checked
    assert checked_status.exited_with(0), checked_diagnostics
    let result = test.run_script(ctx, source)?
    let {success: succeeded, stderr: diagnostics, stdout: output, ..} = result
    assert succeeded, diagnostics
    output == "missing\nready\n"
  }
}

test guarded_return_preserves_boolean_alias_and_unless_proofs [fs, process, error] { |ctx|
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
  let checked = run.capture --text "xsht" check $file ?
  let {status: checked_status, stderr: checked_diagnostics, ..} = checked
  assert checked_status.exited_with(0), checked_diagnostics
  let result = test.run_script(ctx, source)?
  let {success: succeeded, stderr: diagnostics, stdout: output, ..} = result
  assert succeeded, diagnostics
  output == "missing\nready\n"
}

test guarded_loop_exits_preserve_only_the_reaching_iteration_proof [fs, process, error] { |ctx|
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
  let checked = run.capture --text "xsht" check $file ?
  let {status: checked_status, stderr: checked_diagnostics, ..} = checked
  assert checked_status.exited_with(0), checked_diagnostics
  let result = test.run_script(ctx, source)?
  let {success: succeeded, stderr: diagnostics, stdout: output, ..} = result
  assert succeeded, diagnostics
  output == "first\nlast\nfirst\n"
}

test guarded_yield_and_invalid_targets_do_not_establish_continuation_proofs [fs, process, error] { |ctx|
  for source in [
    "stream values(raw: Str?) [] -> Stream[Str] { yield \"missing\" when raw == null; yield raw }\n",
    "let raw: Str? = null\nreturn \"missing\" when raw == null\nlet selected: Str = raw\n",
    "let raw: Str? = null\nbreak when raw == null\nlet selected: Str = raw\n",
    "let raw: Str? = null\ncontinue when raw == null\nlet selected: Str = raw\n",
  ] {
    let file = test.temp_file(ctx, name: "invalid-guarded-proof.xsh", contents: bytes.from_text(source))?
    let checked = run.capture --text "xsht" check $file ?
    let {status: checked_status, stderr: diagnostics, ..} = checked
    assert ! checked_status.exited_with(0), source
    let rejected_receiver = "Str?" in diagnostics
    assert rejected_receiver, diagnostics
  }
}

test guarded_return_keeps_the_continuation_binding_after_exiting_mutation [fs, process, error] { |ctx|
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
  let checked = run.capture --text "xsht" check $file ?
  let {status: checked_status, stderr: checked_diagnostics, ..} = checked
  assert checked_status.exited_with(0), checked_diagnostics
  let result = test.run_script(ctx, source)?
  let {success: succeeded, stderr: diagnostics, stdout: output, ..} = result
  assert succeeded, diagnostics
  output == "ready\n ready \n"
}
