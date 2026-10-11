##! Transcribed from the MIT-licensed uutils du integration tests.
use support.uu as uu

# Capture files must be outside the walked scene so their sizes and inodes are not counted.
proc invoke(s: uu.Scene, args: List[Str], stdin: Bytes = b"", vars: Record = {}) [fs, process, env, error] -> Result[uu.Ran] {
  let capture = test.temp_dir(s.ctx, name: "du-capture")?
  let out = fp"{capture}/stdout"
  let err = fp"{capture}/stderr"
  let r = uu.invoke(s, "du", args, stdin: stdin, vars: vars, stdout: out, stderr: err)?
  Ok({util: r.util, args: r.args, status: r.status, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}

proc scene(ctx: TestContext) [fs, error] -> Result[uu.Scene] {
  let s = uu.scene(ctx)?
  uu.mkdir(s, "subdir/deeper/deeper_dir")?
  uu.mkdir(s, "subdir/links")?
  for name in ["empty.txt", "words.txt", "subdir/deeper/words.txt", "subdir/deeper/deeper_dir/deeper_words.txt", "subdir/links/subwords.txt", "subdir/links/subwords2.txt"] {
    uu.fixture(s, "du", name, name)?
  }
  Ok(s)
}

# The reference walks filesystem metadata independently of the applet output.
# GNU counts directory metadata for disk blocks and counts each inode once.
type Expected = {size: Int, seen: Map[Bool], lines: List[Str]}
proc expected(s: uu.Scene, name: Str = ".", inodes: Bool = false, separate: Bool = false, follow: Bool = false, depth: Int = 100, seen: Map[Bool] = {}) [fs, error] -> Result[Expected] {
  let target = uu.at(s, name)
  let meta = fs.stat(target, follow_symlinks: follow)?
  let key = f"{meta.dev}:{meta.ino}"
  if key in seen { return Ok({size: 0, seen: seen, lines: []}) }
  var visited = seen
  visited[key] = true
  var size = if inodes { 1 } else { meta.blocks_512 * 512 }
  var lines: List[Str] = []
  if meta.kind == "dir" {
    for child in fs.children(target, ordered: false)? {
      let found = expected(s, f"{name}/{child.name}", inodes, separate, follow, depth - 1, visited)?
      visited = found.seen
      let child_meta = fs.stat(uu.at(s, f"{name}/{child.name}"), follow_symlinks: follow)?
      if !separate or child_meta.kind != "dir" { size += found.size }
      lines = lines.extend(found.lines)
    }
    if depth >= 0 {
      let count = if inodes { size } else { (size + 1023) / 1024 }
      lines = lines.push(f"{count}\t{name}\n")
    }
  }
  Ok({size: size, seen: visited, lines: lines})
}

# origin: uutils test_du::diagnostics::test_plain_message_when_stderr_is_a_pipe
test test_uu_du_diagnostics_plain_message_when_stderr_is_a_pipe { |ctx|
  let s = scene(ctx)?
  let r1 = invoke(s, ["-B", "1fb"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_is(r1, "du: invalid suffix in -B argument '1fb'\n")
}

# origin: uutils test_du::test_all_summarize
test test_uu_du_all_summarize { |ctx|
  let s = scene(ctx)?
  let r1 = invoke(s, ["-a", "-s"])?
  uu.fails_with_code(r1, 1)
}

# origin: uutils test_du::test_du_bind_mount_simulation
test test_uu_du_du_bind_mount_simulation { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "mount_test/subdir")?
  uu.write(s, "mount_test/file1.txt", "content1")?
  uu.write(s, "mount_test/subdir/file2.txt", "content2")?
  uu.symlink(s, uu.at(s, "../mount_test").display(), "mount_test/subdir/cycle_link")?
  let r1 = invoke(s, ["mount_test"])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "mount_test/subdir")
  uu.stdout_contains(r1, "mount_test")
  assert !("mount_test/subdir/cycle_link" in r1.stdout.utf8()?)
}

# origin: uutils test_du::test_du_complex_exclude_patterns
test test_uu_du_du_complex_exclude_patterns { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "azerty/xcwww/azeaze")?
  uu.mkdir(s, "azerty/xcwww/qzerty")?
  uu.mkdir(s, "azerty/xcwww/amazing")?
  let r1 = invoke(s, ["--exclude=azerty/*/[^q]*", "azerty"])?
  uu.succeeds(r1)
  assert !("amazing" in r1.stdout.utf8()?)
  uu.stdout_contains(r1, "qzerty")
  assert !("azeaze" in r1.stdout.utf8()?)
  uu.stdout_contains(r1, "xcwww")
  let r2 = invoke(s, ["--exclude=azerty/*/[!q]*", "azerty"])?
  uu.succeeds(r2)
  assert !("amazing" in r2.stdout.utf8()?)
  uu.stdout_contains(r2, "qzerty")
  assert !("azeaze" in r2.stdout.utf8()?)
  uu.stdout_contains(r2, "xcwww")
}

# origin: uutils test_du::test_du_exclude_from_nonexistent_file
test test_uu_du_du_exclude_from_nonexistent_file { |ctx|
  let s = scene(ctx)?
  let r1 = invoke(s, ["--exclude-from=nonexistent-file"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "du: nonexistent-file: No such file or directory")
}


# origin: uutils test_du::test_du_exclude_several_components
test test_uu_du_du_exclude_several_components { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "a/b/c")?
  uu.mkdir(s, "a/x/y")?
  uu.mkdir(s, "a/u/y")?
  let r1 = invoke(s, ["--exclude=a/u", "--exclude=a/b", "a"])?
  uu.succeeds(r1)
  assert !("a/u" in r1.stdout.utf8()?)
  assert !("a/b" in r1.stdout.utf8()?)
}

# origin: uutils test_du::test_du_files0_from
test test_uu_du_du_files0_from { |ctx|
  let s = scene(ctx)?
  uu.write(s, "testfile1", "content1")?
  uu.write(s, "testfile2", "content2")?
  uu.mkdir(s, "testdir")?
  uu.write(s, "testdir/testfile3", "content3")?
  uu.write(s, "filelist", "testfile1\x00testfile2\x00testdir\x00")?
  let r1 = invoke(s, ["--files0-from=filelist"])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "testfile1")
  uu.stdout_contains(r1, "testfile2")
  uu.stdout_contains(r1, "testdir")
}

