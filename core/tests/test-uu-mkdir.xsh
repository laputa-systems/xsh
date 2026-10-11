##! Transcribed from the uutils coreutils mkdir integration tests.

use support.uu as uu

# origin: uutils test_mkdir::test_invalid_arg
test test_uu_mkdir_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mkdir", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_mkdir::test_mkdir_mkdir
test test_uu_mkdir_mkdir_mkdir { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mkdir", ["test_dir"])?
  uu.succeeds(r)
}

# origin: uutils test_mkdir::test_mkdir_mode
test test_uu_mkdir_mkdir_mode { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mkdir", ["-m", "755", "test_dir"])?
  uu.succeeds(r)
}

# origin: uutils test_mkdir::test_mkdir_no_parent
test test_uu_mkdir_mkdir_no_parent { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mkdir", ["parent_dir/child_dir"])?
  uu.fails(r)
}

# origin: uutils test_mkdir::test_mkdir_verbose
test test_uu_mkdir_mkdir_verbose { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mkdir", ["test_dir", "-v"])?
  uu.succeeds(r)
  uu.stdout_is(r, "mkdir: created directory 'test_dir'\n")
}

# origin: uutils test_mkdir::test_mkdir_dup_dir
test test_uu_mkdir_mkdir_dup_dir { |ctx|
  let s = uu.scene(ctx)?
  var r = uu.invoke(s, "mkdir", ["test_dir"])?
  uu.succeeds(r)
  r = uu.invoke(s, "mkdir", ["test_dir"])?
  uu.fails(r)
}

# origin: uutils test_mkdir::test_mkdir_dup_dir_parent
test test_uu_mkdir_mkdir_dup_dir_parent { |ctx|
  let s = uu.scene(ctx)?
  var r = uu.invoke(s, "mkdir", ["test_dir"])?
  uu.succeeds(r)
  r = uu.invoke(s, "mkdir", ["-p", "test_dir"])?
  uu.succeeds(r)
}

# origin: uutils test_mkdir::test_mkdir_dup_file
test test_uu_mkdir_mkdir_dup_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_file.txt")?
  var r = uu.invoke(s, "mkdir", ["test_file.txt"])?
  uu.fails(r)
  r = uu.invoke(s, "mkdir", ["-p", "test_file.txt"])?
  uu.fails(r)
}

# origin: uutils test_mkdir::test_mkdir_parent
test test_uu_mkdir_mkdir_parent { |ctx|
  let s = uu.scene(ctx)?
  for flags in [["-p"], ["-p", "-p"], ["--parent"], ["--parent", "--parent"], ["--parents"], ["--parents", "--parents"]] {
    uu.succeeds(uu.invoke(s, "mkdir", flags.extend(["parent_dir/child_dir"]))?)
  }
}

# origin: uutils test_mkdir::test_symbolic_mode
test test_uu_mkdir_symbolic_mode { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mkdir", ["-m", "a=rwx", "test_dir"])?
  uu.succeeds(r)
  assert fs.stat(uu.at(s, "test_dir"))?.mode == 0o40777
}

# origin: uutils test_mkdir::test_multi_symbolic
test test_uu_mkdir_multi_symbolic { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mkdir", ["-m", "u=rwx,g=rx,o=", "test_dir"])?
  uu.succeeds(r)
  assert fs.stat(uu.at(s, "test_dir"))?.mode == 0o40750
}

# origin: uutils test_mkdir::test_empty_argument
test test_uu_mkdir_empty_argument { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mkdir", [""])?
  uu.fails(r)
  uu.stderr_only(r, "mkdir: cannot create directory '': No such file or directory\n")
}

# origin: uutils test_mkdir::test_mkdir_inside_inexistent_dir
test test_uu_mkdir_mkdir_inside_inexistent_dir { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mkdir", ["a/b"])?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "mkdir: cannot create directory 'a/b': No such file or directory\n")
}

# origin: uutils test_mkdir::diagnostics::test_plain_message_when_stderr_is_a_pipe
test test_uu_mkdir_diagnostics_plain_message_when_stderr_is_a_pipe { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mkdir", ["-m", "u+rw?x", "some_dir"])?
  uu.fails_with_code(r, 1)
  let stderr = r.stderr.utf8()?
  assert stderr.starts_with("mkdir: ")
  assert ! (":1:" in stderr)
}

