##! Transcribed ownership tests from the uutils coreutils integration suite.

use support.uu as uu

# Keep captures in the private scene while reproducing a child working at root.
proc at_root(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let out = uu.at(s, "root-stdout")
  let err = uu.at(s, "root-stderr")
  let r = uu.invoke({ctx: s.ctx, root: p"/"}, "chgrp", args, stdout: out, stderr: err, timeout: 5s)?
  Ok({util: r.util, args: r.args, status: r.status, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}

# origin: uutils test_chgrp::test_invalid_option
test test_uu_chgrp_invalid_option { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "chgrp", ["-w", "/"])?
  uu.fails(r)

}

# origin: uutils test_chgrp::test_invalid_arg
test test_uu_chgrp_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "chgrp", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)

}

# origin: uutils test_chgrp::test_invalid_group
test test_uu_chgrp_invalid_group { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "chgrp", ["__nosuchgroup__", "/"])?
  uu.fails(r)
  uu.stderr_is(r, "chgrp: invalid group: '__nosuchgroup__'\n")
}

# origin: uutils test_chgrp::test_error_1
test test_uu_chgrp_error_1 { |ctx|
  let s = uu.scene(ctx)?
  if unix.id()?.egid == 0 { test.skip("requires a non-root effective group") }
  let r = uu.invoke(s, "chgrp", ["bin", "/dev"])?
  uu.fails(r)
  uu.stderr_contains(r, "chgrp: changing group of '/dev': ")

}

# origin: uutils test_chgrp::test_fail_silently
test test_uu_chgrp_fail_silently { |ctx|
  let s = uu.scene(ctx)?
  if unix.id()?.egid == 0 { test.skip("requires a non-root effective group") }
  let r0 = uu.invoke(s, "chgrp", ["-f", "bin", "/dev"])?
  uu.fails(r0)
  uu.no_output(r0)

  let r1 = uu.invoke(s, "chgrp", ["--silent", "bin", "/dev"])?
  uu.fails(r1)
  uu.no_output(r1)

  let r2 = uu.invoke(s, "chgrp", ["--quiet", "bin", "/dev"])?
  uu.fails(r2)
  uu.no_output(r2)

  let r3 = uu.invoke(s, "chgrp", ["--sil", "bin", "/dev"])?
  uu.fails(r3)
  uu.no_output(r3)

  let r4 = uu.invoke(s, "chgrp", ["--qui", "bin", "/dev"])?
  uu.fails(r4)
  uu.no_output(r4)
}

# origin: uutils test_chgrp::test_preserve_root
test test_uu_chgrp_preserve_root { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "chgrp", ["--preserve-root", "-R", "bin", "/"], timeout: 5s)?
  uu.fails(r0)
  uu.stderr_is(r0, "chgrp: it is dangerous to operate recursively on '/'\nchgrp: use --no-preserve-root to override this failsafe\n")

  let r1 = uu.invoke(s, "chgrp", ["--preserve-root", "-R", "bin", "/////dev///../../../../"], timeout: 5s)?
  uu.fails(r1)
  uu.stderr_is(r1, "chgrp: it is dangerous to operate recursively on '/////dev///../../../../' (same as '/')\nchgrp: use --no-preserve-root to override this failsafe\n")

  let r2 = uu.invoke(s, "chgrp", ["--preserve-root", "-R", "bin", "../../../../../../../../../../../../../../"], timeout: 5s)?
  uu.fails(r2)
  uu.stderr_is(r2, "chgrp: it is dangerous to operate recursively on '../../../../../../../../../../../../../../' (same as '/')\nchgrp: use --no-preserve-root to override this failsafe\n")

  let r3 = uu.invoke(s, "chgrp", ["--preserve-root", "-R", "bin", "./../../../../../../../../../../../../../../"], timeout: 5s)?
  uu.fails(r3)
  uu.stderr_is(r3, "chgrp: it is dangerous to operate recursively on './../../../../../../../../../../../../../../' (same as '/')\nchgrp: use --no-preserve-root to override this failsafe\n")
}

