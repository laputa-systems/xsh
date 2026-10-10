test test_rmdir_parents { |ctx|
  let root = test.temp_dir(ctx, name: "rmdir")?
  let nested = fp"{root}/a/b/c"
  nested.mkdir()
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/rmdir.xsh" -- --ignore-fail-on-non-empty -p $nested
  assert ! fp"{root}/a".exists()?
}

test test_rmdir_parent_failure_and_continuation { |ctx|
  let root = test.temp_dir(ctx, name: "rmdir-failure")?
  let parent = fp"{root}/parent"
  let nested = fp"{parent}/nested"
  nested.mkdir()
  fp"{parent}/retained".write("retained")
  let other = fp"{root}/other"
  other.mkdir()
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/rmdir.xsh"
  let status = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "-p", nested.display(), other.display()],
    root, {LC_ALL: "C"}, b"", out, err))?
  assert status.exited_with(1)
  assert ! nested.exists()?
  assert ! other.exists()?
  assert parent.exists()?
  assert "Directory not empty" in err.read_text()?
}

test test_rmdir_ignore_nonempty_does_not_ignore_missing { |ctx|
  let root = test.temp_dir(ctx, name: "rmdir-ignore")?
  let nonempty = fp"{root}/nonempty"
  nonempty.mkdir()
  fp"{nonempty}/file".write("retained")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/rmdir.xsh" -- --ignore-fail-on-non-empty $nonempty
  let missing = fp"{root}/missing"
  let failed = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/rmdir.xsh" -- --ignore-fail-on-non-empty $missing
  assert failed.status.exited_with(1)
  assert nonempty.exists()?
}

test test_rmdir_trailing_slash_symlink_diagnostic { |ctx|
  let root = test.temp_dir(ctx, name: "rmdir-link")?
  let directory = fp"{root}/directory"
  let link = fp"{root}/link"
  directory.mkdir()
  link.symlink(to: p"directory")
  let operand = f"{link}/"
  let failed = run.capture --text LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/rmdir.xsh" -- $operand
  assert failed.status.exited_with(1)
  assert failed.stderr == f"rmdir: failed to remove '{operand}': Symbolic link not followed\n"
  assert directory.exists()?
  assert fs.stat(link)?.kind == "symlink"
}

# A retained sibling keeps the walk from climbing past the test root.
test test_rmdir_parents_walks_the_operand_text_with_trailing_slashes { |ctx|
  let root = test.temp_dir(ctx, name: "rmdir-slash-walk")?
  let dir = fp"{root}/a/b"
  dir.mkdir()
  fp"{root}/keep".write("keep")
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let status = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), fp"{ctx.core_dir}/rmdir.xsh".display(), "-p", "--ignore-fail-on-non-empty", "a/b/"],
    root, {LC_ALL: "C"}, b"", out, err))?
  assert status.exited_with(0), err.read_text()?
  assert err.read_text()? == ""
  assert ! fp"{root}/a".exists()?
  assert fp"{root}/keep".exists()?
}

test test_rmdir_parent_failure_uses_gnu_directory_wording { |ctx|
  let root = test.temp_dir(ctx, name: "rmdir-parent-wording")?
  let parent = fp"{root}/parent"
  let nested = fp"{parent}/nested"
  nested.mkdir()
  fp"{parent}/retained".write("retained")
  let failed = run.capture --text LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/rmdir.xsh" -- -p $nested
  assert failed.status.exited_with(1)
  assert failed.stderr == f"rmdir: failed to remove directory '{parent}': Directory not empty\n", failed.stderr
  assert ! nested.exists()?
}
