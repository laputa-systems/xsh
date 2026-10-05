# `remove` leaves a path gone. Nothing being there is success unless the call
# says `missing_ok: false`.
test test_remove_accepts_a_missing_path_by_default { |ctx|
  let root = test.temp_dir(ctx, name: "remove-default")?
  let file = fp"{root}/file"
  let tree = fp"{root}/tree"
  file.write("text")
  fp"{tree}/below".mkdir()
  fp"{tree}/below/leaf".write("leaf")

  file.remove()
  tree.remove()
  assert ! file.exists()?
  assert ! tree.exists()?

  # Removing what is already gone succeeds.
  file.remove()
  tree.remove()
  assert file.remove() is Ok(_)
}

test test_remove_with_missing_ok_false_fails_on_a_missing_path { |ctx|
  let root = test.temp_dir(ctx, name: "remove-strict")?
  let file = fp"{root}/file"
  file.write("text")

  # What is there is removed either way.
  file.remove(missing_ok: false)
  assert ! file.exists()?

  assert file.remove(missing_ok: false) is Err(_)
  let strict = strict_flag()
  assert file.remove(missing_ok: strict) is Err(_)
}

# The function and command spellings have the same default, and writing the
# default out changes nothing.
test test_every_spelling_of_remove_has_the_same_default { |ctx|
  let root = test.temp_dir(ctx, name: "remove-spellings")?
  let script = r"""let gone = p"ROOT/gone"
gone.remove(missing_ok: true)?
fs.remove(gone)?
fs.remove(gone, missing_ok: true)?
fs.remove $gone
fs.remove $gone --missing-ok
print (fs.remove(gone, missing_ok: false) is Err(_))
""".replace("ROOT", with: root.display())
  let _ = test.expect(ctx, script, status: 0, stdout: ["true\n"])?
}

# With the default written out the call is the same call, so the lint removes
# the argument and the program behaves as before.
test test_redundant_default_lint_removes_missing_ok_true { |ctx|
  let root = test.temp_dir(ctx, name: "remove-redundant")?
  let source = r"""proc clean(stale: Path) [fs, error] -> Result[Str] {
  stale.remove(missing_ok: true)
  Ok("removed")
}

let present = p"ROOT/present"
present.write("text")
print (clean(present)?) (present.exists()?)
print (clean(present)?)
""".replace("ROOT", with: root.display())
  let candidate = test.temp_file(ctx, name: "remove-redundant.xsh", contents: bytes.from_text(source))?

  let applied = run.capture --text "xsht" lint --only lint.redundant-default --fix $candidate
  assert applied.status.exited_with(0), applied.stderr
  let fixed = candidate.read_text()?
  assert fixed == source.replace("stale.remove(missing_ok: true)", with: "stale.remove()")
  assert fixed != source

  let before = test.run_script(ctx, source)?
  let after = test.run_script(ctx, fixed)?
  assert before.success, before.stderr
  assert before.stdout == "removed false\nremoved\n"
  assert after.stdout == before.stdout
  assert after.status == before.status

  let repeated = run.capture --text "xsht" lint --only lint.redundant-default $candidate
  assert repeated.status.exited_with(0)
  assert "lint.redundant-default" not in repeated.stderr
}

pure strict_flag() -> Bool {
  false
}