# origin: uutils test_chgrp::test_preserve_root_symlink
test test_uu_chgrp_preserve_root_symlink { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, "/", "test_chgrp_symlink2root")?
  let r0 = uu.invoke(s, "chgrp", ["--preserve-root", "-HR", "bin", "test_chgrp_symlink2root"], timeout: 5s)?
  uu.fails(r0)
  uu.stderr_is(r0, "chgrp: it is dangerous to operate recursively on 'test_chgrp_symlink2root' (same as '/')\nchgrp: use --no-preserve-root to override this failsafe\n")
  uu.remove(s, "test_chgrp_symlink2root")?
  uu.symlink(s, "//", "test_chgrp_symlink2root")?
  let r1 = uu.invoke(s, "chgrp", ["--preserve-root", "-HR", "bin", "test_chgrp_symlink2root"], timeout: 5s)?
  uu.fails(r1)
  uu.stderr_is(r1, "chgrp: it is dangerous to operate recursively on 'test_chgrp_symlink2root' (same as '/')\nchgrp: use --no-preserve-root to override this failsafe\n")
  uu.remove(s, "test_chgrp_symlink2root")?
  uu.symlink(s, "///", "test_chgrp_symlink2root")?
  let r2 = uu.invoke(s, "chgrp", ["--preserve-root", "-HR", "bin", "test_chgrp_symlink2root"], timeout: 5s)?
  uu.fails(r2)
  uu.stderr_is(r2, "chgrp: it is dangerous to operate recursively on 'test_chgrp_symlink2root' (same as '/')\nchgrp: use --no-preserve-root to override this failsafe\n")
  uu.remove(s, "test_chgrp_symlink2root")?
  uu.symlink(s, "////dev//../../../../", "test_chgrp_symlink2root")?
  let r3 = uu.invoke(s, "chgrp", ["--preserve-root", "-HR", "bin", "test_chgrp_symlink2root"], timeout: 5s)?
  uu.fails(r3)
  uu.stderr_is(r3, "chgrp: it is dangerous to operate recursively on 'test_chgrp_symlink2root' (same as '/')\nchgrp: use --no-preserve-root to override this failsafe\n")
  uu.remove(s, "test_chgrp_symlink2root")?
  uu.symlink(s, uu.at(s, "..//../../..//../..//../../../../../../../../").display(), "test_chgrp_symlink2root")?
  let r4 = uu.invoke(s, "chgrp", ["--preserve-root", "-HR", "bin", "test_chgrp_symlink2root"], timeout: 5s)?
  uu.fails(r4)
  uu.stderr_is(r4, "chgrp: it is dangerous to operate recursively on 'test_chgrp_symlink2root' (same as '/')\nchgrp: use --no-preserve-root to override this failsafe\n")
  uu.remove(s, "test_chgrp_symlink2root")?
  uu.symlink(s, uu.at(s, ".//../../../../../../..//../../../../../../../").display(), "test_chgrp_symlink2root")?
  let r5 = uu.invoke(s, "chgrp", ["--preserve-root", "-HR", "bin", "test_chgrp_symlink2root"], timeout: 5s)?
  uu.fails(r5)
  uu.stderr_is(r5, "chgrp: it is dangerous to operate recursively on 'test_chgrp_symlink2root' (same as '/')\nchgrp: use --no-preserve-root to override this failsafe\n")
  uu.remove(s, "test_chgrp_symlink2root")?
  uu.symlink(s, "///dev", "test_chgrp_symlink2root")?
  let r6 = uu.invoke(s, "chgrp", ["--preserve-root", "-HR", "bin", ".//test_chgrp_symlink2root/..//..//../../"], timeout: 5s)?
  uu.fails(r6)
  uu.stderr_is(r6, "chgrp: it is dangerous to operate recursively on './/test_chgrp_symlink2root/..//..//../../' (same as '/')\nchgrp: use --no-preserve-root to override this failsafe\n")
  uu.symlink(s, "/", "__root__")?
  let r7 = uu.invoke(s, "chgrp", ["--preserve-root", "-R", "bin", "__root__/."], timeout: 5s)?
  uu.fails(r7)
  uu.stderr_is(r7, "chgrp: it is dangerous to operate recursively on '__root__/.' (same as '/')\nchgrp: use --no-preserve-root to override this failsafe\n")
}