# origin: uutils test_du::test_du_files0_from_duplicate_file_names_with_count_links
test test_uu_du_du_files0_from_duplicate_file_names_with_count_links { |ctx|
  let s = scene(ctx)?
  let file = "testfile"
  uu.touch(s, file)?
  uu.write(s, "filelist", f"{file}\x00{file}\x00")?
  let r1 = invoke(s, ["-l", "--files0-from=filelist"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, f"0\t{file}\n0\t{file}\n")
}

# origin: uutils test_du::test_du_files0_from_ignore_duplicate_file_names
test test_uu_du_du_files0_from_ignore_duplicate_file_names { |ctx|
  let s = scene(ctx)?
  let file = "testfile"
  uu.touch(s, file)?
  uu.write(s, "filelist", f"{file}\x00{file}\x00")?
  let r1 = invoke(s, ["--files0-from=filelist"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, f"0\t{file}\n")
}

# origin: uutils test_du::test_du_files0_from_missing_file_listed_twice
test test_uu_du_du_files0_from_missing_file_listed_twice { |ctx|
  let s = scene(ctx)?
  uu.write(s, "filelist", "missing\x00missing\x00")?
  let r1 = invoke(s, ["--files0-from=filelist"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_only(r1, "du: cannot access 'missing': No such file or directory\ndu: cannot access 'missing': No such file or directory\n")
}

# origin: uutils test_du::test_du_files0_from_stdin
test test_uu_du_du_files0_from_stdin { |ctx|
  let s = scene(ctx)?
  let input = "testfile1\x00testfile2\x00"
  uu.write(s, "testfile1", "content1")?
  uu.write(s, "testfile2", "content2")?
  let r1 = invoke(s, ["--files0-from=-"], stdin: bytes.from_text(input))?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "testfile1")
  uu.stdout_contains(r1, "testfile2")
}

# origin: uutils test_du::test_du_files0_from_stdin_ignore_duplicate_file_names
test test_uu_du_du_files0_from_stdin_ignore_duplicate_file_names { |ctx|
  let s = scene(ctx)?
  let file = "testfile"
  let input = f"{file}\x00{file}"
  uu.touch(s, file)?
  let r1 = invoke(s, ["--files0-from=-"], stdin: bytes.from_text(input))?
  uu.succeeds(r1)
  uu.stdout_is(r1, f"0\t{file}\n")
}

# origin: uutils test_du::test_du_files0_from_stdin_with_invalid_zero_length_file_names
test test_uu_du_du_files0_from_stdin_with_invalid_zero_length_file_names { |ctx|
  let s = scene(ctx)?
  let r1 = invoke(s, ["--files0-from=-"], stdin: bytes.from_text("\x00\x00"))?
  uu.fails_with_code(r1, 1)
  uu.stderr_contains(r1, "-:1: invalid zero-length file name")
  uu.stderr_contains(r1, "-:2: invalid zero-length file name")
}

# origin: uutils test_du::test_du_files0_from_stdin_with_stdin_as_input
test test_uu_du_du_files0_from_stdin_with_stdin_as_input { |ctx|
  let s = scene(ctx)?
  let r1 = invoke(s, ["--files0-from=-"], stdin: bytes.from_text("-"))?
  uu.fails_with_code(r1, 1)
  uu.stderr_is(r1, "du: when reading file names from standard input, no file name of '-' allowed\n")
}

# origin: uutils test_du::test_du_files0_from_with_invalid_zero_length_file_names
test test_uu_du_du_files0_from_with_invalid_zero_length_file_names { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "testfile")?
  uu.write(s, "filelist", "\x00testfile\x00\x00")?
  let r1 = invoke(s, ["--files0-from=filelist"])?
  uu.fails_with_code(r1, 1)
  uu.stdout_contains(r1, "testfile")
  uu.stderr_contains(r1, "filelist:1: invalid zero-length file name")
  uu.stderr_contains(r1, "filelist:3: invalid zero-length file name")
}

# origin: uutils test_du::test_du_h_flag_empty_file
test test_uu_du_du_h_flag_empty_file { |ctx|
  let s = scene(ctx)?
  let r1 = invoke(s, ["-h", "empty.txt"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0\tempty.txt\n")
}

# origin: uutils test_du::test_du_hard_links_multiple_links_in_args
test test_uu_du_du_hard_links_multiple_links_in_args { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "dir1")?
  uu.write(s, "dir1/file", "hello world")?
  uu.hard_link(s, "dir1/file", "dir1/link")?
  let r1 = invoke(s, ["dir1/file", "dir1/link"])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "dir1/file")
  assert !("dir1/link" in r1.stdout.utf8()?)
  let r2 = invoke(s, ["-L", "dir1/file", "dir1/link"])?
  uu.succeeds(r2)
  uu.stdout_contains(r2, "dir1/file")
  assert !("dir1/link" in r2.stdout.utf8()?)
}

# origin: uutils test_du::test_du_inaccessible_directory
test test_uu_du_du_inaccessible_directory { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "d")?
  uu.mkdir(s, "d/no-x")?
  uu.mkdir(s, "d/no-x/y")?
  uu.set_mode(s, "d/no-x", 0o600)?
  defer uu.set_mode(s, "d/no-x", 0o755)?
  let r1 = invoke(s, ["d"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "du: cannot access 'd/no-x/y': Permission denied")
}

# origin: uutils test_du::test_du_invalid_binary_size
test test_uu_du_du_invalid_binary_size { |ctx|
  let s = scene(ctx)?
  let r1 = invoke(s, ["--block-size=0b123", "/tmp"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_only(r1, "du: invalid suffix in --block-size argument '0b123'\n")
  let r2 = invoke(s, ["--threshold=0b123", "/tmp"])?
  uu.fails_with_code(r2, 1)
  uu.stderr_only(r2, "du: invalid suffix in --threshold argument '0b123'\n")
}

# origin: uutils test_du::test_du_invalid_threshold
test test_uu_du_du_invalid_threshold { |ctx|
  let s = scene(ctx)?
  let threshold = "-0"
  let r1 = invoke(s, [f"--threshold={threshold}"])?
  uu.fails(r1)
}

# origin: uutils test_du::test_du_long_symlink_chain
test test_uu_du_du_long_symlink_chain { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "deep/level1/level2/level3/level4/level5")?
  uu.write(s, "deep/level1/level2/level3/level4/level5/file.txt", "content")?
  uu.symlink(s, uu.at(s, "deep/level1").display(), "link1")?
  uu.symlink(s, uu.at(s, "link1/level2").display(), "link2")?
  uu.symlink(s, uu.at(s, "link2/level3").display(), "link3")?
  let r1 = invoke(s, ["-L", "link3"])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "link3")
}

# origin: uutils test_du::test_du_non_existing_files
test test_uu_du_du_non_existing_files { |ctx|
  let s = scene(ctx)?
  let r1 = invoke(s, ["non_existing_a", "non_existing_b"])?
  uu.fails(r1)
  uu.stderr_only(r1, "du: cannot access 'non_existing_a': No such file or directory\ndu: cannot access 'non_existing_b': No such file or directory\n")
}

# origin: uutils test_du::test_du_repeated_0
test test_uu_du_du_repeated_0 { |ctx|
  let s = scene(ctx)?
  let r1 = invoke(s, ["-s", "-0", "-0"])?
  uu.succeeds(r1)
}

# origin: uutils test_du::test_du_repeated_a
test test_uu_du_du_repeated_a { |ctx|
  let s = scene(ctx)?
  let r1 = invoke(s, ["-a", "-a"])?
  uu.succeeds(r1)
}

# origin: uutils test_du::test_du_repeated_apparent_size
test test_uu_du_du_repeated_apparent_size { |ctx|
  let s = scene(ctx)?
  let r1 = invoke(s, ["-s", "-A", "-A"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "6\t.\n")
}

# origin: uutils test_du::test_du_repeated_b
test test_uu_du_du_repeated_b { |ctx|
  let s = scene(ctx)?
  let r1 = invoke(s, ["-s", "-b", "-b"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "5148\t.\n")
}

# origin: uutils test_du::test_du_repeated_block_size
test test_uu_du_du_repeated_block_size { |ctx|
  let s = scene(ctx)?
  let r1 = invoke(s, ["-s", "-B", "100", "-B", "100"])?
  uu.succeeds(r1)
}

# origin: uutils test_du::test_du_repeated_c
test test_uu_du_du_repeated_c { |ctx|
  let s = scene(ctx)?
  let r1 = invoke(s, ["-s", "-c", "-c"])?
  uu.succeeds(r1)
}

# origin: uutils test_du::test_du_repeated_d
test test_uu_du_du_repeated_d { |ctx|
  let s = scene(ctx)?
  let r1 = invoke(s, ["-d", "2", "-d", "2"])?
  uu.succeeds(r1)
}

# origin: uutils test_du::test_du_repeated_dereference
test test_uu_du_du_repeated_dereference { |ctx|
  let s = scene(ctx)?
  let r1 = invoke(s, ["-s", "-L", "-L"])?
  uu.succeeds(r1)
}

# origin: uutils test_du::test_du_repeated_dereference_args
test test_uu_du_du_repeated_dereference_args { |ctx|
  let s = scene(ctx)?
  let r1 = invoke(s, ["-s", "-D", "-D"])?
  uu.succeeds(r1)
}

# origin: uutils test_du::test_du_repeated_files0_from
test test_uu_du_du_repeated_files0_from { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.write(s, "dir/file2", "xyz")?
  uu.write(s, "./somefile", "dir\x00")?
  let r1 = invoke(s, ["-s", "--files0-from", "somefile", "--files0-from", "somefile"])?
  uu.succeeds(r1)
}

# origin: uutils test_du::test_du_repeated_h
test test_uu_du_du_repeated_h { |ctx|
  let s = scene(ctx)?
  let r1 = invoke(s, ["-s", "-h", "-h"])?
  uu.succeeds(r1)
}

# origin: uutils test_du::test_block_override_b_still_has_apparent_size
test test_uu_du_block_override_b_still_has_apparent_size { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "override_args_dir/nested_dir")?
  for file in ["file", "file_2"] {
    uu.touch(s, f"override_args_dir/nested_dir/{file}")?
    uu.truncate(s, f"override_args_dir/nested_dir/{file}", 100000000)?
  }
  for pair in [
    {over: ["-b", "-m"], final: ["-m", "--apparent-size"]},
    {over: ["-b", "-k"], final: ["-k", "--apparent-size"]},
    {over: ["-b", "--si"], final: ["--si", "--apparent-size"]},
    {over: ["-b", "-h"], final: ["-h", "--apparent-size"]},
    {over: ["-b", "--block-size=128"], final: ["--block-size=128", "--apparent-size"]},
  ] {
    let a = invoke(s, ["override_args_dir"].extend(pair.over))?
    let b = invoke(s, ["override_args_dir"].extend(pair.final))?
    uu.succeeds(a)
    uu.succeeds(b)
    assert a.stdout == b.stdout
  }
}

# origin: uutils test_du::test_block_size_args_override
test test_uu_du_block_size_args_override { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "override_args_dir/nested_dir")?
  uu.mkdir(s, "override_args_dir/nested_dir_2")?
  for file in [{name: "nested_dir/file", size: 100000000}, {name: "nested_dir/file_2", size: 100000000}, {name: "nested_dir_2/file_3", size: 100000}, {name: "nested_dir_2/file_4", size: 100}] {
    uu.touch(s, f"override_args_dir/{file.name}")?
    uu.truncate(s, f"override_args_dir/{file.name}", file.size)?
  }
  for pair in [
    {over: ["-sk", "-m"], final: ["-sm"]},
    {over: ["-sk", "-b"], final: ["-sb"]},
    {over: ["-sm", "-k"], final: ["-sk"]},
    {over: ["-sk", "--si"], final: ["-s", "--si"]},
    {over: ["-sk", "-h"], final: ["-s", "-h"]},
    {over: ["-sm", "--block-size=128"], final: ["-s", "--block-size=128"]},
    {over: ["--block-size=128", "-b"], final: ["-b"]},
    {over: ["--si", "-b"], final: ["-b"]},
    {over: ["-h", "-b"], final: ["-b"]},
  ] {
    let a = invoke(s, ["override_args_dir"].extend(pair.over))?
    let b = invoke(s, ["override_args_dir"].extend(pair.final))?
    uu.succeeds(a)
    uu.succeeds(b)
    assert a.stdout == b.stdout
  }
}

# origin: uutils test_du::test_du_apparent_size
test test_uu_du_du_apparent_size { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "a/b")?
  uu.write(s, "a/b/file1", "foo")?
  uu.write(s, "a/b/file2", "foobar")?
  let r = invoke(s, ["--apparent-size", "--all", "a"])?
  uu.succeeds(r)
  assert "1\ta/b/file2" in r.stdout.utf8()?.lines()
  assert "1\ta/b/file1" in r.stdout.utf8()?.lines()
  assert "1\ta/b" in r.stdout.utf8()?.lines()
  assert "1\ta" in r.stdout.utf8()?.lines()
}

# origin: uutils test_du::test_du_basics
test test_uu_du_du_basics { |ctx|
  let s = scene(ctx)?
  let r = invoke(s, [])?
  uu.succeeds(r)
  uu.stdout_is(r, expected(s, ".")?.lines.join(""))
}

# origin: uutils test_du::test_du_basics_subdir
test test_uu_du_du_basics_subdir { |ctx|
  let s = scene(ctx)?
  let r = invoke(s, ["subdir/deeper"])?
  uu.succeeds(r)
  uu.stdout_is(r, expected(s, "subdir/deeper")?.lines.join(""))
}

# origin: uutils test_du::test_du_blocksize_zero_do_not_panic
test test_uu_du_du_blocksize_zero_do_not_panic { |ctx|
  let s = scene(ctx)?
  uu.write(s, "foo", "some content")?
  for size in ["0", "00", "000", "0x0", "0b0"] {
    let r = invoke(s, [f"-B{size}", "foo"])?
    uu.fails(r)
    uu.stderr_only(r, f"du: invalid -B argument '{size}'\n")
  }
}

# origin: uutils test_du::test_du_bytes
test test_uu_du_du_bytes { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "a/b")?
  uu.write(s, "a/b/file1", "foo")?
  uu.write(s, "a/b/file2", "foobar")?
  let r = invoke(s, ["--bytes", "--all", "a"])?
  uu.succeeds(r)
  assert "6\ta/b/file2" in r.stdout.utf8()?.lines()
  assert "3\ta/b/file1" in r.stdout.utf8()?.lines()
  assert "9\ta/b" in r.stdout.utf8()?.lines()
  assert "9\ta" in r.stdout.utf8()?.lines()
}

