use version

test parses_semver { |ctx|
  let v = version.parse("1.2.3")?
  assert v.major == 1
  assert v.minor == 2, f"minor of {ctx.name}"
}

test runs_script [fs, process, error] { |ctx|
  let result = test.run_script(ctx, "print \"hi\"")?
  assert result.stdout == "hi\n"
}
