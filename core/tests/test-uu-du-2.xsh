##! Transcribed from the MIT-licensed uutils du integration tests.

use support.uu as uu

proc scene(ctx: TestContext) [fs, error] -> Result[uu.Scene, Error] {
  let s = uu.scene(ctx)?
  for name in ["subdir", "subdir/deeper", "subdir/deeper/deeper_dir", "subdir/links"] { uu.mkdir(s, name)? }
  for name in ["words.txt", "empty.txt", "subdir/deeper/words.txt", "subdir/deeper/deeper_dir/deeper_words.txt", "subdir/links/subwords.txt", "subdir/links/subwords2.txt"] {
    uu.fixture(s, "du", name, name)?
  }
  Ok(s)
}

# Capture streams outside the traversed scene so implicit '.' sees only the fixture tree.
proc invoke(s: uu.Scene, args: List[Str], vars: Record = {}) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let captures = test.temp_dir(s.ctx, name: "du-captures")?
  let out = fp"{captures}/stdout"
  let err = fp"{captures}/stderr"
  let plan = uu.command(s, "du", args, vars: vars, stdout: out, stderr: err, timeout: 30s)?
  let status = process.run(plan)?.exit_code()?
  Ok({util: "du", args: args, status: status, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}

proc stdout_lacks(r: uu.Ran, needle: Str) [error] -> Result[Unit, Error] {
  let actual = r.stdout.utf8()?
  assert !(needle in actual), f"unexpected {needle}: {actual}"
  Ok()
}

# origin: uutils test_du::test_du_repeated_inodes
test test_uu_du_du_repeated_inodes { |ctx|
  let s = scene(ctx)?
  let r = invoke(s, ["-s", "--inodes", "--inodes"])?
  uu.succeeds(r)
  uu.stdout_only(r, "11\t.\n")
}

# origin: uutils test_du::test_du_repeated_k
test test_uu_du_du_repeated_k { |ctx|
  let s = scene(ctx)?
  let r = invoke(s, ["-s", "-k", "-k"])?
  uu.succeeds(r)
}

# origin: uutils test_du::test_du_repeated_l
test test_uu_du_du_repeated_l { |ctx|
  let s = scene(ctx)?
  let r = invoke(s, ["-s", "-l", "-l"])?
  uu.succeeds(r)
}

# origin: uutils test_du::test_du_repeated_m
test test_uu_du_du_repeated_m { |ctx|
  let s = scene(ctx)?
  let r = invoke(s, ["-s", "-m", "-m"])?
  uu.succeeds(r)
  uu.stdout_only(r, "1\t.\n")
}

# origin: uutils test_du::test_du_repeated_no_dereference
test test_uu_du_du_repeated_no_dereference { |ctx|
  let s = scene(ctx)?
  let r = invoke(s, ["-s", "-P", "-P"])?
  uu.succeeds(r)
}

# origin: uutils test_du::test_du_repeated_s
test test_uu_du_du_repeated_s { |ctx|
  let s = scene(ctx)?
  let r = invoke(s, ["-s", "-s"])?
  uu.succeeds(r)
}

# origin: uutils test_du::test_du_repeated_separate_dirs
test test_uu_du_du_repeated_separate_dirs { |ctx|
  let s = scene(ctx)?
  let r = invoke(s, ["-s", "-S", "-S"])?
  uu.succeeds(r)
}

# origin: uutils test_du::test_du_repeated_si
test test_uu_du_du_repeated_si { |ctx|
  let s = scene(ctx)?
  let r = invoke(s, ["-s", "--si", "--si"])?
  uu.succeeds(r)
}

# origin: uutils test_du::test_du_repeated_t
test test_uu_du_du_repeated_t { |ctx|
  let s = scene(ctx)?
  let r = invoke(s, ["-s", "-t", "100", "-t", "100"])?
  uu.succeeds(r)
}

# origin: uutils test_du::test_du_repeated_time
test test_uu_du_du_repeated_time { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "date_test")?
  fs.set_times(uu.at(s, "date_test"), atime_ns: 1431648000000000000)?
  fs.set_times(uu.at(s, "date_test"), mtime_ns: 1466035200000000000)?
  let r = invoke(s, ["--time", "date_test", "--time", "date_test"])?
  uu.succeeds(r)
  uu.stdout_only(r, "0\t2016-06-16 00:00\tdate_test\n")
}