# origin: uutils test_du::test_du_count_links_hardlinks_separately
test test_uu_du_du_count_links_hardlinks_separately { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.touch(s, "dir/file")?
  uu.hard_link(s, "dir/file", "dir/hard_link")?
  let without = invoke(s, ["-b", "dir"])?
  uu.succeeds(without)
  let size_without = without.stdout.utf8()?.split("\t")[0].parse_int()?
  for arg in ["-l", "--count-links"] {
    let counted = invoke(s, ["-b", arg, "dir"])?
    uu.succeeds(counted)
    assert counted.stdout.utf8()?.split("\t")[0].parse_int()? >= size_without
  }
}

# origin: uutils test_du::test_du_d_flag
test test_uu_du_du_d_flag { |ctx|
  let s = scene(ctx)?
  let r = invoke(s, ["-d1"])?
  uu.succeeds(r)
  uu.stdout_is(r, expected(s, ".", depth: 1)?.lines.join(""))
}

# origin: uutils test_du::test_du_deduplicated_input_args
test test_uu_du_du_deduplicated_input_args { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "d/d")?
  uu.touch(s, "d/f")?
  uu.hard_link(s, "d/f", "d/h")?
  let r = invoke(s, ["--inodes", "d", "d", "d"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout.utf8()?.lines() == ["1\td/d", "3\td"]
}

# origin: uutils test_du::test_du_dereference
test test_uu_du_du_dereference { |ctx|
  let s = scene(ctx)?
  uu.symlink(s, uu.at(s, "subdir/deeper/deeper_dir").display(), "subdir/links/deeper_dir")?
  let r = invoke(s, ["-L", "subdir/links"])?
  uu.succeeds(r)
  uu.stdout_is(r, expected(s, "subdir/links", follow: true)?.lines.join(""))
}

# origin: uutils test_du::test_du_env_block_size_hierarchy
test test_uu_du_du_env_block_size_hierarchy { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "a")?
  uu.write(s, "a/file", "some content")?
  let baseline = invoke(s, ["a", "--block-size=1"]) ?
  uu.succeeds(baseline)
  let r0 = invoke(s, ["a"], vars: {BLOCK_SIZE: "0", DU_BLOCK_SIZE: "1"})?
  uu.succeeds(r0)
  assert baseline.stdout == r0.stdout
  let r1 = invoke(s, ["a"], vars: {BLOCK_SIZE: "1", BLOCKSIZE: "0"})?
  uu.succeeds(r1)
  assert baseline.stdout == r1.stdout
}

