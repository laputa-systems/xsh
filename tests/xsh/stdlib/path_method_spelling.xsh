test test_removed_fs_function_names_the_path_method { |ctx|
  test.expect(
    ctx,
    "fs.write(p\"/tmp/xsh-removed-fs-never/file\", \"text\")\n",
    status: 2,
    stderr: [
      "err[check.removed-fs-function]: `fs.write` was removed; call the `Path` method `write`",
      "`fs.write` is no longer a function",
      "-> p\"/tmp/xsh-removed-fs-never/file\".write(",
    ],
  )?
}

test test_removed_fs_functions_are_rewritten_by_lint_fix { |ctx|
  let root = test.temp_dir(ctx, name: "removed-fs-fix")?
  let shown = root.display()
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
  fs.rename(
    fp"{root}/out/deep/copy",
    fp"{root}/moved",
  )
  fs.remove(fp"{root}/out", missing_ok: true)
  let kind = fs.metadata(if never(root) { root } else { log })?.kind
  let mode = fs.metadata(log)?.mode % 4096
  let order = fs.read_text(log)?
  let moved = fs.read_text(fp"{root}/moved")?
  let kept = fs.exists(fp"{root}/out")?
  let runnable = fs.executable(log)?
  print $order $moved $kept $kind $mode $runnable
  fs.write_atomic("ROOT/literal", "text")
  fs.remove(
    /ROOT_BARE/literal,
  )
  fs.remove(fp"{root}/moved")
  fs.remove(fp"{log}.copy")
  fs.remove(log)
}

proc never(root: Path) -> Bool {
  root.name() == "never"
}

publish(p"ROOT")
""".replace("/ROOT_BARE", with: shown).replace("ROOT", with: shown)
  let rejected = test.expect(ctx, source, status: 2, stderr: ["err[check.removed-fs-function]"])?
  assert rejected.stdout == ""
  assert rejected.stderr.split("err[check.removed-fs-function]").len() == 20, rejected.stderr
  assert "err[check." not in rejected.stderr.replace("err[check.removed-fs-function]", with: "")

  let candidate = test.temp_file(ctx, name: "removed-fs.xsh", contents: bytes.from_text(source))?
  let applied = run.capture --text "xsht" lint --only check.removed-fs-function --fix $candidate
  assert applied.status.exited_with(0), applied.stderr
  let fixed = candidate.read_text()?
  assert "fs." not in fixed.replace("[fs, error]", with: "")
  for call in [
    r"""log.write((log.read_text() ?? "") + label + "\n")""",
    r"""fp"{root}/out/deep".mkdir(parents: true)""",
    r"""fp"{next(log, "path")?}.copy".write(f"{next(log, "data")?.name()}\n")""",
    r"""fp"{log}.copy".copy(to: fp"{root}/out/deep/copy", overwrite: true)""",
    "log.chmod(0o600)",
    r"""fp"{root}/out/deep/copy".rename(
    to: fp"{root}/moved",
  )""",
    r"""fp"{root}/out".remove(missing_ok: true)""",
    "(if never(root) { root } else { log }).metadata()?.kind",
    r"""fp"{root}/out".exists()?""",
    "log.executable()?",
    f"""p"{shown}/literal".write_atomic("text")""",
    f"""p"{shown}/literal".remove()""",
  ] {
    assert call in fixed, call
  }

  # The rewritten program checks, and the path operand is still evaluated
  # before the data.
  let after = test.expect(ctx, fixed, status: 0)?
  assert after.stdout == "path\ndata\n order.log\n false file 384 false\n"
  assert ! fp"{root}/literal".exists()?
}

# A string read as a path and the path literal with the same text between the
# quotes decode the same escapes, so the rewrite names the same file.
test test_removed_fs_function_fix_keeps_the_escapes_of_a_string_operand { |ctx|
  let root = test.temp_dir(ctx, name: "removed-fs-escape")?
  let source = r"""proc publish() [fs, error] {
  fs.write("ROOT/a\tb \"c\" \\ \u{e9}.txt", "kept")
  for entry in fs.children(p"ROOT")? {
    print $entry.name
  }
}

publish()
""".replace("ROOT", with: root.display())
  let candidate = test.temp_file(ctx, name: "removed-fs-escape.xsh", contents: bytes.from_text(source))?
  let applied = run.capture --text "xsht" lint --only check.removed-fs-function --fix $candidate
  assert applied.status.exited_with(0), applied.stderr
  let fixed = candidate.read_text()?
  assert r"""/a\tb \"c\" \\ \u{e9}.txt".write("kept")""" in fixed, fixed
  assert "fs.write" not in fixed

  let after = test.expect(ctx, fixed, status: 0)?
  assert after.stdout == "a\tb \"c\" \\ é.txt\n"
}

# A comment beside the path would be lost, a string with `$` or a raw string
# has no path literal that is certainly the same path, text that is not a
# literal has no `Path` method, and a command word has no receiver spelling.
test test_removed_fs_function_without_a_rewrite_is_still_reported { |ctx|
  let source = r"""proc publish(out: Path, text: Str, name: Str) [fs, error] {
  fs.write(
    out, # the target
    text,
  )
  fs.write("cost-$5.txt", text)
  fs.write(r"raw\name.txt", text)
  fs.write(name, text)
  fs.write(f"{name}.txt", text)
  fs.mkdir build
  fs.remove $out --missing-ok
}
"""
  let checked = test.expect(ctx, source, status: 2)?
  assert checked.stderr.split("err[check.removed-fs-function]").len() == 8, checked.stderr
  assert "err[check." not in checked.stderr.replace("err[check.removed-fs-function]", with: "")
  assert "help:" not in checked.stderr, checked.stderr
  assert checked.stderr.split("convert it first with `Path(text)`").len() == 3, checked.stderr
  assert "a method has no command form; write the call `PATH.mkdir(...)`" in checked.stderr
  assert "a method has no command form; write the call `PATH.remove(...)`" in checked.stderr

  let candidate = test.temp_file(ctx, name: "removed-fs-manual.xsh", contents: bytes.from_text(source))?
  let _ = run.capture --text "xsht" lint --only check.removed-fs-function --fix $candidate
  assert candidate.read_text()? == source
}

# `fs.executable(mode)` tests a mode and has no path to be called on, so it
# is still a function; only its path overload became the method.
test test_fs_executable_still_tests_a_mode { |ctx|
  let root = test.temp_dir(ctx, name: "removed-fs-executable")?
  let source = r"""proc show(tool: Path) [fs, error] {
  tool.write("#!/bin/sh\n", mode: 0o755)
  print (fs.executable(tool.metadata()?.mode)) (fs.executable(0o644))
  print (fs.executable(tool)?)
}

show(p"ROOT/tool")
""".replace("ROOT", with: root.display())
  let rejected = test.expect(
    ctx,
    source,
    status: 2,
    stderr: [
      "err[check.removed-fs-function]: `fs.executable` no longer takes a path; call the `Path` method `executable`",
      "-> tool.executable(",
    ],
  )?
  assert rejected.stderr.split("err[check.").len() == 2, rejected.stderr

  let candidate = test.temp_file(ctx, name: "executable.xsh", contents: bytes.from_text(source))?
  let applied = run.capture --text "xsht" lint --only check.removed-fs-function --fix $candidate
  assert applied.status.exited_with(0), applied.stderr
  assert candidate.read_text()? == source.replace("fs.executable(tool)?", with: "tool.executable()?")
  let after = test.expect(ctx, candidate.read_text()?, status: 0)?
  assert after.stdout == "true false\ntrue\n"
}
