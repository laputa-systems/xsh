test test_path_write_lines_terminates_every_line { |ctx|
  let root = test.temp_dir(ctx, name: "path-write-lines")?
  let report = fp"{root}/report.txt"
  report.write_lines(["first", "second"])?
  assert report.read_text()? == "first\nsecond\n"

  # The file is replaced, never appended to.
  report.write_lines(["only"])?
  assert report.read_text()? == "only\n"

  # No lines is an empty file, where joining and appending a newline writes one.
  let none = []
  report.write_lines(none)?
  assert report.read_bytes()? == b""
  let joined = none.join("\n") + "\n"
  assert joined == "\n"

  # Elements are written as given.
  report.write_lines(["", "a\nb", "tail "])?
  assert report.read_text()? == "\na\nb\ntail \n"
}

test test_path_write_lines_fails_and_creates_like_write { |ctx|
  let root = test.temp_dir(ctx, name: "path-write-lines-failure")?
  let missing_parent = fp"{root}/absent/report.txt"
  let by_lines_failure = missing_parent.write_lines(["x"])
  let by_write_failure = missing_parent.write("x\n")
  assert by_lines_failure is Err(is NotFound)
  assert by_write_failure is Err(is NotFound)
  if let Err(lines_error) = by_lines_failure {
    if let Err(write_error) = by_write_failure {
      assert lines_error.message == write_error.message
    }
  }

  let by_lines = fp"{root}/lines.txt"
  let by_write = fp"{root}/write.txt"
  by_lines.write_lines(["x"])?
  by_write.write("x\n")?
  assert by_lines.metadata()?.mode == by_write.metadata()?.mode

  # An existing file keeps its mode, as it does under write.
  by_lines.chmod(0o600)?
  by_lines.write_lines(["y"])?
  assert by_lines.metadata()?.mode.bit_and(0o777) == 0o600
}

test test_path_write_lines_is_a_filesystem_effect { |ctx|
  let checked = test.run_script(
    ctx,
    r"""
pure save(report: Path) -> Result[Unit] {
  report.write_lines(["x"])
}
""",
  )?
  assert ! checked.success
  assert "pure" in checked.stderr

  let typed = test.run_script(
    ctx,
    r"""
p"report.txt".write_lines([1, 2])?
""",
  )?
  assert ! typed.success
  assert "check.type-mismatch" in typed.stderr
}

test test_write_lines_lint_fixes_only_lists_proven_nonempty { |ctx|
  let root = test.temp_dir(ctx, name: "write-lines-lint")?
  let source = r"""proc save(out: Path, names: List[Str]) [fs, error] {
  fs.write(out, ["header", @names].join("\n") + "\n")?
  print (out.read_text()?)
  out.write(names.join("\n") + "\n")?
  print (out.read_text()?.byte_len())
}

save(p"ROOT/out.txt", ["a", "b"])?
save(p"ROOT/out.txt", [])?
""".replace("ROOT", root.display())
  let candidate = test.temp_file(ctx, name: "write-lines.xsh", contents: bytes.from_text(source))?
  let applied = run.capture --text "xsht" lint --only lint.prefer-write-lines --fix $candidate ?
  let applied_succeeded = applied.status.exited_with(0)
  let applied_details = applied.stderr
  assert applied_succeeded, applied_details
  let fixed = candidate.read_text()?

  # `names` may be empty, where the join writes one newline and write_lines
  # writes nothing, so that call keeps its spelling and is only reported.
  assert fixed == source.replace(
    r"""fs.write(out, ["header", @names].join("\n") + "\n")?""",
    r"""out.write_lines(["header", @names])?""",
  )
  assert fixed != source
  let before = test.run_script(ctx, source)?
  let after = test.run_script(ctx, fixed)?
  let {success: succeeded, stderr: failure_details, ..} = after
  assert succeeded, failure_details
  assert after.stdout == before.stdout
  assert after.stdout == "header\na\nb\n\n4\nheader\n\n1\n"

  let remaining = run.capture --text "xsht" lint --only lint.prefer-write-lines $candidate ?
  assert "lint.prefer-write-lines" in remaining.stderr
  assert "an empty list" in remaining.stderr
  assert candidate.read_text()? == fixed
}