# origin: uutils test_du::test_du_exclude
test test_uu_du_du_exclude { |ctx|
  let s = scene(ctx)?
  uu.symlink(s, uu.at(s, "subdir/deeper/deeper_dir").display(), "subdir/links/deeper_dir")?
  uu.mkdir(s, "subdir/links")?
  let first = invoke(s, ["--exclude=subdir", "subdir/deeper/deeper_dir"])?
  uu.succeeds(first)
  uu.stdout_contains(first, "subdir/deeper/deeper_dir")
  let excluded = invoke(s, ["--exclude=subdir", "subdir"])?
  uu.succeeds(excluded)
  uu.no_output(excluded)
  let verbose = invoke(s, ["--exclude=subdir", "--verbose", "subdir"])?
  uu.fails_with_code(verbose, 1)
  uu.no_stdout(verbose)
  uu.stderr_is(verbose, "du: unrecognized option '--verbose'\nTry 'du --help' for more information.\n")
}

# origin: uutils test_du::test_du_exclude_2
test test_uu_du_du_exclude_2 { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "azerty/xcwww/azeaze")?
  let r = invoke(s, ["azerty"])?
  uu.succeeds(r)
  assert rx"(.*)azerty/xcwww/azeaze(.*)azerty/xcwww(.*)azerty".matches(r.stdout.utf8()?.replace("\n", with: "").trim())
  for pattern in ["azeaze", "azeaz", "azea?", "azea{z,b}", "azea*", "azeaz?"] {
    let result = invoke(s, [f"--exclude={pattern}", "azerty"])?
    uu.succeeds(result)
    let excluded = pattern in ["azeaze", "azea*", "azeaz?"]
    assert "azerty/xcwww/azeaze" in result.stdout.utf8()? == !excluded
  }
}

