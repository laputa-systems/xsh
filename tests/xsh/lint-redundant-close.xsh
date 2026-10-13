# This default-on rule uses checked binding identities and local resource types.
test test_redundant_close_is_default_enabled_and_fixable { |ctx|
  let file = test.temp_file(
    ctx,
    name: "root-close.xsh",
    contents: bytes.from_text(
      "proc inspect(dir: Path) [fs, error] -> Result[Bool] {\n  let root = fs.open_root(dir)?\n  defer root.close()\n  root.exists(p\".\")\n}\nprint inspect(p\".\")?\n",
    ),
  )?
  let reported = run.capture --text --accept=[0, 1] "xsht" lint $file
  assert reported.status.exited_with(1), reported.stderr
  assert "warn[lint.redundant-close]" in reported.stderr, reported.stderr

  let fixed = run.capture --text "xsht" lint --fix $file
  assert fixed.status.exited_with(0), fixed.stderr
  let text = file.read_text()?
  assert "defer root.close()" not in text, text
  assert_checked(file)

  let clean = run.capture --text "xsht" lint $file
  assert clean.status.exited_with(0), clean.stderr
  assert "lint.redundant-close" not in clean.stderr, clean.stderr
}

test test_redundant_close_only_fix_removes_the_whole_line { |ctx|
  let source = """proc inspect(dir: Path) [fs, error] -> Result[Bool] {
  let root = fs.open_root(dir)?
  defer root.close()
  root.exists(p".")
}
print inspect(p".")?
"""
  let file = test.temp_file(ctx, name: "root-close-only.xsh", contents: bytes.from_text(source))?
  let fixed = run.capture --text "xsht" lint --only lint.redundant-close --fix $file
  assert fixed.status.exited_with(0), fixed.stderr

  let expected = source.replace("  defer root.close()\n", with: "")
  assert file.read_text()? == expected, file.read_text()?
}

proc assert_checked(file: Path) [process, error] {
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
}