# origin: uutils test_mkdir::test_mkdir_deep_nesting
test test_uu_mkdir_mkdir_deep_nesting { |ctx|
  let s = uu.scene(ctx)?
  let operand = ["d" for _ in range(350)].join("/")
  uu.succeeds(uu.invoke(s, "mkdir", ["-p", operand])?)
  assert uu.dir_exists(s, operand)?
}

# origin: uutils test_mkdir::test_mkdir_dot_components
test test_uu_mkdir_mkdir_dot_components { |ctx|
  let s = uu.scene(ctx)?
  var r = uu.invoke(s, "mkdir", ["-p", "test/././test2/././test3/././test4"])?
  uu.succeeds(r)
  assert uu.dir_exists(s, "test/test2/test3/test4")?
  r = uu.invoke(s, "mkdir", ["-p", "./test_dot/test_dot2"])?
  uu.succeeds(r)
  assert uu.dir_exists(s, "test_dot/test_dot2")?
  r = uu.invoke(s, "mkdir", ["-p", "mixed/./normal/./path"])?
  uu.succeeds(r)
  assert uu.dir_exists(s, "mixed/normal/path")?
}

# origin: uutils test_mkdir::test_mkdir_mixed_special_components
test test_uu_mkdir_mkdir_mixed_special_components { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mkdir", ["-p", "./start/./middle/../end/./final"])?
  uu.succeeds(r)
  assert uu.dir_exists(s, "start/end/final")?
}

# origin: uutils test_mkdir::test_mkdir_parent_dir_components
test test_uu_mkdir_mkdir_parent_dir_components { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "base/child")?
  var r = uu.invoke(s, "mkdir", ["-p", "base/child/../sibling"])?
  uu.succeeds(r)
  assert uu.dir_exists(s, "base/sibling")?
  r = uu.invoke(s, "mkdir", ["-p", "base/child/../../other"])?
  uu.succeeds(r)
  assert uu.dir_exists(s, "other")?
  r = uu.invoke(s, "mkdir", ["-p", "base/child/../sibling"])?
  uu.succeeds(r)
  assert uu.dir_exists(s, "base/sibling")?
}

# origin: uutils test_mkdir::test_mkdir_trailing_spaces_and_dots
test test_uu_mkdir_mkdir_trailing_spaces_and_dots { |ctx|
  let s = uu.scene(ctx)?
  for operand in ["test ", "test   ", ".hidden", "test.", "...test"] {
    uu.succeeds(uu.invoke(s, "mkdir", ["-p", operand])?)
    assert uu.dir_exists(s, operand)?
  }
}

# origin: uutils test_mkdir::test_mkdir_reserved_device_names
test test_uu_mkdir_mkdir_reserved_device_names { |ctx|
  let s = uu.scene(ctx)?
  for operand in ["CON", "PRN", "AUX", "COM1", "LPT1"] {
    let r = uu.invoke(s, "mkdir", ["-p", operand])?
    if r.status == 0 { assert uu.dir_exists(s, operand)? }
  }
}

# origin: uutils test_mkdir::test_mkdir_case_sensitivity
test test_uu_mkdir_mkdir_case_sensitivity { |ctx|
  let s = uu.scene(ctx)?
  var r = uu.invoke(s, "mkdir", ["-p", "CaseTest"])?
  uu.succeeds(r)
  assert uu.dir_exists(s, "CaseTest")?
  r = uu.invoke(s, "mkdir", ["-p", "casetest"])?
  if r.status == 0 {
    assert uu.dir_exists(s, "CaseTest")?
    assert uu.dir_exists(s, "casetest")?
  } else { assert uu.dir_exists(s, "CaseTest")? }
  r = uu.invoke(s, "mkdir", ["-p", "CASETEST"])?
  uu.succeeds(r)
  r = uu.invoke(s, "mkdir", ["-p", "caseTEST"])?
  uu.succeeds(r)
}

# origin: uutils test_mkdir::test_mkdir_network_paths
test test_uu_mkdir_mkdir_network_paths { |ctx|
  let s = uu.scene(ctx)?
  var r = uu.invoke(s, "mkdir", ["-p", "//server/share/test"])?
  if r.status == 0 { assert p"//server/share/test".is_dir()? }
  r = uu.invoke(s, "mkdir", ["-p", "server_share_test"])?
  uu.succeeds(r)
  assert uu.dir_exists(s, "server_share_test")?
  r = uu.invoke(s, "mkdir", ["-p", "test//double//slash"])?
  uu.succeeds(r)
  assert uu.dir_exists(s, "test//double//slash")?
}