# origin: uutils test_du::test_du_exclude_mix
test test_uu_du_du_exclude_mix { |ctx|
  let s = scene(ctx)?
  uu.write(s, "file-ignore1", "azeaze")?
  uu.write(s, "file-ignore2", "amaz?ng")?
  uu.mkdir(s, "azerty/xcwww/azeaze")?
  uu.mkdir(s, "azerty/xcwww/qzerty")?
  uu.mkdir(s, "azerty/xcwww/amazing")?
  let first = invoke(s, ["azerty"])?
  uu.succeeds(first)
  uu.stdout_contains(first, "azerty/xcwww/azeaze")
  let excluded = invoke(s, ["--exclude=azeaze", "azerty"])?
  uu.succeeds(excluded)
  assert !("azerty/xcwww/azeaze" in excluded.stdout.utf8()?)
  let one = invoke(s, ["--exclude=qzerty", "azerty"])?
  uu.succeeds(one)
  assert !("qzerty" in one.stdout.utf8()?)
  uu.stdout_contains(one, "azerty")
  uu.stdout_contains(one, "xcwww")
  let file = invoke(s, ["--exclude-from=file-ignore1", "azerty"])?
  uu.succeeds(file)
  assert !("azeaze" in file.stdout.utf8()?)
  uu.stdout_contains(file, "qzerty")
  uu.stdout_contains(file, "xcwww")
  let mix = invoke(s, ["--exclude=qzerty", "--exclude-from=file-ignore1", "--exclude-from=file-ignore2", "azerty"])?
  uu.succeeds(mix)
  for name in ["amazing", "qzerty", "azeaze"] { assert !(name in mix.stdout.utf8()?) }
  uu.stdout_contains(mix, "xcwww")
}

