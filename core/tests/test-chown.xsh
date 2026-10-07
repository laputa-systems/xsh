type PermRan = {status: Int, stdout: Str, stderr: Str}

proc perm_run(ctx: TestContext, args: List[Union[Str, Path]]) [fs, process, error] -> Result[PermRan] {
  let root = test.temp_dir(ctx, name: "capture")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let words: List[Union[Str, Path]] = collect { yield ctx.xsh_bin; yield fp"{ctx.core_dir}/chown.xsh"; yield "--"; for arg in args { yield arg } }
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

test test_chown_verbose_from_mismatch_reports_retained_owner { |ctx|
  let target = test.temp_file(ctx, name: "retained-owner", contents: b"x")?
  let before = fs.stat(target)?
  let result = perm_run(ctx, ["-v", "--from=99999", f"{before.uid}", target.display()])?
  assert result.status == 0
  assert result.stdout.find("retained as") != null
  assert fs.stat(target)?.uid == before.uid
}

test test_chown_verbose_missing_file_reports_stdout_write_failure { |ctx|
  if ! (p"/dev/full".exists() ?? false) { test.skip("/dev/full is unavailable") }
  let root = test.temp_dir(ctx, name: "failed-owner-output")?
  let stderr = fp"{root}/stderr"
  let uid = user.current()?.uid
  let words = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/chown.xsh".display(), "--", "-v", f"{uid}", f"{root}/missing"]
  let status = process.run(process.command_argv(ctx.xsh_bin, words, root, {LC_ALL: "C"}, b"", p"/dev/full", stderr))?
  assert status.exit_code()? == 1
  assert stderr.read_text()?.find("write error: No space left on device") != null, stderr.read_text()?
}

test test_chown_logical_cycle_visits_link_without_repeating_descendants { |ctx|
  let root = test.temp_dir(ctx, name: "ownership-cycle")?
  let child = fp"{root}/child"
  child.mkdir()
  fp"{child}/back".symlink(to: root)
  let uid = user.current()?.uid
  let result = perm_run(ctx, ["-vRL", f"{uid}", root.display()])?
  assert result.status == 0
  assert result.stdout.find(f"ownership of '{child}/back' retained as") != null
  assert result.stdout.find(f"{child}/back/child") == null
}

test test_chown_numeric_id_allows_leading_space_and_forced_numeric_prefix { |ctx|
  let target = test.temp_file(ctx, name: "numeric-owner", contents: b"x")?
  let before = fs.stat(target)?
  assert perm_run(ctx, [f"\t+{before.uid}", target.display()])?.status == 0
  assert fs.stat(target)?.uid == before.uid
  assert fs.stat(target)?.gid == before.gid
}

test test_chown_numeric_owner_with_empty_group_is_invalid_spec { |ctx|
  let target = test.temp_file(ctx, name: "numeric-owner-group", contents: b"x")?
  let spec = f"{user.current()?.uid}:"
  let result = perm_run(ctx, [spec, target.display()])?
  assert result.status == 1
  assert result.stderr.find(f"invalid spec: '{spec}'") != null
}

test test_chown_numeric_owner_dot_is_invalid_user { |ctx|
  let target = test.temp_file(ctx, name: "numeric-owner-dot", contents: b"x")?
  let spec = f"{user.current()?.uid}."
  let result = perm_run(ctx, [spec, target.display()])?
  assert result.status == 1
  assert result.stderr.find(f"invalid user: '{spec}'") != null
  assert result.stderr.find("should be ':'") == null
}

test test_chown_accepts_non_utf8_operand_bytes { |ctx|
  let root = test.temp_dir(ctx, name: "raw-owner-operand")?
  let file = Path.parse_bytes(bytes.concat([root.bytes(), b"/file\xff"]))?
  file.write("x")
  let current = user.current()?
  let result = perm_run(ctx, [current.name, file])?
  assert result.status == 0, result.stderr
  assert fs.stat(file)?.uid == current.uid
}

test test_chown_reference_accepts_non_utf8_path_bytes { |ctx|
  let root = test.temp_dir(ctx, name: "raw-reference")?
  let reference = Path.parse_bytes(bytes.concat([root.bytes(), b"/reference\xff"]))?
  let target = fp"{root}/target"
  reference.write("reference")
  target.write("target")
  let result = perm_run(ctx, ["--reference", reference, target])?
  assert result.status == 0, result.stderr
  assert fs.stat(target)?.uid == user.current()?.uid
}

test test_chown_recursive_preserves_non_utf8_child_names { |ctx|
  if user.current()?.uid != 0 { test.skip("changing numeric owners requires root") }
  let root = test.temp_dir(ctx, name: "raw-owner-name")?
  let child = Path.parse_bytes(bytes.concat([root.bytes(), b"/child\xff"]))?
  child.write("x")
  let result = perm_run(ctx, ["-R", "12345", root.display()])?
  assert result.status == 0, result.stderr
  assert fs.stat(child)?.uid == 12345
}