# origin: uutils test_du::test_du_repeated_time_style
test test_uu_du_du_repeated_time_style { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "date_test")?
  fs.set_times(uu.at(s, "date_test"), atime_ns: 1431648000000000000)?
  fs.set_times(uu.at(s, "date_test"), mtime_ns: 1466035200000000000)?
  let r = invoke(s, ["--time", "--time-style=full-iso", "--time-style=full-iso", "date_test"])?
  uu.succeeds(r)
  uu.stdout_only(r, "0\t2016-06-16 00:00:00.000000000 +0000\tdate_test\n")
}

# origin: uutils test_du::test_du_repeated_x
test test_uu_du_du_repeated_x { |ctx|
  let s = scene(ctx)?
  let r = invoke(s, ["-s", "-x", "-x"])?
  uu.succeeds(r)
}

# origin: uutils test_du::test_du_safe_traversal_with_symlinks
test test_uu_du_du_safe_traversal_with_symlinks { |ctx|
  let s = scene(ctx)?
  var deep = "symlink_test"
  uu.mkdir(s, deep)?
  for index in range(8) {
    let name = f"{["b" for _ in range(50)].join("")}{index}"
    deep = f"{deep}/{name}"
    uu.mkdir_all(s, deep)?
  }
  uu.write(s, f"{deep}/target.txt", "target content")?
  uu.symlink(s, uu.at(s, f"{deep}/target.txt").display(), "shallow_link.txt")?
  let followed = invoke(s, ["-L", "shallow_link.txt"])?
  uu.succeeds(followed)
  assert !followed.stdout.is_empty()
  let unfollowed = invoke(s, ["shallow_link.txt"])?
  uu.succeeds(unfollowed)
  assert !unfollowed.stdout.is_empty()
}

# origin: uutils test_du::test_du_soft_link
test test_uu_du_du_soft_link { |ctx|
  let s = scene(ctx)?
  uu.mkdir_all(s, "subdir/links")?
  uu.write(s, "subdir/links/subwords.txt", ["hello world\n" for _ in range(100)].join(""))?
  uu.symlink(s, uu.at(s, "subdir/links/subwords.txt").display(), "subdir/links/sublink.txt")?
  let r = invoke(s, ["subdir/links"])?
  uu.succeeds(r)
  # Linux compares allocated usage with the reference tool on the same filesystem.
  var blocks = fs.stat(uu.at(s, "subdir/links"))?.blocks_512
  for name in ["subwords.txt", "subwords2.txt", "sublink.txt"] {
    blocks += fs.stat(uu.at(s, f"subdir/links/{name}"), follow_symlinks: false)?.blocks_512
  }
  uu.stdout_is(r, f"{(blocks + 1) / 2}\tsubdir/links\n")
}

