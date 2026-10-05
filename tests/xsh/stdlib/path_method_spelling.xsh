test test_prefer_path_method_lint_moves_the_path_in_front_of_the_call { |ctx|
  let root = test.temp_dir(ctx, name: "path-method-lint")?
  let source = r"""proc next(log: Path, label: Str) [fs, error] -> Result[Path] {
  # Each operand reports when it is evaluated.
  fs.write(log, (fs.read_text(log) ?? "") + label + "\n")
  Ok(log)
}

proc publish(root: Path) [fs, error] {
  let log = fp"{root}/order.log"
  fs.mkdir(fp"{root}/out/deep", parents: true)
  fs.write(fp"{next(log, "path")?}.copy", f"{next(log, "data")?.name()}\n")
  fs.copy(fp"{log}.copy", fp"{root}/out/deep/copy", overwrite: true)
  fs.chmod(log, 0o600)
  fs.rename(fp"{root}/out/deep/copy", fp"{root}/moved")
  fs.remove(fp"{root}/out", missing_ok: true)
  let kind = fs.metadata(log)?.kind
  let mode = fs.metadata(log)?.mode % 4096
  let order = fs.read_text(log)?
  let moved = fs.read_text(fp"{root}/moved")?
  let kept = fs.exists(fp"{root}/out")?
  let runnable = fs.executable(log)?
  print $order $moved $kept $kind $mode $runnable
  fs.remove(fp"{root}/moved")
  fs.remove(fp"{log}.copy")
  fs.remove(log)
}

publish(p"ROOT")
""".replace("ROOT", root.display())
  let candidate = test.temp_file(ctx, name: "path-method.xsh", contents: bytes.from_text(source))?
  let applied = run.capture --text "xsht" lint --only lint.prefer-path-method --fix $candidate ?
  assert applied.status.exited_with(0), applied.stderr
  let fixed = candidate.read_text()?
  assert fixed != source
  assert "fs." not in fixed.replace("[fs, error]", "")
  for call in [
    r"""log.write((log.read_text() ?? "") + label + "\n")""",
    r"""fp"{root}/out/deep".mkdir(parents: true)""",
    r"""fp"{next(log, "path")?}.copy".write(f"{next(log, "data")?.name()}\n")""",
    r"""fp"{log}.copy".copy(fp"{root}/out/deep/copy", overwrite: true)""",
    "log.chmod(0o600)",
    r"""fp"{root}/out/deep/copy".rename(fp"{root}/moved")""",
    r"""fp"{root}/out".remove(missing_ok: true)""",
    "log.metadata()?.kind",
    r"""fp"{root}/out".exists()?""",
    "log.executable()?",
  ] {
    assert call in fixed, call
  }

  # The same output, with the path operand still evaluated before the data.
  let before = test.run_script(ctx, source)?
  let after = test.run_script(ctx, fixed)?
  assert before.success, before.stderr
  assert after.success, after.stderr
  assert before.stdout == "path\ndata\n order.log\n false file 384 false\n"
  assert after.stdout == before.stdout

  let repeated = run.capture --text "xsht" lint --only lint.prefer-path-method $candidate ?
  assert repeated.status.exited_with(0)
  assert "lint.prefer-path-method" not in repeated.stderr
}