test test_path_read_lines_is_read_text_then_lines { |ctx|
  let root = test.temp_dir(ctx, name: "path-read-lines")?
  let file = fp"{root}/lines.txt"
  for contents in ["", "\n", "one", "one\ntwo\n", "one\r\ntwo\r\n\r\n", "\n\nlast", " padded \n"] {
    file.write(contents)?
    let text = file.read_text()?
    assert file.read_lines()? == text.lines()
    assert file.read_lines()? == contents.lines()
  }

  file.write("one\ntwo\n")?
  assert file.read_lines()? == ["one", "two"]

  # The partner of write_lines.
  let written = ["first", "", "third", ""]
  file.write_lines(written)?
  assert file.read_lines()? == written
  file.write_lines([])?
  assert file.read_lines()? == []
}

test test_path_read_lines_fails_like_read_text { |ctx|
  let root = test.temp_dir(ctx, name: "path-read-lines-failure")?
  let missing = fp"{root}/missing.txt"
  let lines_failure = missing.read_lines()
  let text_failure = missing.read_text()
  assert lines_failure is Err(is NotFound)
  if let Err(lines_error) = lines_failure {
    if let Err(text_error) = text_failure {
      assert lines_error.message == text_error.message
    }
  }

  # Decoding fails at the call, before any line is produced.
  let binary = fp"{root}/binary"
  binary.write(b"ok\n\xff\n")?
  let decoded = binary.read_lines()
  assert decoded is Err(_)
  if let Err(decode_error) = decoded {
    assert "not valid UTF-8 at byte 3" in decode_error.message, decode_error.message
  }

  assert root.read_lines() is Err(_)
}

test test_path_read_lines_is_a_filesystem_effect { |ctx|
  let checked = test.run_script(
    ctx,
    r"""
pure load(source: Path) -> Result[List[Str]] {
  source.read_lines()
}
""",
  )?
  assert ! checked.success
  assert "pure" in checked.stderr
}

test test_read_lines_lint_fix_keeps_values_and_failures { |ctx|
  let root = test.temp_dir(ctx, name: "read-lines-lint")?
  fp"{root}/list.txt".write("one\r\ntwo\n\nfour")?
  fp"{root}/binary".write(b"ok\n\xff\n")?
  let source = r"""proc count(source: Path) [fs, error] -> Result[Int] {
  let direct = source.read_text()?.lines()
  let by_module = fs.read_text(source)?.lines()
  let trimmed = source.read_text()?.trim().lines()
  direct.len() + by_module.len() * 10 + trimmed.len() * 100
}

proc describe(source: Path) [fs, error] -> Str {
  match count(source) {
    Ok(total) => f"{total}",
    Err(error) => error.message.replace("ROOT", ""),
  }
}

print (describe(p"ROOT/list.txt"))
print (describe(p"ROOT/missing.txt"))
print (describe(p"ROOT/binary"))
""".replace("ROOT", root.display())
  let candidate = test.temp_file(ctx, name: "read-lines.xsh", contents: bytes.from_text(source))?
  let applied = run.capture --text "xsht" lint --only lint.prefer-read-lines --fix $candidate ?
  let applied_succeeded = applied.status.exited_with(0)
  let applied_details = applied.stderr
  assert applied_succeeded, applied_details
  let fixed = candidate.read_text()?

  # A read that is trimmed before the split is a different program.
  assert fixed == source.replace("let direct = source.read_text()?.lines()", "let direct = source.read_lines()?")
    .replace("let by_module = fs.read_text(source)?.lines()", "let by_module = source.read_lines()?")
  assert "source.read_text()?.trim().lines()" in fixed
  let before = test.run_script(ctx, source)?
  let after = test.run_script(ctx, fixed)?
  let {success: succeeded, stderr: failure_details, ..} = after
  assert succeeded, failure_details
  assert after.stdout == before.stdout
  let reported = after.stdout.lines()
  assert reported.len() == 3
  assert reported[0] == "444"
  assert "missing.txt" in reported[1]
  assert "not valid UTF-8 at byte 3" in reported[2]

  let repeated = run.capture --text "xsht" lint --only lint.prefer-read-lines $candidate ?
  assert repeated.status.exited_with(0)
  assert "lint.prefer-read-lines" not in repeated.stderr
}