# origin: uutils test_du::test_du_symlink_depth_tracking
test test_uu_du_du_symlink_depth_tracking { |ctx|
  let s = scene(ctx)?
  uu.mkdir_all(s, "chain/dir1/dir2/dir3")?
  uu.write(s, "chain/dir1/dir2/dir3/file.txt", "content")?
  uu.symlink(s, uu.at(s, "chain/dir1/dir2").display(), "shortcut")?
  let r = invoke(s, ["-L", "shortcut"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "shortcut/dir3")
  uu.stdout_contains(r, "shortcut")
}

# origin: uutils test_du::test_du_symlink_fail
test test_uu_du_du_symlink_fail { |ctx|
  let s = scene(ctx)?
  uu.symlink(s, uu.at(s, "non-existing.txt").display(), "target.txt")?
  let r = invoke(s, ["-L", "target.txt"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_du::test_du_symlink_multiple_fail
test test_uu_du_du_symlink_multiple_fail { |ctx|
  let s = scene(ctx)?
  uu.symlink(s, uu.at(s, "non-existing.txt").display(), "target.txt")?
  uu.write(s, "file1", "azeaze")?
  let r = invoke(s, ["-L", "target.txt", "file1"])?
  uu.fails_with_code(r, 1)
  uu.stdout_contains(r, "4\tfile1\n")
}

# origin: uutils test_du::test_du_symlink_self_reference
test test_uu_du_du_symlink_self_reference { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "selfref")?
  uu.symlink(s, uu.at(s, "selfref").display(), "selfref/self")?
  let r = invoke(s, ["-L", "selfref"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "selfref")
  stdout_lacks(r, "selfref/self")?
}

# origin: uutils test_du::test_du_symlinks_multiple_links_in_args
test test_uu_du_du_symlinks_multiple_links_in_args { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "dir1")?
  uu.write(s, "dir1/file", "hello world")?
  uu.symlink(s, uu.at(s, "dir1/file").display(), "dir1/link")?
  let unfollowed = invoke(s, ["dir1/file", "dir1/link"])?
  uu.succeeds(unfollowed)
  uu.stdout_contains(unfollowed, "dir1/file")
  uu.stdout_contains(unfollowed, "dir1/link")
  let followed = invoke(s, ["-L", "dir1/file", "dir1/link"])?
  uu.succeeds(followed)
  uu.stdout_contains(followed, "dir1/file")
  stdout_lacks(followed, "dir1/link")?
}

# origin: uutils test_du::test_du_threshold
test test_uu_du_du_threshold { |ctx|
  let s = scene(ctx)?
  uu.mkdir_all(s, "subdir/links")?
  uu.mkdir_all(s, "subdir/deeper/deeper_dir")?
  uu.write(s, "subdir/links/bigfile.txt", ["x" for _ in range(10000)].join(""))?
  uu.write(s, "subdir/deeper/deeper_dir/smallfile.txt", "small")?
  let above = invoke(s, ["--apparent-size", "--threshold=10K"])?
  uu.succeeds(above)
  uu.stdout_contains(above, "links")
  stdout_lacks(above, "deeper_dir")?
  let below = invoke(s, ["--apparent-size", "--threshold=-10K"])?
  uu.succeeds(below)
  stdout_lacks(below, "links")?
  uu.stdout_contains(below, "deeper_dir")
}

# origin: uutils test_du::test_du_threshold_error_handling
test test_uu_du_du_threshold_error_handling { |ctx|
  let s = scene(ctx)?
  let r = invoke(s, ["--threshold"])?
  uu.fails(r)
  uu.stderr_contains(r, "option '--threshold' requires an argument")
  uu.stderr_contains(r, "Try 'du --help' for more information.")
}

# origin: uutils test_du::test_du_threshold_no_suggested_values
test test_uu_du_du_threshold_no_suggested_values { |ctx|
  let s = scene(ctx)?
  let r = invoke(s, ["--threshold"])?
  uu.fails(r)
  assert !("[possible values: ]" in r.stderr.utf8()?)
}

# origin: uutils test_du::test_du_threshold_with_leading_whitespace
test test_uu_du_du_threshold_with_leading_whitespace { |ctx|
  let s = scene(ctx)?
  let r = invoke(s, ["--threshold=  -1K"])?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_du::test_du_time
test test_uu_du_du_time { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "date_test")?
  fs.set_times(uu.at(s, "date_test"), atime_ns: 1431648000000000000)?
  fs.set_times(uu.at(s, "date_test"), mtime_ns: 1466035200000000000)?
  {
  let r = invoke(s, ["--time", "date_test"])?
  uu.succeeds(r)
  uu.stdout_only(r, "0\t2016-06-16 00:00\tdate_test\n")
  }
  {
  let r = invoke(s, ["--time", "--time-style=long-iso", "date_test"])?
  uu.succeeds(r)
  uu.stdout_only(r, "0\t2016-06-16 00:00\tdate_test\n")
  }
  {
  let r = invoke(s, ["--time", "--time-style=full-iso", "date_test"])?
  uu.succeeds(r)
  uu.stdout_only(r, "0\t2016-06-16 00:00:00.000000000 +0000\tdate_test\n")
  }
  {
  let r = invoke(s, ["--time", "--time-style=iso", "date_test"])?
  uu.succeeds(r)
  uu.stdout_only(r, "0\t2016-06-16\tdate_test\n")
  }
  {
  let r = invoke(s, ["--time", "--time-style=+%Y__%H", "date_test"])?
  uu.succeeds(r)
  uu.stdout_only(r, "0\t2016__00\tdate_test\n")
  }
  {
  let r = invoke(s, ["--time", "--time-style=+%Y_\n_%H", "date_test"])?
  uu.succeeds(r)
  uu.stdout_only(r, "0\t2016_\n_00\tdate_test\n")
  }
  {
  let r = invoke(s, ["--time", "date_test"], vars: {TIME_STYLE: "full-iso"})?
  uu.succeeds(r)
  uu.stdout_only(r, "0\t2016-06-16 00:00:00.000000000 +0000\tdate_test\n")
  }
  {
  let r = invoke(s, ["--time", "date_test"], vars: {TIME_STYLE: "posix-full-iso"})?
  uu.succeeds(r)
  uu.stdout_only(r, "0\t2016-06-16 00:00:00.000000000 +0000\tdate_test\n")
  }
  {
  let r = invoke(s, ["--time", "date_test"], vars: {TIME_STYLE: "+XXX\nYYY"})?
  uu.succeeds(r)
  uu.stdout_only(r, "0\tXXX\tdate_test\n")
  }
  {
  let r = invoke(s, ["--time", "date_test"], vars: {TIME_STYLE: "locale"})?
  uu.succeeds(r)
  uu.stdout_only(r, "0\t2016-06-16 00:00\tdate_test\n")
  }
  {
  let r = invoke(s, ["--time", "--time-style=iso", "date_test"], vars: {TIME_STYLE: "full-iso"})?
  uu.succeeds(r)
  uu.stdout_only(r, "0\t2016-06-16\tdate_test\n")
  }
  for argument in ["--time=atime", "--time=atim", "--time=a"] {
    let r = invoke(s, [argument, "date_test"])?
    uu.succeeds(r)
    uu.stdout_only(r, "0\t2015-05-15 00:00\tdate_test\n")
  }
  let change = invoke(s, ["--time=ctime", "date_test"])?
  uu.succeeds(change)
  assert rx"0\t[0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}\tdate_test".matches(change.stdout.utf8()?)
  if fs.stat(s.root)?.birth_ns != null {
    let birth = invoke(s, ["--time=birth", "date_test"])?
    uu.succeeds(birth)
    assert rx"0\t[0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}\tdate_test".matches(birth.stdout.utf8()?)
  }
}

# origin: uutils test_du::test_du_time_atime
test test_uu_du_du_time_atime { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "f")?
  fs.set_times(uu.at(s, "f"), atime_ns: 1640995200000000000)?
  let r = invoke(s, ["--time=atime", "f"])?
  uu.succeeds(r)
  uu.stdout_only(r, "0\t2022-01-01 00:00\tf\n")
}

# origin: uutils test_du::test_du_time_directory
test test_uu_du_du_time_directory { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "d")?
  uu.touch(s, "d/old")?
  fs.set_times(uu.at(s, "d/old"), mtime_ns: 1577836800000000000)?
  uu.touch(s, "d/new")?
  fs.set_times(uu.at(s, "d/new"), mtime_ns: 1672531200000000000)?
  fs.set_times(uu.at(s, "d"), mtime_ns: 1546300800000000000)?
  {
  let r = invoke(s, ["--time", "d/old"])?
  uu.succeeds(r)
  uu.stdout_only(r, "0\t2020-01-01 00:00\td/old\n")
  }
  {
  let r = invoke(s, ["--time", "d"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "2023-01-01 00:00")
  uu.stdout_contains(r, "\td\n")
  }
  let r = invoke(s, ["--time", "-a", "d"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "2020-01-01 00:00\td/old\n")
  uu.stdout_contains(r, "2023-01-01 00:00\td/new\n")
  uu.stdout_contains(r, "2023-01-01 00:00\td\n")
}

# origin: uutils test_du::test_du_time_directory_nested
test test_uu_du_du_time_directory_nested { |ctx|
  let s = scene(ctx)?
  uu.mkdir_all(s, "d/sub")?
  uu.touch(s, "d/old")?
  fs.set_times(uu.at(s, "d/old"), mtime_ns: 1577836800000000000)?
  uu.touch(s, "d/sub/new")?
  fs.set_times(uu.at(s, "d/sub/new"), mtime_ns: 1672531200000000000)?
  fs.set_times(uu.at(s, "d/sub"), mtime_ns: 1546300800000000000)?
  fs.set_times(uu.at(s, "d"), mtime_ns: 1546300800000000000)?
  let r = invoke(s, ["--time", "-a", "d"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "2020-01-01 00:00\td/old\n")
  uu.stdout_contains(r, "2023-01-01 00:00\td/sub/new\n")
  uu.stdout_contains(r, "2023-01-01 00:00\td/sub\n")
  uu.stdout_contains(r, "2023-01-01 00:00\td\n")
}

# origin: uutils test_du::test_du_time_style_empty
test test_uu_du_du_time_style_empty { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "a")?
  {
  let r = invoke(s, ["--time", "--time-style=", "a"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "du: ambiguous argument '' for 'time style'")
  }
  let r = invoke(s, ["--time", "a"], vars: {TIME_STYLE: "posix-"})?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "du: ambiguous argument '' for 'time style'")
}

# origin: uutils test_du::test_du_time_unaffected_by_exclude
test test_uu_du_du_time_unaffected_by_exclude { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "d")?
  uu.touch(s, "d/keep")?
  uu.touch(s, "d/ignore")?
  fs.set_times(uu.at(s, "d/keep"), mtime_ns: 1577836800000000000)?
  fs.set_times(uu.at(s, "d/ignore"), mtime_ns: 1672531200000000000)?
  fs.set_times(uu.at(s, "d"), mtime_ns: 1546300800000000000)?
  let r = invoke(s, ["--time", "--exclude=ignore", "d"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "2020-01-01 00:00")
  uu.stdout_contains(r, "\td\n")
}

# origin: uutils test_du::test_du_very_deep_directory
test test_uu_du_du_very_deep_directory { |ctx|
  let s = scene(ctx)?
  var current = "x"
  uu.mkdir(s, current)?
  for _ in range(10) {
    current = f"{current}/x"
    uu.mkdir_all(s, current)?
  }
  uu.write(s, f"{current}/file.txt", "deep file")?
  let summarized = invoke(s, ["-s", "x"])?
  uu.succeeds(summarized)
  uu.stdout_contains(summarized, "x")
  let all = invoke(s, ["-a", "x"])?
  uu.succeeds(all)
  uu.stdout_contains(all, "file.txt")
}

# origin: uutils test_du::test_du_with_posixly_correct
test test_uu_du_du_with_posixly_correct { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "a")?
  uu.write(s, "a/file", "some content")?
  let expected = invoke(s, ["a", "--block-size=512"]) ?
  uu.succeeds(expected)
  {
  let r = invoke(s, ["a"], vars: {POSIXLY_CORRECT: "1"})?
  uu.succeeds(r)
  assert r.stdout == expected.stdout
  }
}

# origin: uutils test_du::test_du_zero_env_block_size
test test_uu_du_du_zero_env_block_size { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "a")?
  uu.write(s, "a/file", "some content")?
  let expected = invoke(s, ["a", "--block-size=1024"]) ?
  uu.succeeds(expected)
  {
  let r = invoke(s, ["a"], vars: {DU_BLOCK_SIZE: "0"})?
  uu.succeeds(r)
  assert r.stdout == expected.stdout
  }
}

# origin: uutils test_du::test_du_zero_env_block_size_hierarchy
test test_uu_du_du_zero_env_block_size_hierarchy { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "a")?
  uu.write(s, "a/file", "some content")?
  let expected = invoke(s, ["a", "--block-size=1024"]) ?
  uu.succeeds(expected)
  {
  let r = invoke(s, ["a"], vars: {BLOCK_SIZE: "1", DU_BLOCK_SIZE: "0"})?
  uu.succeeds(r)
  assert r.stdout == expected.stdout
  }
  {
  let r = invoke(s, ["a"], vars: {BLOCK_SIZE: "1", BLOCKSIZE: "1", DU_BLOCK_SIZE: "0"})?
  uu.succeeds(r)
  assert r.stdout == expected.stdout
  }
}

# origin: uutils test_du::test_human_size
test test_uu_du_human_size { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "d")?
  let directory = uu.at(s, "d").display()
  for index in range(1, 1024) { uu.touch(s, f"d/file{index}")? }
  let r = invoke(s, ["--inodes", "-h", directory])?
  uu.succeeds(r)
  uu.stdout_contains(r, f"1.0K\t{directory}")
}

# origin: uutils test_du::test_invalid_arg
test test_uu_du_invalid_arg { |ctx|
  let s = scene(ctx)?
  let r = invoke(s, ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_du::test_invalid_time_style
test test_uu_du_invalid_time_style { |ctx|
  let s = scene(ctx)?
  let r = invoke(s, ["-s", "--time-style=banana"])?
  uu.succeeds(r)
  stdout_lacks(r, "du: invalid argument 'banana' for 'time style'")?
}

# origin: uutils test_du::test_overriding_block_size_arg_with_invalid_value_still_errors
test test_uu_du_overriding_block_size_arg_with_invalid_value_still_errors { |ctx|
  for option in ["-m", "-k", "-b", "-h", "--si"] {
    let s = scene(ctx)?
    let r = invoke(s, ["--block-size=abc", option])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "invalid --block-size argument 'abc'")
  }
}