# origin: uutils test_du::test_du_files0_from_combined
test test_uu_du_du_files0_from_combined { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "dir")?
  let r = invoke(s, ["--files0-from=-", "foo"])?
  uu.fails(r)
  uu.stderr_contains(r, "file operands cannot be combined with --files0-from")
}

# origin: uutils test_du::test_du_files0_from_dir
test test_uu_du_du_files0_from_dir { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "dir")?
  let r = invoke(s, ["--files0-from=dir"])?
  uu.fails(r)
  uu.stderr_is(r, "du: dir: read error: Is a directory\n")
}

# origin: uutils test_du::test_du_h_locale_decimal_separator
test test_uu_du_du_h_locale_decimal_separator { |ctx|
  let s = scene(ctx)?
  for case in [{locale: "fr_FR.UTF-8", text: "8,4K"}, {locale: "C", text: "8.4K"}] {
    let each = scene(ctx)?
    uu.touch(each, "test.txt")?
    uu.truncate(each, "test.txt", 8500)?
    let target = uu.at(each, "test.txt").display()
    let r = invoke(each, ["-h", "--apparent-size", target], vars: {LC_ALL: case.locale})?
    uu.succeeds(r)
    uu.stdout_only(r, f"{case.text}\t{target}\n")
  }
}

# origin: uutils test_du::test_du_h_precision
test test_uu_du_du_h_precision { |ctx|
  let s = scene(ctx)?
  for case in [{size: 133456345, text: "128M"}, {size: 12582912, text: "12M"}, {size: 8500, text: "8.4K"}] {
    let each = scene(ctx)?
    uu.touch(each, "test.txt")?
    uu.truncate(each, "test.txt", case.size)?
    let target = uu.at(each, "test.txt").display()
    let r = invoke(each, ["-h", "--apparent-size", target])?
    uu.succeeds(r)
    uu.stdout_only(r, f"{case.text}\t{target}\n")
  }
}

# origin: uutils test_du::test_du_hard_link
test test_uu_du_du_hard_link { |ctx|
  let s = scene(ctx)?
  uu.hard_link(s, "subdir/links/subwords.txt", "subdir/links/sublink.txt")?
  let r = invoke(s, ["subdir/links"])?
  uu.succeeds(r)
  uu.stdout_is(r, expected(s, "subdir/links")?.lines.join(""))
}

# origin: uutils test_du::test_du_hard_links_multiple_dirs_in_args
test test_uu_du_du_hard_links_multiple_dirs_in_args { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "dir1")?
  uu.mkdir(s, "dir2")?
  uu.write(s, "dir1/file", "hello world")?
  uu.hard_link(s, "dir1/file", "dir2/link")?
  let r = invoke(s, ["dir1", "dir2"])?
  uu.succeeds(r)
  let lines = r.stdout.utf8()?.lines()
  assert lines[0].split("\t")[0].parse_int()? > lines[1].split("\t")[0].parse_int()?
}

# origin: uutils test_du::test_du_inodes
test test_uu_du_du_inodes { |ctx|
  let s = scene(ctx)?
  let sum = invoke(s, ["--summarize", "--inodes"])?
  uu.succeeds(sum)
  uu.stdout_only(sum, "11\t.\n")
  let separated = invoke(s, ["--separate-dirs", "--inodes"])?
  uu.succeeds(separated)
  uu.stdout_contains(separated, "3\t./subdir/links\n")
  uu.stdout_contains(separated, "3\t.\n")
  uu.stdout_is(separated, expected(s, inodes: true, separate: true)?.lines.join(""))
}

# origin: uutils test_du::test_du_inodes_basic
test test_uu_du_du_inodes_basic { |ctx|
  let s = scene(ctx)?
  let r = invoke(s, ["--inodes"])?
  uu.succeeds(r)
  uu.stdout_is(r, expected(s, ".", inodes: true)?.lines.join(""))
}

# origin: uutils test_du::test_du_inodes_blocksize_ineffective
test test_uu_du_du_inodes_blocksize_ineffective { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "test.txt")?
  for method in ["-B3", "--block-size=3"] {
    let r = invoke(s, [method, "--inodes", "test.txt"])?
    uu.succeeds(r)
    uu.stdout_only(r, "1\ttest.txt\n")
  }
  for method in ["--apparent-size", "-b"] {
    let r = invoke(s, [method, "--inodes", "test.txt"])?
    uu.succeeds(r)
    uu.stdout_is(r, "1\ttest.txt\n")
    uu.stderr_is(r, "du: warning: options --apparent-size and -b are ineffective with --inodes\n")
  }
}