# origin: uutils test_mkdir::test_mkdir_environment_expansion
test test_uu_mkdir_mkdir_environment_expansion { |ctx|
  let s = uu.scene(ctx)?
  for operand in ["$TEST_VAR/dir", r"${TEST_VAR}_braced/dir", "~/test_dir"] {
    uu.succeeds(uu.invoke(s, "mkdir", ["-p", operand], vars: {TEST_VAR: "expanded_value"})?)
    assert uu.dir_exists(s, operand)?
  }
  assert ! uu.exists(s, "expanded_value/dir")?
}

# origin: uutils test_mkdir::test_mkdir_control_characters
test test_uu_mkdir_mkdir_control_characters { |ctx|
  let s = uu.scene(ctx)?
  for operand in ["test\nname", "test\tname"] {
    var r = uu.invoke(s, "mkdir", ["-p", operand])?
    if r.status == 0 { assert uu.dir_exists(s, operand)? }
  }
  var r = uu.invoke(s, "mkdir", ["-p", "test name"])?
  uu.succeeds(r)
  assert uu.dir_exists(s, "test name")?
  r = uu.invoke(s, "mkdir", ["-pv", "a/\"\"/b/c"])?
  uu.succeeds(r)
  assert uu.dir_exists(s, "a/\"\"/b/c")?
  r = uu.invoke(s, "mkdir", ["-p", "a/''/b/c"])?
  uu.succeeds(r)
  assert uu.dir_exists(s, "a/''/b/c")?
}

# origin: uutils test_mkdir::test_mkdir_maximum_path_length
test test_uu_mkdir_mkdir_maximum_path_length { |ctx|
  let s = uu.scene(ctx)?
  let long_path = [["a" for _ in range(50)].join(""), ["b" for _ in range(50)].join(""), ["c" for _ in range(50)].join("")].join("/")
  let longer_path = [["x" for _ in range(100)].join(""), ["y" for _ in range(50)].join(""), ["z" for _ in range(30)].join("")].join("/")
  for operand in [long_path, longer_path] {
    uu.succeeds(uu.invoke(s, "mkdir", ["-p", operand])?)
    assert uu.dir_exists(s, operand)?
  }
  let very_long_path = [["very_long_directory_name_" for _ in range(20)].join(""), "final"].join("/")
  let r = uu.invoke(s, "mkdir", ["-p", very_long_path])?
  if r.status == 0 { assert uu.dir_exists(s, very_long_path)? }
}

# origin: uutils test_mkdir::test_mkdir_trailing_dot
test test_uu_mkdir_mkdir_trailing_dot { |ctx|
  let s = uu.scene(ctx)?
  var r = uu.invoke(s, "mkdir", ["-p", "-v", "test_dir"])?
  uu.succeeds(r)
  var separate = uu.scene(ctx)?
  r = uu.invoke(separate, "mkdir", ["-p", "-v", "test_dir_a/."])?
  uu.succeeds(r)
  uu.stdout_contains(r, "created directory 'test_dir_a'")
  separate = uu.scene(ctx)?
  r = uu.invoke(separate, "mkdir", ["-p", "-v", "test_dir_b/.."])?
  uu.succeeds(r)
  uu.stdout_contains(r, "created directory 'test_dir_b'")
  let listing = uu.scene(ctx)?
  let listed = uu.invoke(listing, "ls", ["-al"])?
  print listed.stdout.utf8()?
}

