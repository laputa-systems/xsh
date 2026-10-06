type FakeOutput = {success: Bool, status: Int, stdout: Str, stderr: Str, stdout_bytes: Bytes, stderr_bytes: Bytes}

proc chattr_fake(ctx: TestContext, args: List[Str], flags: Int, log: Path) [fs, process, error] -> FakeOutput {
  test.linux_fake(ctx, {file_attrs_flags: flags, log: log})
  let source = fp"{ctx.core_dir}/chattr.xsh".read_text()?
  test.run_script(ctx, source, args, {XSH_MODULE_PATH: ctx.core_dir.display()}, b"", "chattr")?
}

test test_chattr_add_remove_assignment_and_generation { |ctx|
  let root = test.temp_dir(ctx, name: "chattr")?
  let file = fp"{root}/file"
  let log = fp"{root}/operations"
  file.write("content")
  let added = chattr_fake(ctx, ["+d", file.display()], 128, log)
  assert added.status == 0, added.stderr
  assert "\"flags\":\"192\"" in log.read_text()?
  log.write("")
  let removed = chattr_fake(ctx, ["-A", file.display()], 192, log)
  assert removed.status == 0, removed.stderr
  assert "\"flags\":\"64\"" in log.read_text()?
  log.write("")
  let assigned = chattr_fake(ctx, ["=d", "-v", "7", file.display()], 128, log)
  assert assigned.status == 0, assigned.stderr
  assert "set_file_version" in log.read_text()?
  assert "\"flags\":\"64\"" in log.read_text()?
  assert file.read_text()? == "content"
}

test test_chattr_rejects_conflicts_and_recursion_skips_symlinks { |ctx|
  let root = test.temp_dir(ctx, name: "chattr-recursive")?
  let log = fp"{root}/operations"
  fp"{root}/dir".mkdir()
  fp"{root}/dir/file".write("content")
  fp"{root}/dir/link".symlink(to: p"file")
  assert chattr_fake(ctx, ["+d", "-d", root.display()], 0, log).status == 1
  assert chattr_fake(ctx, ["=d", "+A", root.display()], 0, log).status == 1
  let recursive = chattr_fake(ctx, ["-R", "+d", fp"{root}/dir".display()], 0, log)
  assert recursive.status == 0, recursive.stderr
  assert "dir/file" in log.read_text()?
  assert "dir/link" not in log.read_text()?
}

test test_chattr_real_nodump_is_limited_to_temporary_fixtures { |ctx|
  let root = test.temp_dir(ctx, name: "chattr-real")?
  let file = fp"{root}/file"
  file.write("content")
  let metadata = linux.file_attrs(file)
  if let Err(failure) = metadata {
    if failure.errno == 95 or failure.errno == 25 { test.skip("inode flags unavailable on fixture filesystem"); return }
    test.fail(failure.message)
  }
  let before = metadata?.flags
  let probe = linux.set_file_attrs(file, before.bit_or(64))
  if let Err(failure) = probe {
    if failure.errno == 95 or failure.errno == 1 { test.skip("fixture filesystem forbids changing inode flags"); return }
    test.fail(failure.message)
  }
  linux.set_file_attrs(file, before)
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/chattr.xsh"
  let args = [ctx.xsh_bin.display(), script.display(), "--", "+d", file.display()]
  let plan = process.command_argv(ctx.xsh_bin, args, root, {}, b"", out, err)
  assert process.run(plan)?.exit_code()? == 0, err.read_text()?
  assert linux.file_attrs(file)?.flags == before.bit_or(64)
  linux.set_file_attrs(file, before)
  assert file.read_text()? == "content"
}

test test_chattr_quiet_errors_still_fail_and_dirsync_is_directory_only { |ctx|
  let root = test.temp_dir(ctx, name: "chattr-errors")?
  let log = fp"{root}/operations"
  let absent = chattr_fake(ctx, ["-f", "+d", fp"{root}/absent".display()], 0, log)
  assert absent.status == 1
  assert absent.stderr == ""
  fp"{root}/file".write("content")
  let regular = chattr_fake(ctx, ["+D", fp"{root}/file".display()], 64, log)
  assert regular.status == 0, regular.stderr
  assert "\"flags\":\"64\"" in log.read_text()?
  log.write("")
  fp"{root}/dir".mkdir()
  let directory = chattr_fake(ctx, ["+D", fp"{root}/dir".display()], 64, log)
  assert directory.status == 0, directory.stderr
  assert "\"flags\":\"65600\"" in log.read_text()?
}


test test_chattr_project_id_is_unsigned_and_preserves_other_flags { |ctx|
  let root = test.temp_dir(ctx, name: "chattr-project")?
  let file = fp"{root}/file"
  let log = fp"{root}/operations"
  file.write("content")
  let output = chattr_fake(ctx, ["-p", "4294967295", file.display()], 128, log)
  assert output.status == 0, output.stderr
  assert "\"project\":\"4294967295\"" in log.read_text()?
  assert "\"flags\":\"128\"" in log.read_text()?
  assert chattr_fake(ctx, ["-p", "4294967296", file.display()], 128, log).status == 1
  assert chattr_fake(ctx, ["-p", "-1", file.display()], 128, log).status == 1
}
