test test_labeled_arguments_read_as_a_sentence { |ctx|
  let root = test.temp_dir(ctx, name: "argument-label")?
  let file = fp"{root}/file"
  file.write("text")

  let copied = fp"{root}/copied"
  file.copy(to: copied)
  file.copy(overwrite: true, to: copied)
  assert copied.read_text()? == "text"

  let moved = fp"{root}/moved"
  copied.rename(to: moved)
  assert ! copied.exists()?
  assert moved.read_text()? == "text"

  # The receiver of `hardlink` is the file that exists; the receiver of
  # `symlink` is the link.
  let hard = fp"{root}/hard"
  file.hardlink(at: hard)
  assert hard.read_text()? == "text"
  let link = fp"{root}/link"
  link.symlink(to: file)
  assert link.is_symlink()?
  assert link.readlink()? == file
  assert link.read_text()? == "text"

  # The target is stored as written and need not exist.
  let dangling = fp"{root}/dangling"
  dangling.symlink(to: "../absent")
  assert dangling.readlink()? == ../absent
  test.error_kind(dangling.symlink(to: file), "fs-symlink")

  assert "a-b-c".replace("-", with: "+") == "a+b+c"
  assert "a-b-c".replace(with: "+", from: "-") == "a+b+c"
  assert "a-b-c".replace(...{from: "-", with: ""}) == "abc"
  assert rx"-+".replace("a--b-c", with: " ") == "a b c"
  let maybe: Str? = "a-b"
  assert (maybe?.replace("-", with: "") ?? "absent") == "ab"
}

test test_former_argument_spellings_still_check_and_mean_the_same { |ctx|
  let root = test.temp_dir(ctx, name: "argument-label-former")?
  let source = r"""let root = p"ROOT"
let file = fp"{root}/file"
file.write("text")?
file.copy(fp"{root}/by-position")?
file.copy(dest: fp"{root}/by-name", overwrite: true)?
fp"{root}/by-name".rename(dest: fp"{root}/renamed")?
file.hardlink(fp"{root}/hard-by-position")?
file.hardlink(path: fp"{root}/hard-by-name")?
fs.symlink(file, fp"{root}/link")?
for name in ["by-position", "renamed", "hard-by-position", "hard-by-name", "link"] {
  print (fp"{root}/{name}".read_text()?)
}
print ("a-b".replace("-", "+")) ("a-b".replace(from: "-", to: "+")) ("a-b".replace(to: "+", from: "-"))
print ("a-b".replace(...{from: "-", to: "+"}))
print (rx"-".replace("a-b", "+")) (rx"-".replace("a-b", replacement: "+"))
""".replace("ROOT", with: root.display())
  let _ = test.expect(ctx, source, status: 0, stdout: ["text\ntext\ntext\ntext\ntext\na+b a+b a+b\na+b\na+b a+b\n"])?
}

test test_an_unknown_label_is_still_rejected { |ctx|
  let checked = test.run_script(
    ctx,
    r"""print ("a-b".replace("-", by: "+"))
""",
  )?
  assert ! checked.success
  assert "check.named-arg" in checked.stderr, checked.stderr
  let twice = test.run_script(
    ctx,
    r"""print ("a-b".replace("-", with: "+", to: "*"))
""",
  )?
  assert ! twice.success
  assert "check." in twice.stderr, twice.stderr
}

test test_prefer_argument_label_lint_writes_the_label { |ctx|
  let root = test.temp_dir(ctx, name: "argument-label-lint")?
  let source = r"""proc stage(root: Path, name: Str) [fs, error] {
  let file = fp"{root}/{name}"
  file.write(name.replace("-", "+"))
  file.copy(fp"{root}/copied")
  file.copy(dest: fp"{root}/copied", overwrite: true)
  fp"{root}/copied".rename(fp"{root}/renamed")
  file.hardlink(fp"{root}/hard")
  fs.symlink(file, fp"{root}/link")
  fs.symlink("../absent", fp"{root}/dangling")
  let restored = rx"\+".replace(fp"{root}/hard".read_text()?, "-")
  let nested = name.replace(from: "-", to: "/")
  print (fp"{root}/link".read_text()?) (fp"{root}/dangling".readlink()?) $restored $nested
}

stage(p"ROOT", "a-b")
"""
  let expected = r"""proc stage(root: Path, name: Str) [fs, error] {
  let file = fp"{root}/{name}"
  file.write(name.replace("-", with: "+"))
  file.copy(to: fp"{root}/copied")
  file.copy(to: fp"{root}/copied", overwrite: true)
  fp"{root}/copied".rename(to: fp"{root}/renamed")
  file.hardlink(at: fp"{root}/hard")
  fp"{root}/link".symlink(to: file)
  fp"{root}/dangling".symlink(to: "../absent")
  let restored = rx"\+".replace(fp"{root}/hard".read_text()?, with: "-")
  let nested = name.replace(from: "-", with: "/")
  print (fp"{root}/link".read_text()?) (fp"{root}/dangling".readlink()?) $restored $nested
}

stage(p"ROOT", "a-b")
"""
  let candidate = test.temp_file(ctx, name: "argument-label.xsh", contents: bytes.from_text(source))?
  let applied = run.capture --text "xsht" lint --only lint.prefer-argument-label --fix $candidate ?
  assert applied.status.exited_with(0), applied.stderr
  assert candidate.read_text()? == expected

  let formatted = run.capture --text "xsht" fmt --check $candidate ?
  assert formatted.status.exited_with(0), formatted.stderr
  let repeated = run.capture --text "xsht" lint --only lint.prefer-argument-label $candidate ?
  assert repeated.status.exited_with(0)
  assert "lint.prefer-argument-label" not in repeated.stderr

  # The fixed program does what the original did, each in its own directory.
  let before = fp"{root}/before"
  let after = fp"{root}/after"
  before.mkdir()
  after.mkdir()
  let output = "a+b ../absent a-b a/b\n"
  let _ = test.expect(ctx, source.replace("ROOT", with: before.display()), status: 0, stdout: [output])?
  let _ = test.expect(ctx, expected.replace("ROOT", with: after.display()), status: 0, stdout: [output])?
}

test test_prefer_argument_label_lint_keeps_an_observable_order { |ctx|
  let source = r"""proc link_for(root: Path) [io] -> Path {
  print "link"
  fp"{root}/link"
}

proc stage(root: Path, target: Path) [io, fs, error] {
  fs.symlink(target.resolve()?, link_for(root))?
}
"""
  let candidate = test.temp_file(ctx, name: "argument-label-order.xsh", contents: bytes.from_text(source))?
  let reported = run.capture --text "xsht" lint --only lint.prefer-argument-label --fix $candidate ?
  assert ! reported.status.exited_with(0)
  assert "lint.prefer-argument-label" in reported.stderr, reported.stderr
  assert candidate.read_text()? == source
}
