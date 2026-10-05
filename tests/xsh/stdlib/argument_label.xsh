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
  assert "a-b-c".replace(...{from: "-"}, with: "") == "abc"
  assert rx"-+".replace("a--b-c", with: " ") == "a b c"
  let maybe: Str? = "a-b"
  assert (maybe?.replace("-", with: "") ?? "absent") == "ab"
}

test test_a_labeled_argument_by_position_is_rejected { |ctx|
  let positional = [
    r"""print ("a-b".replace("-", "+"))""",
    r"""print ("a-b".replace(...{from: "-"}, "+"))""",
    r"""print (rx"-".replace("a-b", "+"))""",
    r"""p"/tmp/a".copy(p"/tmp/b")""",
    r"""p"/tmp/a".rename(p"/tmp/b", overwrite: true)""",
    r"""p"/tmp/a".hardlink(p"/tmp/b")""",
    r"""p"/tmp/a".symlink(p"/tmp/b")""",
    r"""let text: Str? = "a-b"
print (text?.replace("-", "+") ?? "")""",
  ]
  for source in positional {
    let rejected = test.expect(ctx, source, status: 2, stderr: ["check.named-arg", "passed by position"])?
    assert rejected.stdout == "", source
  }

  # The names the parameters had while they were passed by position are not
  # labels, like any other name.
  let former = [
    r"""print ("a-b".replace("-", by: "+"))""",
    r"""print ("a-b".replace(from: "-", to: "+"))""",
    r"""print ("a-b".replace(...{from: "-", to: "+"}))""",
    r"""print (rx"-".replace("a-b", replacement: "+"))""",
    r"""p"/tmp/a".copy(dest: p"/tmp/b")""",
    r"""p"/tmp/a".hardlink(path: p"/tmp/b")""",
  ]
  for source in former {
    let rejected = test.expect(ctx, source, status: 2, stderr: ["check.named-arg", "unknown named parameter"])?
    assert rejected.stdout == "", source
  }
}

test test_prefer_argument_label_lint_calls_the_symlink_method { |ctx|
  let root = test.temp_dir(ctx, name: "argument-label-lint")?
  let source = r"""proc stage(root: Path, name: Str) [fs, error] {
  let file = fp"{root}/{name}"
  file.write(name)
  fs.symlink(file, fp"{root}/link")
  fs.symlink("../absent", fp"{root}/dangling")
  print (fp"{root}/link".read_text()?) (fp"{root}/dangling".readlink()?)
}

stage(p"ROOT", "a-b")
"""
  let expected = r"""proc stage(root: Path, name: Str) [fs, error] {
  let file = fp"{root}/{name}"
  file.write(name)
  fp"{root}/link".symlink(to: file)
  fp"{root}/dangling".symlink(to: "../absent")
  print (fp"{root}/link".read_text()?) (fp"{root}/dangling".readlink()?)
}

stage(p"ROOT", "a-b")
"""
  let candidate = test.temp_file(ctx, name: "argument-label.xsh", contents: bytes.from_text(source))?
  let applied = run.capture --text "xsht" lint --only lint.prefer-argument-label --fix $candidate
  assert applied.status.exited_with(0), applied.stderr
  assert candidate.read_text()? == expected

  let formatted = run.capture --text "xsht" fmt --check $candidate
  assert formatted.status.exited_with(0), formatted.stderr
  let repeated = run.capture --text "xsht" lint --only lint.prefer-argument-label $candidate
  assert repeated.status.exited_with(0)
  assert "lint.prefer-argument-label" not in repeated.stderr

  # The fixed program does what the original did, each in its own directory.
  let before = fp"{root}/before"
  let after = fp"{root}/after"
  before.mkdir()
  after.mkdir()
  let output = "a-b ../absent\n"
  test.expect(ctx, source.replace("ROOT", with: before.display()), status: 0, stdout: [output])?
  test.expect(ctx, expected.replace("ROOT", with: after.display()), status: 0, stdout: [output])?
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
  let reported = run.capture --text "xsht" lint --only lint.prefer-argument-label --fix $candidate
  assert ! reported.status.exited_with(0)
  assert "lint.prefer-argument-label" in reported.stderr, reported.stderr
  assert candidate.read_text()? == source
}
