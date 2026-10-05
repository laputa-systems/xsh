type PermRan = {status: Int, stdout: Str, stderr: Str}

proc perm_run(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[PermRan] {
  let root = test.temp_dir(ctx, name: "capture")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let words = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/chown.xsh".display(), "--"].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, words, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", out, err))?
  {status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?}
}

test test_chown_current_user { |ctx|
  let target = test.temp_file(ctx, name: "owned.txt", contents: b"payload")?
  let current = user.current()?
  let name = current.name
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/chown.xsh" -- $name $target
  assert output == ""
  assert target.metadata()?.uid == current.uid
  let root = test.temp_dir(ctx, name: "owned-tree")?
  let child = fp"{root}/child.txt"
  child.write("payload")
  let recursive = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/chown.xsh" -- -R $name $root
  assert recursive == ""
  assert child.metadata()?.uid == current.uid
}

test test_chown_numeric_owner_preserves_group_and_from_filters { |ctx|
  let file = test.temp_file(ctx, name: "owned", contents: b"x")?
  let before = fs.stat(file)?
  let current = user.current()?
  let retained = perm_run(ctx, ["-v", f"{current.uid}", file.display()])?
  assert retained.status == 0
  assert retained.stdout.find("retained as") != null
  assert fs.stat(file)?.gid == before.gid
  if current.uid == 0 {
    assert perm_run(ctx, ["--from=99999", "12345:54321", file.display()])?.status == 0
    assert fs.stat(file)?.uid == before.uid
    assert perm_run(ctx, [f"--from={before.uid}", "12345:54321", file.display()])?.status == 0
    assert fs.stat(file)?.uid == 12345
    assert fs.stat(file)?.gid == 54321
  }
}

test test_chown_reference_and_quiet_failure_continue { |ctx|
  let file = test.temp_file(ctx, name: "owned", contents: b"x")?
  let before = fs.stat(file)?
  let result = perm_run(ctx, ["-f", f"{before.uid}", f"{file}/absent", file.display()])?
  assert result.status == 1
  assert result.stderr == ""
  assert perm_run(ctx, [f"--reference={file}", file.display()])?.status == 0
}

test test_chown_recursive_physical_changes_link_while_logical_changes_referent { |ctx|
  if user.current()?.uid != 0 { test.skip("changing numeric owners requires root") }
  let root = test.temp_dir(ctx, name: "owners")?
  let outside = test.temp_file(ctx, name: "outside", contents: b"x")?
  let link = fp"{root}/link"
  link.symlink(to: outside)
  assert perm_run(ctx, ["-R", "12345", root.display()])?.status == 0
  assert fs.stat(link)?.uid == 12345
  assert fs.stat(outside)?.uid == 0
  assert perm_run(ctx, ["-RL", "23456", root.display()])?.status == 0
  assert fs.stat(outside)?.uid == 23456
  assert fs.stat(link)?.uid == 12345
}

test test_chown_root_guard_and_link_policy_validation { |ctx|
  let current = user.current()?
  let guarded = perm_run(ctx, ["--preserve-root", "-R", current.name, "/"])?
  assert guarded.status == 1
  assert guarded.stderr.find("dangerous") != null
  let target = test.temp_file(ctx, name: "policy", contents: b"x")?
  let result = perm_run(ctx, ["-R", "--dereference", current.name, target.display()])?
  assert result.status == 1
  assert result.stderr.find("requires -H or -L") != null
}

test test_chown_dot_separator_uses_login_group_and_warns { |ctx|
  let current = user.current()?
  let target = test.temp_file(ctx, name: "legacy-owner", contents: b"x")?
  let result = perm_run(ctx, [f"{current.name}.", target.display()])?
  assert result.status == 0
  assert result.stderr.find("warning: '.' should be ':'") != null
  assert fs.stat(target)?.gid == current.gid
}