# origin: uutils test_du::test_du_inodes_total_text
test test_uu_du_du_inodes_total_text { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "d/d")?
  let r = invoke(s, ["--inodes", "-c", "d"])?
  uu.succeeds(r)
  let lines = r.stdout.utf8()?.lines()
  assert lines.len() == 3
  assert "total" in lines[2]
  let parts = lines[2].split("\t")
  assert parts.len() == 2
  assert parts[0].parse_int()? >= 0
}

# origin: uutils test_du::test_du_inodes_with_count_links
test test_uu_du_du_inodes_with_count_links { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.touch(s, "dir/file")?
  uu.hard_link(s, "dir/file", "dir/hard_link_a")?
  uu.hard_link(s, "dir/file", "dir/hard_link_b")?
  let once = invoke(s, ["--inodes", "dir"])?
  uu.succeeds(once)
  uu.stdout_is(once, "2\tdir\n")
  for arg in ["-l", "--count-links"] {
    let r = invoke(s, ["--inodes", arg, "dir"])?
    uu.succeeds(r)
    uu.stdout_is(r, "4\tdir\n")
  }
}

# origin: uutils test_du::test_du_inodes_with_count_links_all
test test_uu_du_du_inodes_with_count_links_all { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "d/d")?
  uu.touch(s, "d/f")?
  uu.hard_link(s, "d/f", "d/h")?
  let r = invoke(s, ["--inodes", "-al", "d"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  let lines = r.stdout.utf8()?.lines()
  assert lines.len() == 4
  for line in ["1\td/d", "1\td/f", "1\td/h", "4\td"] { assert line in lines }
}

# origin: uutils test_du::test_du_invalid_env_block_size_stops_lookup
test test_uu_du_du_invalid_env_block_size_stops_lookup { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "a")?
  uu.write(s, "a/file", "some content")?
  let baseline = invoke(s, ["a", "--block-size=1024"]) ?
  uu.succeeds(baseline)
  let r0 = invoke(s, ["a"], vars: {DU_BLOCK_SIZE: "invalid", BLOCK_SIZE: "1"})?
  uu.succeeds(r0)
  assert baseline.stdout == r0.stdout
  let r1 = invoke(s, ["a"], vars: {DU_BLOCK_SIZE: "", BLOCK_SIZE: "1"})?
  uu.succeeds(r1)
  assert baseline.stdout == r1.stdout
}

# origin: uutils test_du::test_du_invalid_size
test test_uu_du_du_invalid_size { |ctx|
  let s = scene(ctx)?
  for option in ["block-size", "threshold"] {
    let suffix = invoke(s, [f"--{option}=1fb4t", "/tmp"])?
    uu.fails_with_code(suffix, 1)
    uu.stderr_only(suffix, f"du: invalid suffix in --{option} argument '1fb4t'\n")
    let invalid = invoke(s, [f"--{option}=x", "/tmp"])?
    uu.fails_with_code(invalid, 1)
    uu.stderr_only(invalid, f"du: invalid --{option} argument 'x'\n")
    let large = invoke(s, [f"--{option}=1Y", "/tmp"])?
    uu.fails_with_code(large, 1)
    uu.stderr_only(large, f"du: --{option} argument '1Y' too large\n")
  }
}

# origin: uutils test_du::test_du_long_path_safe_traversal
test test_uu_du_du_long_path_safe_traversal { |ctx|
  let s = scene(ctx)?
  var deep = "long_path_test"
  uu.mkdir(s, deep)?
  let name = ["a" for _ in range(100)].join("")
  for i in range(15) {
    deep = f"{deep}/{name}{i}"
    uu.mkdir(s, deep)?
  }
  uu.write(s, f"{deep}/test.txt", "test content")?
  let summary = invoke(s, ["-s", "long_path_test"])?
  uu.succeeds(summary)
  uu.stdout_contains(summary, "long_path_test")
  let r = invoke(s, ["long_path_test"])?
  uu.succeeds(r)
  assert r.stdout.utf8()?.trim().lines().len() >= 15
}

# origin: uutils test_du::test_du_negative_max_depth_is_rejected
test test_uu_du_du_negative_max_depth_is_rejected { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "deep/deeper")?
  for depth in ["-7", "-1"] {
    let r = invoke(s, ["-d", depth, "deep"])?
    uu.fails_with_code(r, 1)
    uu.no_stdout(r)
    uu.stderr_contains(r, f"du: invalid maximum depth '{depth}'")
  }
  let r = invoke(s, ["--max-depth=-3", "deep"])?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
  uu.stderr_contains(r, "du: invalid maximum depth '-3'")
}

# origin: uutils test_du::test_du_no_deduplicated_input_args
test test_uu_du_du_no_deduplicated_input_args { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "d")?
  uu.touch(s, "d/d")?
  let r = invoke(s, ["--inodes", "-l", "d", "d", "d"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout.utf8()?.lines() == ["2\td", "2\td", "2\td"]
}

# origin: uutils test_du::test_du_no_dereference
test test_uu_du_du_no_dereference { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "a_dir")?
  uu.symlink(s, uu.at(s, "a_dir").display(), "symlink")?
  for arg in ["-P", "--no-dereference"] {
    let first = invoke(s, [arg])?
    uu.succeeds(first)
    uu.stdout_contains(first, "a_dir")
    assert !("symlink" in first.stdout.utf8()?)
    let no = invoke(s, ["--dereference", arg])?
    uu.succeeds(no)
    uu.stdout_contains(no, "a_dir")
    assert !("symlink" in no.stdout.utf8()?)
    let follow = invoke(s, [arg, "--dereference"])?
    uu.succeeds(follow)
    uu.stdout_is(follow, expected(s, follow: true)?.lines.join(""))
  }
}

