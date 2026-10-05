type PermRan = {status: Int, stdout: Str, stderr: Str}

proc perm_run(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[PermRan] {
  let root = test.temp_dir(ctx, name: "capture")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let words = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/chroot.xsh".display(), "--"].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, words, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", out, err))?
  {status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?}
}

test test_chroot_missing_operand_and_skip_chdir_validation { |ctx|
  assert perm_run(ctx, [])?.status == 125
  let root = test.temp_dir(ctx, name: "chroot")?
  let result = perm_run(ctx, ["--skip-chdir", root.display()])?
  assert result.status == 125
  assert result.stderr.find("only permitted") != null
}

test test_chroot_exec_status_and_root_working_directory { |ctx|
  if user.current()?.uid != 0 { test.skip("chroot requires root") }
  let script = test.temp_file(ctx, name: "cwd.xsh", contents: b"print fs.cwd()?\n")?
  let result = perm_run(ctx, ["/", ctx.xsh_bin.display(), script.display()])?
  assert result.status == 0, result.stderr
  assert result.stdout == "/\n"
  assert perm_run(ctx, ["/", "/xsh-missing-command"])?.status == 127
  assert perm_run(ctx, ["/", script.display()])?.status == 126
}

test test_chroot_relative_command_resolves_inside_new_root { |ctx|
  if user.current()?.uid != 0 { test.skip("chroot requires root") }
  let root = test.temp_dir(ctx, name: "isolated-root")?
  let binary = fp"{root}/xsh"
  ctx.xsh_bin.copy(to: binary)
  binary.chmod(0o755)
  fp"{root}/report.xsh".write("print fs.cwd()?\n")
  let result = perm_run(ctx, [root.display(), "./xsh", "/report.xsh"])?
  assert result.status == 0, result.stderr
  assert result.stdout == "/\n"
}

test test_chroot_explicit_and_empty_supplementary_groups { |ctx|
  if user.current()?.uid != 0 { test.skip("setting supplementary groups requires root") }
  let script = test.temp_file(ctx, name: "groups.xsh", contents: b"for entry in unix.id()?.groups { print $entry.gid }\n")?
  let explicit = perm_run(ctx, ["--groups=123,456,123", "/", ctx.xsh_bin.display(), script.display()])?
  assert explicit.status == 0, explicit.stderr
  assert explicit.stdout == "0\n123\n456\n"
  let empty = perm_run(ctx, ["--groups=invalid", "--groups=", "/", ctx.xsh_bin.display(), script.display()])?
  assert empty.status == 0, empty.stderr
  assert empty.stdout == "0\n"
}

test test_chroot_invalid_supplementary_group_fails { |ctx|
  let invalid = perm_run(ctx, ["--groups=xsh-missing-group", "/", "/missing-command"])?
  assert invalid.status == 125
  if user.current()?.uid == 0 {
    assert invalid.stderr.find("invalid group") != null
  } else {
    assert invalid.stderr.find("cannot change root directory") != null
  }
}

test test_chroot_skip_chdir_rejects_missing_directory { |ctx|
  let result = perm_run(ctx, ["--skip-chdir", "/xsh-missing-root-directory"])?
  assert result.status == 125
  assert result.stderr.find("only permitted") != null
}

test test_chroot_sets_explicit_user_group_and_supplementary_groups { |ctx|
  if user.current()?.uid != 0 { test.skip("changing credentials requires root") }
  ctx.temp_root.chmod(0o755)
  let script = test.temp_file(ctx, name: "identity.xsh", contents: b"let identity = unix.id()?\nprint f\"{identity.uid}:{identity.gid}\"\nfor entry in identity.groups { print $entry.gid }\n")?
  script.chmod(0o644)
  let result = perm_run(ctx, ["--userspec=12345:12346", "--groups=12347", "/", ctx.xsh_bin.display(), script.display()])?
  assert result.status == 0, result.stderr
  assert result.stdout == "12345:12346\n12346\n12347\n"
  let group_only = perm_run(ctx, ["--userspec=:12346", "/", ctx.xsh_bin.display(), script.display()])?
  assert group_only.status == 0, group_only.stderr
  assert group_only.stdout.starts_with("0:12346\n")
}

test test_chroot_unknown_uid_without_primary_group_fails { |ctx|
  if user.current()?.uid != 0 { test.skip("chroot requires root") }
  let result = perm_run(ctx, ["--userspec=99999", "/", "/missing-command"])?
  assert result.status == 125
  assert result.stderr.find("no group specified for unknown uid: 99999") != null
}

test test_chroot_uses_accounts_inside_the_new_root { |ctx|
  if user.current()?.uid != 0 { test.skip("chroot requires root") }
  let root = test.temp_dir(ctx, name: "account-root")?
  root.chmod(0o755)
  fp"{root}/etc".mkdir()
  fp"{root}/etc/passwd".write("xsh-chroot-account:x:12345:12346::/:/xsh\n")
  fp"{root}/etc/group".write("primary:x:12346:\nsecondary:x:23456:xsh-chroot-account\n")
  let binary = fp"{root}/xsh"
  ctx.xsh_bin.copy(to: binary)
  binary.chmod(0o755)
  fp"{root}/report.xsh".write("let identity = unix.id()?\nprint f\"{identity.uid}:{identity.gid}\"\nfor entry in identity.groups { print $entry.gid }\n")
  let result = perm_run(ctx, ["--userspec=xsh-chroot-account", root.display(), "/xsh", "/report.xsh"])?
  assert result.status == 0, result.stderr
  assert result.stdout == "12345:12346\n12346\n23456\n"
  let alternate = perm_run(ctx, ["--userspec=xsh-chroot-account:34567", root.display(), "/xsh", "/report.xsh"])?
  assert alternate.status == 0, alternate.stderr
  assert alternate.stdout == "12345:34567\n23456\n34567\n"
}

test test_chroot_inaccessible_command_reports_permission_denied { |ctx|
  if user.current()?.uid != 0 { test.skip("changing credentials requires root") }
  let root = test.temp_dir(ctx, name: "private-root")?
  let binary = fp"{root}/xsh"
  ctx.xsh_bin.copy(to: binary)
  binary.chmod(0o755)
  root.chmod(0o000)
  let result = perm_run(ctx, ["--userspec=12345:12346", "--groups=", root.display(), "/xsh"])?
  root.chmod(0o755)
  assert result.status == 126, result.stderr
  assert result.stderr.find("Permission denied") != null
}

test test_chroot_numeric_identity_can_use_outer_group_database { |ctx|
  if user.current()?.uid != 0 { test.skip("chroot requires root") }
  let root = test.temp_dir(ctx, name: "minimal-identity-root")?
  root.chmod(0o755)
  let binary = fp"{root}/xsh"
  ctx.xsh_bin.copy(to: binary)
  binary.chmod(0o755)
  fp"{root}/report.xsh".write("p\"/created\".write(\"x\")\nlet identity = fs.stat(p\"/created\")?\nprint f\"{identity.uid}:{identity.gid}\"\n")
  let result = perm_run(ctx, ["--userspec=0", root.display(), "/xsh", "/report.xsh"])?
  assert result.status == 0, result.stderr
  assert result.stdout == "0:0\n"
}