# origin: uutils test_mkdir::test_mkdir_trailing_dot_and_slash
test test_uu_mkdir_mkdir_trailing_dot_and_slash { |ctx|
  let s = uu.scene(ctx)?
  var r = uu.invoke(s, "mkdir", ["-p", "-v", "test_dir"])?
  uu.succeeds(r)
  let separate = uu.scene(ctx)?
  r = uu.invoke(separate, "mkdir", ["-p", "-v", "test_dir_a/./"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "created directory 'test_dir_a'")
  let listing = uu.scene(ctx)?
  let listed = uu.invoke(listing, "ls", ["-al"])?
  print listed.stdout.utf8()?
}

# origin: uutils test_mkdir::test_recursive_reporting
test test_uu_mkdir_recursive_reporting { |ctx|
  let s = uu.scene(ctx)?
  var operand = "test_dir/test_dir_a/test_dir_b"
  var r = uu.invoke(s, "mkdir", ["-p", "-v", operand])?
  uu.succeeds(r)
  for created in ["test_dir", "test_dir/test_dir_a", operand] { uu.stdout_contains(r, f"created directory '{created}'") }
  var separate = uu.scene(ctx)?
  r = uu.invoke(separate, "mkdir", ["-v", operand])?
  uu.fails(r)
  uu.no_stdout(r)
  separate = uu.scene(ctx)?
  operand = "test_dir/../test_dir_a/../test_dir_b"
  r = uu.invoke(separate, "mkdir", ["-p", "-v", operand])?
  uu.succeeds(r)
  for created in ["test_dir", "test_dir/../test_dir_a", operand] { uu.stdout_contains(r, f"created directory '{created}'") }
}

# origin: uutils test_mkdir::test_symbolic_alteration
test test_uu_mkdir_symbolic_alteration { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mkdir", ["-m", "-w", "test_dir"], umask: 0o022)?
  uu.succeeds(r)
  assert fs.stat(uu.at(s, "test_dir"))?.mode == 0o40577
}

# origin: uutils test_mkdir::test_mkdir_parent_mode
test test_uu_mkdir_mkdir_parent_mode { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mkdir", ["-p", "a/b"], umask: 0o160)?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.dir_exists(s, "a")?
  assert fs.stat(uu.at(s, "a"))?.mode == 0o40717
  assert uu.dir_exists(s, "a/b")?
  assert fs.stat(uu.at(s, "a/b"))?.mode == 0o40617
}

# origin: uutils test_mkdir::test_mkdir_parent_mode_check_existing_parent
test test_uu_mkdir_mkdir_parent_mode_check_existing_parent { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a")?
  let existing = fs.stat(uu.at(s, "a"))?.mode
  let r = uu.invoke(s, "mkdir", ["-p", "a/b/c"], umask: 0o160)?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.dir_exists(s, "a")?
  assert fs.stat(uu.at(s, "a"))?.mode == existing
  assert uu.dir_exists(s, "a/b")?
  assert fs.stat(uu.at(s, "a/b"))?.mode == 0o40717
  assert uu.dir_exists(s, "a/b/c")?
  assert fs.stat(uu.at(s, "a/b/c"))?.mode == 0o40617
}

# origin: uutils test_mkdir::test_mkdir_parent_mode_skip_existing_last_component_chmod
test test_uu_mkdir_mkdir_parent_mode_skip_existing_last_component_chmod { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a/b")?
  uu.set_mode(s, "a/b", 0)?
  let r = uu.invoke(s, "mkdir", ["-p", "a/b"], umask: 0o160)?
  uu.succeeds(r)
  uu.no_output(r)
  assert fs.stat(uu.at(s, "a/b"))?.mode == 0o40000
}

# origin: uutils test_mkdir::test_mkdir_p_respects_umask_without_acl
test test_uu_mkdir_mkdir_p_respects_umask_without_acl { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mkdir", ["-p", "a/b/c"], umask: 0o022)?
  uu.succeeds(r)
  assert uu.mode(s, "a/b/c")? == 0o755
}

# origin: uutils test_mkdir::test_mkdir_explicit_mode_zero
test test_uu_mkdir_mkdir_explicit_mode_zero { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mkdir", ["-m", "0", "d"], umask: 0o022)?
  uu.succeeds(r)
  assert uu.mode(s, "d")? == 0
}

# origin: uutils test_mkdir::test_mkdir_explicit_mode_with_umask
test test_uu_mkdir_mkdir_explicit_mode_with_umask { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mkdir", ["-m", "777", "d"], umask: 0o077)?
  uu.succeeds(r)
  assert uu.mode(s, "d")? == 0o777
}

# origin: uutils test_mkdir::test_umask_compliance
test test_uu_mkdir_umask_compliance { |ctx|
  let s = uu.scene(ctx)?
  for mask in range(0o027) {
    let separate = uu.scene(ctx)?
    uu.succeeds(uu.invoke(separate, "mkdir", ["test_dir"], umask: mask)?)
    assert fs.stat(uu.at(separate, "test_dir"))?.mode == 0o40000 + 0o777.clear_bits(mask)
  }
}

# origin: uutils test_mkdir::test_mkdir_mode_ignores_umask
test test_uu_mkdir_mkdir_mode_ignores_umask { |ctx|
  let s = uu.scene(ctx)?
  for case in [{spec: "0700", name: "test_700", mask: 0o077, expected: 0o40700}, {spec: "0777", name: "test_777", mask: 0o022, expected: 0o40777}, {spec: "0755", name: "test_755", mask: 0o077, expected: 0o40755}, {spec: "a=rwx", name: "test_symbolic", mask: 0o022, expected: 0o40777}] {
    let separate = uu.scene(ctx)?
    uu.succeeds(uu.invoke(separate, "mkdir", ["-m", case.spec, case.name], umask: case.mask)?)
    assert fs.stat(uu.at(separate, case.name))?.mode == case.expected
  }
}

# origin: uutils test_mkdir::test_mkdir_parent_mode_with_explicit_mode
test test_uu_mkdir_mkdir_parent_mode_with_explicit_mode { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mkdir", ["-p", "-m", "0700", "parent/child/target"], umask: 0o022)?
  uu.succeeds(r)
  assert fs.stat(uu.at(s, "parent"))?.mode == 0o40755
  assert fs.stat(uu.at(s, "parent/child"))?.mode == 0o40755
  assert fs.stat(uu.at(s, "parent/child/target"))?.mode == 0o40700
}

# origin: uutils test_mkdir::test_mkdir_parent_inherits_setgid
test test_uu_mkdir_mkdir_parent_inherits_setgid { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "parent")?
  uu.set_mode(s, "parent", 0o2755)?
  let r = uu.invoke(s, "mkdir", ["-p", "parent/child/grandchild"])?
  uu.succeeds(r)
  uu.no_output(r)
  for name in ["parent", "parent/child", "parent/child/grandchild"] {
    assert fs.stat(uu.at(s, name))?.mode.bit_and(0o2000) == 0o2000
  }
}