# origin: uutils test_chgrp::test_preserve_root_symlink_cwd_root
test test_uu_chgrp_preserve_root_symlink_cwd_root { |ctx|
  let s = uu.scene(ctx)?
  let r0 = at_root(s, ["--preserve-root", "-R", "bin", "."])?
  uu.fails(r0)
  uu.stderr_is(r0, "chgrp: it is dangerous to operate recursively on '.' (same as '/')\nchgrp: use --no-preserve-root to override this failsafe\n")
  let r1 = at_root(s, ["--preserve-root", "-R", "bin", "/."])?
  uu.fails(r1)
  uu.stderr_is(r1, "chgrp: it is dangerous to operate recursively on '/.' (same as '/')\nchgrp: use --no-preserve-root to override this failsafe\n")
  let r2 = at_root(s, ["--preserve-root", "-R", "bin", ".."])?
  uu.fails(r2)
  uu.stderr_is(r2, "chgrp: it is dangerous to operate recursively on '..' (same as '/')\nchgrp: use --no-preserve-root to override this failsafe\n")
  let r3 = at_root(s, ["--preserve-root", "-R", "bin", "/.."])?
  uu.fails(r3)
  uu.stderr_is(r3, "chgrp: it is dangerous to operate recursively on '/..' (same as '/')\nchgrp: use --no-preserve-root to override this failsafe\n")
  let r4 = at_root(s, ["--preserve-root", "-R", "bin", "..."])?
  uu.fails(r4)
  uu.stderr_is(r4, "chgrp: cannot access '...': No such file or directory\n")
}

# origin: uutils test_chgrp::test_reference
test test_uu_chgrp_reference { |ctx|
  let s = uu.scene(ctx)?
  if unix.id()?.egid == 0 { test.skip("requires a non-root effective group") }
  let r = uu.invoke(s, "chgrp", ["-v", "--reference=/etc/passwd", "/etc"])?
  uu.fails(r)
  uu.stderr_contains(r, "chgrp: changing group of '/etc': Operation not permitted\n")
  uu.stdout_contains(r, "failed to change group of '/etc' from ")

}

# origin: uutils test_chgrp::test_reference_multi_no_equal
test test_uu_chgrp_reference_multi_no_equal { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "chgrp", "ref_file", "ref_file")?
  uu.fixture(s, "chgrp", "file1", "file1")?
  uu.fixture(s, "chgrp", "file2", "file2")?
  let r = uu.invoke(s, "chgrp", ["-v", "--reference", "ref_file", "file1", "file2"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "group of 'file1' retained as ")
  uu.stdout_contains(r, "\ngroup of 'file2' retained as ")
  uu.no_stderr(r)
}

# origin: uutils test_chgrp::test_reference_last
test test_uu_chgrp_reference_last { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "chgrp", "ref_file", "ref_file")?
  uu.fixture(s, "chgrp", "file1", "file1")?
  uu.fixture(s, "chgrp", "file2", "file2")?
  uu.fixture(s, "chgrp", "file3", "file3")?
  let r = uu.invoke(s, "chgrp", ["-v", "file1", "file2", "file3", "--reference", "ref_file"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "group of 'file1' retained as ")
  uu.stdout_contains(r, "\ngroup of 'file2' retained as ")
  uu.stdout_contains(r, "\ngroup of 'file3' retained as ")
  uu.no_stderr(r)
}

# origin: uutils test_chgrp::test_big_p
test test_uu_chgrp_big_p { |ctx|
  let s = uu.scene(ctx)?
  if unix.id()?.egid == 0 { test.skip("requires a non-root effective group") }
  let r = uu.invoke(s, "chgrp", ["-RP", "bin", "/proc/self/cwd"])?
  uu.fails(r)
  uu.stderr_contains(r, "chgrp: changing group of '/proc/self/cwd': Operation not permitted\n")

}

