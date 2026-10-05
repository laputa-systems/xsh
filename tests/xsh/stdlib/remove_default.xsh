test test_explicit_missing_ok_lint_writes_the_default_of_remove { |ctx|
  let root = test.temp_dir(ctx, name: "remove-default")?
  let source = r"""proc clean(stale: Path) [fs, error] -> Result[Str] {
  stale.remove()
  Ok("removed")
}

let present = p"ROOT/present"
present.write("text")
print (clean(present)?) (present.exists()?)
print (clean(present)?)
""".replace("ROOT", root.display())
  let candidate = test.temp_file(ctx, name: "remove-default.xsh", contents: bytes.from_text(source))?

  # The rule is a migration aid: an ordinary lint run does not report it.
  let ordinary = run.capture --text "xsht" lint $candidate ?
  assert "lint.explicit-missing-ok" not in ordinary.stderr, ordinary.stderr

  let applied = run.capture --text "xsht" lint --only lint.explicit-missing-ok --fix $candidate ?
  assert applied.status.exited_with(0), applied.stderr
  let fixed = candidate.read_text()?
  assert fixed == source.replace("stale.remove()", "stale.remove(missing_ok: false)")
  assert fixed != source

  # Written out, the default is what the call already did: the present path
  # is removed and the missing one is still an error.
  let before = test.run_script(ctx, source)?
  let after = test.run_script(ctx, fixed)?
  assert before.stdout == "removed false\n"
  assert after.stdout == before.stdout
  assert ! before.success
  assert after.status == before.status
  assert "fs-remove" in before.stderr, before.stderr
  assert "fs-remove" in after.stderr, after.stderr

  let repeated = run.capture --text "xsht" lint --only lint.explicit-missing-ok $candidate ?
  assert repeated.status.exited_with(0)
  assert "lint.explicit-missing-ok" not in repeated.stderr
}