# origin: uutils test_mkdir::test_mkdir_acl
test test_uu_mkdir_mkdir_acl { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a")?
  fs.xattr_set(uu.at(s, "a"), "system.posix_acl_default", bytes.from_ints([2, 0, 0, 0, 1, 0, 7, 0, 255, 255, 255, 255, 4, 0, 7, 0, 255, 255, 255, 255, 32, 0, 5, 0, 255, 255, 255, 255])?)?
  let r = uu.invoke(s, "mkdir", ["-p", "a/b"], umask: 119)?
  uu.succeeds(r)
  assert fs.stat(uu.at(s, "a/b"))?.mode == 16893
}

# origin: uutils test_mkdir::test_mkdir_acl_inheritance_with_restrictive_mask
test test_uu_mkdir_mkdir_acl_inheritance_with_restrictive_mask { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "parent")?
  fs.xattr_set(uu.at(s, "parent"), "system.posix_acl_default", bytes.from_ints([2, 0, 0, 0, 1, 0, 7, 0, 255, 255, 255, 255, 4, 0, 7, 0, 255, 255, 255, 255, 16, 0, 5, 0, 255, 255, 255, 255, 32, 0, 0, 0, 255, 255, 255, 255])?)?
  let r = uu.invoke(s, "mkdir", ["-p", "parent/child"], umask: 0o000)?
  uu.succeeds(r)
  assert fs.stat(uu.at(s, "parent/child"))?.mode.bit_and(0o777) == 0o750
  assert "system.posix_acl_access" in fs.xattr_list(uu.at(s, "parent/child"))?
}

# origin: uutils test_mkdir::test_mkdir_concurrent_creation
test test_uu_mkdir_mkdir_concurrent_creation { |ctx|
  let s = uu.scene(ctx)?
  for round in range(10) {
    let separate = uu.scene(ctx)?
    let target = uu.at(separate, ["concurrent_test", ["a" for _ in range(41)].join("/")].join("/"))
    let jobs = [spawn (uu.command(separate, "mkdir", ["-p", target.display()],
      stdout: uu.at(separate, f"stdout-{worker}"), stderr: uu.at(separate, f"stderr-{worker}"), timeout: 30s)?)? for worker in range(8)]
    for job in jobs {
      assert (wait job?).exited_with(0)
    }
    assert target.is_dir()?
  }
}

# origin: uutils test_mkdir::test_mkdir_concurrent_non_recursive
test test_uu_mkdir_mkdir_concurrent_non_recursive { |ctx|
  let s = uu.scene(ctx)?
  for round in range(10) {
    let separate = uu.scene(ctx)?
    let target = uu.at(separate, f"concurrent_target_{round}")
    let jobs = [spawn (uu.command(separate, "mkdir", [target.display()],
      stdout: uu.at(separate, f"stdout-{worker}"), stderr: uu.at(separate, f"stderr-{worker}"), timeout: 30s)?)? for worker in range(16)]
    var winners = 0
    for job in jobs {
      if (wait job?).exited_with(0) { winners += 1 }
    }
    assert winners == 1, f"round {round}: expected exactly 1 winner for concurrent non-recursive mkdir"
    assert target.is_dir()?
  }
}