# origin: uutils test_chgrp::test_big_h
test test_uu_chgrp_big_h { |ctx|
  let s = uu.scene(ctx)?
  if unix.id()?.egid == 0 { test.skip("requires a non-root effective group") }
  let r = uu.invoke(s, "chgrp", ["-RH", "bin", "/proc/self/fd"])?
  uu.fails(r)

  assert r.stderr.utf8()?.lines().len() > 1
}

# origin: uutils test_chgrp::basic_succeeds
test test_uu_chgrp_basic_succeeds { |ctx|
  let s = uu.scene(ctx)?
  let groups = unix.id()?.supplementary
  if groups.len() < 1 { test.skip("requires 1 supplementary groups") }
  uu.touch(s, "f1")?
  let r = uu.invoke(s, "chgrp", [f"{groups[0]}", "f1"])?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_chgrp::test_no_change
test test_uu_chgrp_no_change { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  let r = uu.invoke(s, "chgrp", ["", uu.at(s, "file").display()])?
  uu.succeeds(r)

}

# origin: uutils test_chgrp::test_verbosity_messages
test test_uu_chgrp_verbosity_messages { |ctx|
  let s = uu.scene(ctx)?
  let groups = unix.id()?.supplementary
  if groups.len() < 1 { test.skip("requires 1 supplementary groups") }
  uu.touch(s, "ref_file")?
  uu.touch(s, "target_file")?
  let r = uu.invoke(s, "chgrp", ["-v", "--reference=ref_file", "target_file"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "group of 'target_file' retained as ")
  uu.no_stderr(r)
}

# origin: uutils test_chgrp::test_chgrp_non_utf8_paths
test test_uu_chgrp_chgrp_non_utf8_paths { |ctx|
  let s = uu.scene(ctx)?
  let filename = uu.at_bytes(s, b"\xff\xfe")?
  filename.write(b"test content")?
  let gid = unix.id()?.egid
  let r = uu.invoke_paths(s, "chgrp", [Path(f"{gid}"), Path.parse_bytes(b"\xff\xfe")?])?
  uu.succeeds(r)
}

# origin: uutils test_chgrp::test_chgrp_recursive_on_file
test test_uu_chgrp_chgrp_recursive_on_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "regular_file")?
  let gid = unix.id()?.egid
  let r = uu.invoke(s, "chgrp", ["-R", f"{gid}", "regular_file"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert fs.stat(uu.at(s, "regular_file"))?.gid == gid
}

# origin: uutils test_chgrp::test_chgrp_exit_code_not_being_overwritten_by_last_file
test test_uu_chgrp_chgrp_exit_code_not_being_overwritten_by_last_file { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.mkdir(s, "dir/a")?
  uu.mkdir(s, "dir/b")?
  uu.touch(s, "dir/b/file")?
  uu.touch(s, "dir/a/file")?
  uu.set_mode(s, "dir/a", 0o0000)?
  let gid = unix.id()?.egid
  let r = uu.invoke(s, "chgrp", ["-R", f"{gid}", "dir"])?
  uu.fails(r)

  uu.set_mode(s, "dir/a", 0o700)?
}

# origin: uutils test_chgrp::verbose_missing_file_write_error_is_reported_not_panic
test test_uu_chgrp_verbose_missing_file_write_error_is_reported_not_panic { |ctx|
  let s = uu.scene(ctx)?
  let gid = unix.id()?.egid
  let r = uu.invoke(s, "chgrp", ["--verbose", f"{gid}", "does-not-exist"], stdout: p"/dev/full")?
  uu.fails(r)

  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "chgrp: write error: No space left on device")
}

# origin: uutils test_chgrp::test_traverse_symlinks
test test_uu_chgrp_traverse_symlinks { |ctx|
  let s = uu.scene(ctx)?
  let groups = unix.id()?.supplementary
  if groups.len() < 2 { test.skip("requires 2 supplementary groups") }
  let first = groups[0]
  let second = groups[1]
  uu.mkdir(s, "dir")?
  uu.mkdir(s, "dir2")?
  uu.touch(s, "dir2/file")?
  uu.mkdir(s, "dir3")?
  uu.touch(s, "dir3/file")?
  uu.symlink(s, uu.at(s, "dir2").display(), "dir/dir2_ln")?
  uu.symlink(s, uu.at(s, "dir3").display(), "dir3_ln")?
  let setup0 = uu.invoke(s, "chgrp", [f"{first}", "dir2/file", "dir3/file"])?
  uu.succeeds(setup0)

  assert fs.stat(uu.at(s, "dir2/file"))?.gid == first
  assert fs.stat(uu.at(s, "dir3/file"))?.gid == first
  let r0 = uu.invoke(s, "chgrp", ["-R", f"{second}", "dir", "dir3_ln"])?
  uu.succeeds(r0)
  uu.no_stderr(r0)
  assert fs.stat(uu.at(s, "dir2/file"))?.gid == first
  assert fs.stat(uu.at(s, "dir3/file"))?.gid == first
  uu.remove(s, "dir")?
  uu.remove(s, "dir2")?
  uu.remove(s, "dir3")?
  uu.remove(s, "dir3_ln")?
  uu.mkdir(s, "dir")?
  uu.mkdir(s, "dir2")?
  uu.touch(s, "dir2/file")?
  uu.mkdir(s, "dir3")?
  uu.touch(s, "dir3/file")?
  uu.symlink(s, uu.at(s, "dir2").display(), "dir/dir2_ln")?
  uu.symlink(s, uu.at(s, "dir3").display(), "dir3_ln")?
  let setup1 = uu.invoke(s, "chgrp", [f"{first}", "dir2/file", "dir3/file"])?
  uu.succeeds(setup1)

  assert fs.stat(uu.at(s, "dir2/file"))?.gid == first
  assert fs.stat(uu.at(s, "dir3/file"))?.gid == first
  let r1 = uu.invoke(s, "chgrp", ["-R", "-H", f"{second}", "dir", "dir3_ln"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert fs.stat(uu.at(s, "dir2/file"))?.gid == first
  assert fs.stat(uu.at(s, "dir3/file"))?.gid == second
  uu.remove(s, "dir")?
  uu.remove(s, "dir2")?
  uu.remove(s, "dir3")?
  uu.remove(s, "dir3_ln")?
  uu.mkdir(s, "dir")?
  uu.mkdir(s, "dir2")?
  uu.touch(s, "dir2/file")?
  uu.mkdir(s, "dir3")?
  uu.touch(s, "dir3/file")?
  uu.symlink(s, uu.at(s, "dir2").display(), "dir/dir2_ln")?
  uu.symlink(s, uu.at(s, "dir3").display(), "dir3_ln")?
  let setup2 = uu.invoke(s, "chgrp", [f"{first}", "dir2/file", "dir3/file"])?
  uu.succeeds(setup2)

  assert fs.stat(uu.at(s, "dir2/file"))?.gid == first
  assert fs.stat(uu.at(s, "dir3/file"))?.gid == first
  let r2 = uu.invoke(s, "chgrp", ["-R", "-P", f"{second}", "dir", "dir3_ln"])?
  uu.succeeds(r2)
  uu.no_stderr(r2)
  assert fs.stat(uu.at(s, "dir2/file"))?.gid == first
  assert fs.stat(uu.at(s, "dir3/file"))?.gid == first
  uu.remove(s, "dir")?
  uu.remove(s, "dir2")?
  uu.remove(s, "dir3")?
  uu.remove(s, "dir3_ln")?
  uu.mkdir(s, "dir")?
  uu.mkdir(s, "dir2")?
  uu.touch(s, "dir2/file")?
  uu.mkdir(s, "dir3")?
  uu.touch(s, "dir3/file")?
  uu.symlink(s, uu.at(s, "dir2").display(), "dir/dir2_ln")?
  uu.symlink(s, uu.at(s, "dir3").display(), "dir3_ln")?
  let setup3 = uu.invoke(s, "chgrp", [f"{first}", "dir2/file", "dir3/file"])?
  uu.succeeds(setup3)

  assert fs.stat(uu.at(s, "dir2/file"))?.gid == first
  assert fs.stat(uu.at(s, "dir3/file"))?.gid == first
  let r3 = uu.invoke(s, "chgrp", ["-R", "-L", f"{second}", "dir", "dir3_ln"])?
  uu.succeeds(r3)
  uu.no_stderr(r3)
  assert fs.stat(uu.at(s, "dir2/file"))?.gid == second
  assert fs.stat(uu.at(s, "dir3/file"))?.gid == second
  uu.remove(s, "dir")?
  uu.remove(s, "dir2")?
  uu.remove(s, "dir3")?
  uu.remove(s, "dir3_ln")?
}

# origin: uutils test_chgrp::test_from_option
test test_uu_chgrp_from_option { |ctx|
  let s = uu.scene(ctx)?
  let groups = unix.id()?.supplementary
  if groups.len() < 2 { test.skip("requires 2 supplementary groups") }
  let first = groups[0]
  let second = groups[1]
  uu.touch(s, "test_file")?
  let setup = uu.invoke(s, "chgrp", [f"{first}", "test_file"])?
  uu.succeeds(setup)

  # A bare --from value selects the owner, while :GROUP selects the group.
  let r = uu.invoke(s, "chgrp", ["--from", f"{first}", f"{second}", "test_file"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert fs.stat(uu.at(s, "test_file"))?.gid == (if unix.id()?.euid == first { second } else { first })
  let r2 = uu.invoke(s, "chgrp", ["--from", f"{first}", f"{first}", "test_file"])?
  uu.succeeds(r2)
  uu.no_stderr(r2)
  assert fs.stat(uu.at(s, "test_file"))?.gid == first
}

# origin: uutils test_chgrp::test_from_with_invalid_group
test test_uu_chgrp_from_with_invalid_group { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_file")?
  let r0 = uu.invoke(s, "chgrp", ["--from", "nonexistent_group", "nobody", "test_file"])?
  uu.fails(r0)
  uu.stderr_is(r0, "chgrp: invalid user: 'nonexistent_group'\n")

  let r1 = uu.invoke(s, "chgrp", ["--from", "nonexistent_group", "another_nonexistent_group", "test_file"])?
  uu.fails(r1)
  uu.stderr_is(r1, "chgrp: invalid user: 'nonexistent_group'\n")
}

# origin: uutils test_chgrp::test_from_with_reference
test test_uu_chgrp_from_with_reference { |ctx|
  let s = uu.scene(ctx)?
  let groups = unix.id()?.supplementary
  if groups.len() < 2 { test.skip("requires 2 supplementary groups") }
  let first = groups[0]
  let second = groups[1]
  uu.touch(s, "ref_file")?
  uu.touch(s, "test_file")?
  let setup1 = uu.invoke(s, "chgrp", [f"{first}", "test_file"])?
  uu.succeeds(setup1)

  let setup2 = uu.invoke(s, "chgrp", [f"{second}", "ref_file"])?
  uu.succeeds(setup2)

  let r = uu.invoke(s, "chgrp", ["--from", f"{first}", "--reference=ref_file", "test_file"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert fs.stat(uu.at(s, "test_file"))?.gid == (if unix.id()?.euid == first { fs.stat(uu.at(s, "ref_file"))?.gid } else { first })
}

# origin: uutils test_chgrp::test_numeric_group_formats
test test_uu_chgrp_numeric_group_formats { |ctx|
  let s = uu.scene(ctx)?
  let groups = unix.id()?.supplementary
  if groups.len() < 2 { test.skip("requires 2 supplementary groups") }
  let first = groups[0]
  let second = groups[1]
  uu.touch(s, "test_file")?
  let setup = uu.invoke(s, "chgrp", [f"{first}", "test_file"])?
  uu.succeeds(setup)

  let r = uu.invoke(s, "chgrp", [f"--from=:{first}", f"{second}", "test_file"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert fs.stat(uu.at(s, "test_file"))?.gid == second
  let r2 = uu.invoke(s, "chgrp", [f"--from={second}", f":{first}", "test_file"])?
  uu.fails_with_code(r2, 1)
  uu.stderr_is(r2, f"chgrp: invalid group: ':{first}'\n")
  assert fs.stat(uu.at(s, "test_file"))?.gid == second
}