# origin: uutils test_du::test_du_no_exec_permission
test test_uu_du_du_no_exec_permission { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "d/no-x/y")?
  uu.set_mode(s, "d/no-x", 0o600)?
  defer uu.set_mode(s, "d/no-x", 0o755)?
  let r = invoke(s, ["d/no-x"])?
  uu.fails(r)
  uu.stderr_contains(r, "du: cannot access 'd/no-x/y': Permission denied")
}

# origin: uutils test_du::test_du_no_permission
test test_uu_du_du_no_permission { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "subdir/links")?
  uu.set_mode(s, "subdir/links", 0o311)?
  defer uu.set_mode(s, "subdir/links", 0o755)?
  let r = invoke(s, ["subdir/links"])?
  uu.fails(r)
  uu.stderr_contains(r, "du: cannot read directory 'subdir/links': Permission denied")
  let directory_bytes = fs.stat(uu.at(s, "subdir/links"))?.blocks_512 * 512
  uu.stdout_is(r, f"{(directory_bytes + 1023) / 1024}\tsubdir/links\n")
}

# origin: uutils test_du::test_du_one_file_system
test test_uu_du_du_one_file_system { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "subdir/deeper/deeper_dir")?
  uu.write(s, "subdir/deeper/deeper_dir/deeper_words.txt", "hello world")?
  uu.write(s, "subdir/deeper/words.txt", "world")?
  let r = invoke(s, ["-x", "subdir/deeper"])?
  uu.succeeds(r)
  uu.stdout_is(r, expected(s, "subdir/deeper")?.lines.join(""))
}

# origin: uutils test_du::test_du_posixly_correct_default
test test_uu_du_du_posixly_correct_default { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "a")?
  uu.write(s, "a/file", "some content")?
  let baseline = invoke(s, ["a", "--block-size=512"]) ?
  uu.succeeds(baseline)
  let r0 = invoke(s, ["a"], vars: {POSIXLY_CORRECT: "1"})?
  uu.succeeds(r0)
  assert baseline.stdout == r0.stdout
}

# origin: uutils test_du::test_du_binary_block_size
test test_uu_du_du_binary_block_size { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "a")?
  uu.touch(s, "a/file")?
  uu.truncate(s, "a/file", 100000)?
  for sizes in [{binary: "0b1", decimal: "1"}, {binary: "0b10100", decimal: "20"}, {binary: "0b1000000000", decimal: "512"}, {binary: "0b10K", decimal: "2K"}] {
    let decimal = invoke(s, ["a", f"--block-size={sizes.decimal}"])?
    let binary = invoke(s, ["a", f"--block-size={sizes.binary}"])?
    uu.succeeds(decimal)
    uu.succeeds(binary)
    assert decimal.stdout == binary.stdout
  }
}

# origin: uutils test_du::test_du_binary_edge_cases
test test_uu_du_du_binary_edge_cases { |ctx|
  let s = scene(ctx)?
  uu.write(s, "foo", "test")?
  let lower = invoke(s, ["-B0b", "foo"])?
  uu.fails(lower)
  uu.stderr_only(lower, "du: invalid -B argument '0b'\n")
  let upper = invoke(s, ["-B0B", "foo"])?
  uu.fails(upper)
  uu.stderr_only(upper, "du: invalid -B argument '0B'\n")
  let overflow = invoke(s, ["--block-size=0b1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111", "foo"])?
  uu.fails_with_code(overflow, 1)
  uu.stderr_contains(overflow, "too large")
}

# origin: uutils test_du::test_du_binary_env_block_size
test test_uu_du_du_binary_env_block_size { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "a")?
  uu.touch(s, "a/file")?
  uu.truncate(s, "a/file", 100000)?
  let baseline = invoke(s, ["a", "--block-size=1024"])?
  let binary = invoke(s, ["a"], vars: {DU_BLOCK_SIZE: "0b10000000000"})?
  uu.succeeds(baseline)
  uu.succeeds(binary)
  assert baseline.stdout == binary.stdout
}

# origin: uutils test_du::test_du_binary_threshold
test test_uu_du_du_binary_threshold { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "subdir/links")?
  uu.mkdir(s, "subdir/deeper/deeper_dir")?
  uu.write(s, "subdir/links/bigfile.txt", ["x" for _ in range(10000)].join(""))?
  uu.write(s, "subdir/deeper/deeper_dir/smallfile.txt", "small")?
  let r = invoke(s, ["--apparent-size", "--threshold=0b10011100010000"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "links")
  assert !("deeper_dir" in r.stdout.utf8()?)
}

# origin: uutils test_du::test_du_exclude_invalid_syntax
test test_uu_du_du_exclude_invalid_syntax { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "azerty/xcwww/azeaze")?
  let r = invoke(s, ["--exclude=a[ze", "azerty"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is(r, expected(s, "azerty")?.lines.join(""))
}
