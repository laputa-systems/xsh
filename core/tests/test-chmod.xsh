type PermRan = {status: Int, stdout: Str, stderr: Str}

proc perm_run(ctx: TestContext, args: List[Union[Str, Path]]) [fs, process, error] -> Result[PermRan] {
  let root = test.temp_dir(ctx, name: "capture")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let words: List[Union[Str, Path]] = collect { yield ctx.xsh_bin; yield fp"{ctx.core_dir}/chmod.xsh"; yield "--"; for arg in args { yield arg } }
  let status = process.run(process.command_argv(ctx.xsh_bin, words, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", out, err))?
  {status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?}
}

test test_chmod_recursive { |ctx|
  let root = test.temp_dir(ctx, name: "chmod")?
  let dir = fp"{root}/dir"
  dir.mkdir()
  let child = fp"{dir}/child.txt"
  child.write("payload")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/chmod.xsh" -- -R 700 $dir
  assert child.metadata()?.mode % 512 == 448
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/chmod.xsh" -- 600 $child
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/chmod.xsh" -- u+x,g+r $child
  assert child.metadata()?.mode % 512 == 480
}

test test_chmod_symbolic_copy_chained_and_conditional_execute { |ctx|
  let file = test.temp_file(ctx, name: "mode", contents: b"x")?
  file.chmod(0o640)
  assert perm_run(ctx, ["g=u,o=g", file.display()])?.status == 0
  assert fs.stat(file)?.mode.bit_and(0o7777) == 0o666
  assert perm_run(ctx, ["u=rw+x-w,g=X,o=", file.display()])?.status == 0
  assert fs.stat(file)?.mode.bit_and(0o7777) == 0o510
}

test test_chmod_reference_changes_and_failure_continuation { |ctx|
  let file = test.temp_file(ctx, name: "mode", contents: b"x")?
  let reference = test.temp_file(ctx, name: "reference", contents: b"x")?
  reference.chmod(0o651)
  let changed = perm_run(ctx, ["-c", f"--reference={reference}", file.display()])?
  assert changed.status == 0
  assert changed.stdout.find("changed from") != null
  assert fs.stat(file)?.mode.bit_and(0o7777) == 0o651
  assert perm_run(ctx, ["-c", f"--reference={reference}", file.display()])?.stdout == ""
  let failure = perm_run(ctx, ["-f", "600", f"{file}/absent", file.display()])?
  assert failure.status == 1
  assert failure.stderr == ""
  assert fs.stat(file)?.mode.bit_and(0o7777) == 0o600
}

test test_chmod_invalid_modes_fail_before_mutation { |ctx|
  let file = test.temp_file(ctx, name: "mode", contents: b"x")?
  file.chmod(0o640)
  for invalid in ["u+z", "u+r,", "888", "10000", "u+ug"] {
    assert perm_run(ctx, [invalid, file.display()])?.status == 1
    assert fs.stat(file)?.mode.bit_and(0o7777) == 0o640
  }
}

test test_chmod_malformed_option_like_mode_is_invalid_mode { |ctx|
  let file = test.temp_file(ctx, name: "negative-mode", contents: b"x")?
  file.chmod(0o640)
  let result = perm_run(ctx, ["-rw%x", file.display()])?
  assert result.status == 1
  assert result.stderr.find("invalid mode: '-rw%x'") != null
  assert result.stderr.find("invalid option") == null
  assert fs.stat(file)?.mode.bit_and(0o7777) == 0o640
}

test test_chmod_accepts_non_utf8_operand_bytes { |ctx|
  let root = test.temp_dir(ctx, name: "raw-operand")?
  let file = Path.parse_bytes(bytes.concat([root.bytes(), b"/file\xff"]))?
  file.write("x")
  file.chmod(0o644)
  let result = perm_run(ctx, ["755", file])?
  assert result.status == 0, result.stderr
  assert fs.stat(file)?.mode.bit_and(0o7777) == 0o755
}

test test_chmod_recursive_skips_inner_symlink_and_keeps_directory_setgid { |ctx|
  let root = test.temp_dir(ctx, name: "mode-tree")?
  let outside = test.temp_file(ctx, name: "outside", contents: b"x")?
  outside.chmod(0o600)
  fp"{root}/link".symlink(to: outside)
  root.chmod(0o2770)
  assert perm_run(ctx, ["-R", "755", root.display()])?.status == 0
  assert fs.stat(root)?.mode.bit_and(0o7777) == 0o2755
  assert fs.stat(outside)?.mode.bit_and(0o7777) == 0o600
}

test test_chmod_umask_assignment_and_numeric_operators { |ctx|
  let file = test.temp_file(ctx, name: "numeric-mode", contents: b"x")?
  file.chmod(0o777)
  assert perm_run(ctx, ["=rw", file.display()])?.status == 0
  assert fs.stat(file)?.mode.bit_and(0o7777) == 0o666.clear_bits(fs.umask()?)
  assert perm_run(ctx, ["+100", file.display()])?.status == 0
  assert fs.stat(file)?.mode.bit_and(0o100) == 0o100
  assert perm_run(ctx, ["-100", file.display()])?.status == 0
  assert fs.stat(file)?.mode.bit_and(0o100) == 0
  assert perm_run(ctx, ["=600", file.display()])?.status == 0
  assert fs.stat(file)?.mode.bit_and(0o7777) == 0o600
}

test test_chmod_option_like_mode_permutation_and_umask_diagnostic { |ctx|
  let file = test.temp_file(ctx, name: "option-mode", contents: b"x")?
  file.chmod(0o666)
  let result = perm_run(ctx, [file.display(), "-w", "-w"]) ?
  let expected = 0o666.clear_bits(0o222.clear_bits(fs.umask()?))
  assert fs.stat(file)?.mode.bit_and(0o7777) == expected
  assert result.status == (if expected != 0o444 { 1 } else { 0 })
  if expected != 0o444 { assert result.stderr.find("new permissions are") != null }
  file.chmod(0o666)
  let explicit = perm_run(ctx, ["--", "-w", file.display()])?
  assert explicit.status == 0
  assert explicit.stderr == ""
  assert fs.stat(file)?.mode.bit_and(0o7777) == expected
}

test test_chmod_root_guard_and_long_abbreviation { |ctx|
  let guarded = perm_run(ctx, ["--preserve-root", "-R", "000", "/"])?
  assert guarded.status == 1
  assert guarded.stderr.find("dangerous") != null
  let file = test.temp_file(ctx, name: "abbreviated", contents: b"x")?
  assert perm_run(ctx, ["--verb", "600", file.display()])?.stdout.find("mode of") != null
}

test test_chmod_recursive_grants_access_before_descent { |ctx|
  let root = test.temp_dir(ctx, name: "inaccessible")?
  let child = fp"{root}/child"
  child.write("x")
  child.chmod(0o000)
  root.chmod(0o000)
  let result = perm_run(ctx, ["-R", "u+rwX", root.display()])?
  assert result.status == 0, result.stderr
  assert fs.stat(root)?.mode.bit_and(0o700) == 0o700
  assert fs.stat(child)?.mode.bit_and(0o700) == 0o600
}

test test_chmod_no_dereference_skips_symlink { |ctx|
  let root = test.temp_dir(ctx, name: "links")?
  let file = fp"{root}/file"
  let link = fp"{root}/link"
  file.write("x")
  file.chmod(0o640)
  link.symlink(to: file)
  assert perm_run(ctx, ["--no-dereference", "000", link.display()])?.status == 0
  assert fs.stat(file)?.mode.bit_and(0o7777) == 0o640
}

test test_chmod_verbose_uses_four_octal_digits_and_relative_descendants { |ctx|
  let root = test.temp_dir(ctx, name: "relative-mode")?
  fp"{root}/dir".mkdir()
  fp"{root}/dir/child".write("x")
  fp"{root}/dir/child".chmod(0o644)
  let output = fp"{root}/output"
  cd $root {
    let result = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/chmod.xsh" -- -Rv 600 dir
    output.write(result)
  }
  fp"{root}/dir".chmod(0o700)
  assert output.read_text()?.find("mode of 'dir/child' changed from 0644 (rw-r--r--) to 0600 (rw-------)") != null
}

test test_chmod_verbose_missing_file_reports_access_failure { |ctx|
  let root = test.temp_dir(ctx, name: "missing-mode")?
  let result = perm_run(ctx, ["-v", "755", f"{root}/missing"])?
  assert result.status == 1
  assert result.stdout.find("could not be accessed") != null
}

test test_chmod_multiple_option_modes_accumulate_actions { |ctx|
  let target = test.temp_file(ctx, name: "mode-actions", contents: b"x")?
  target.chmod(0o777)
  let result = perm_run(ctx, ["-w", "-x", target.display()])?
  assert fs.stat(target)?.mode.bit_and(0o7777) == 0o777.clear_bits(0o333.clear_bits(fs.umask()?))
  assert result.status == (if fs.umask()?.bit_and(0o333) != 0 { 1 } else { 0 })
}

test test_chmod_traversal_is_independent_of_link_mutation { |ctx|
  let root = test.temp_dir(ctx, name: "link-descent")?
  let tree = fp"{root}/tree"
  tree.mkdir()
  let child = fp"{tree}/child"
  child.write("x")
  child.chmod(0o644)
  let link = fp"{root}/link"
  link.symlink(to: tree)
  let directory_mode = fs.stat(tree)?.mode.bit_and(0o7777)
  assert perm_run(ctx, ["-RL", "--no-dereference", "600", link.display()])?.status == 0
  assert fs.stat(child)?.mode.bit_and(0o7777) == 0o600
  assert fs.stat(tree)?.mode.bit_and(0o7777) == directory_mode
}

test test_chmod_recursive_reports_search_permission_failure { |ctx|
  if user.current()?.uid == 0 { test.skip("root bypasses directory search permissions") }
  let root = test.temp_dir(ctx, name: "searchless")?
  let blocked = fp"{root}/blocked"
  blocked.mkdir()
  fp"{blocked}/child".write("x")
  blocked.chmod(0o400)
  let result = perm_run(ctx, ["-R", "a+r", root.display()])?
  blocked.chmod(0o700)
  assert result.status == 1
  assert result.stderr.find("Permission denied") != null
}

test test_chmod_recursive_reports_inaccessible_descendant { |ctx|
  if user.current()?.uid == 0 { test.skip("root bypasses directory search permissions") }
  let root = test.temp_dir(ctx, name: "inaccessible-child")?
  let blocked = fp"{root}/blocked"
  let child = fp"{blocked}/child"
  blocked.mkdir()
  child.mkdir()
  fp"{child}/file".write("x")
  blocked.chmod(0o655)
  let result = perm_run(ctx, ["-R", "o=r", root.display()])?
  blocked.chmod(0o700)
  assert result.status == 1
  assert result.stderr.find(f"cannot access '{child}'") != null, result.stderr
  assert result.stderr.find(f"cannot read directory '{blocked}'") == null, result.stderr
}

test test_chmod_recursive_preserves_non_utf8_child_names { |ctx|
  let root = test.temp_dir(ctx, name: "raw-name")?
  let child = Path.parse_bytes(bytes.concat([root.bytes(), b"/child\xff"]))?
  child.write("x")
  child.chmod(0o644)
  let result = perm_run(ctx, ["-R", "700", root.display()])?
  assert result.status == 0, result.stderr
  assert fs.stat(child)?.mode.bit_and(0o7777) == 0o700
}
