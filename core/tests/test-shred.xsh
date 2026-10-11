type Outcome = {status: Int, stdout: Str, stderr: Str}

proc run_applet(ctx: TestContext, root: Path, args: List[Str], tempdir: Str = "", diagnostics: Str = "") [fs, process, error] -> Result[Outcome] {
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/shred.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), root, {TMPDIR: tempdir, LC_ALL: "C", UUTILS_DIAG: diagnostics}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

proc run_applet_paths(ctx: TestContext, root: Path, args: List[Union[Str, Path]]) [fs, process, error] -> Result[Outcome] {
  let out = fp"{root}/stdout-bytes"
  let err = fp"{root}/stderr-bytes"
  let script = fp"{ctx.core_dir}/shred.xsh"
  let words: List[Union[Str, Path]] = collect { yield ctx.xsh_bin; yield script; yield "--"; for arg in args { yield arg } }
  let plan = process.command_argv(ctx.xsh_bin, words, root, {LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

test test_shred_zero_preserves_hardlinks_and_exact_length { |ctx|
  let root = test.temp_dir(ctx, name: "shred-zero")?
  let file = fp"{root}/file"
  file.write("secret")
  fs.link(file, fp"{root}/link")
  let before = fs.stat(file)?
  let result = run_applet(ctx, root, ["-n", "0", "-z", "-x", "file"])?
  assert result.status == 0, result.stderr
  assert file.read_bytes()? == b"\0\0\0\0\0\0"
  assert fp"{root}/link".read_bytes()? == file.read_bytes()?
  assert fs.stat(file)?.ino == before.ino
}

test test_shred_source_and_remove { |ctx|
  let root = test.temp_dir(ctx, name: "shred-source")?
  fp"{root}/source".write("0123456789")
  fp"{root}/file".write("abcde")
  let result = run_applet(ctx, root, ["-x", "-n", "1", "--random-source=source", "file"])?
  assert result.status == 0, result.stderr
  assert fp"{root}/file".read_text()? == "01234"
  assert run_applet(ctx, root, ["-n", "0", "--remove=unlink", "file"])?.status == 0
  assert ! fp"{root}/file".exists()?
}


test test_shred_pattern_passes_and_rename_collisions { |ctx|
  let root = test.temp_dir(ctx, name: "shred-patterns")?
  fp"{root}/test".write("secret")
  fp"{root}/000".write("keep")
  let result = run_applet(ctx, root, ["-n", "25", "-x", "-v", "-u", "test"])?
  assert result.status == 0, result.stderr
  for label in ["000000", "ffffff", "249249", "db6db6", "eeeeee"] { assert label in result.stderr }
  assert "renamed to 0000" in result.stderr
  assert "renamed to 001" in result.stderr
  assert "renamed to 00" in result.stderr
  assert ! fp"{root}/test".exists()?
  assert fp"{root}/000".read_text()? == "keep"
}

test test_shred_force_changes_only_permission_bits { |ctx|
  let root = test.temp_dir(ctx, name: "shred-force")?
  let file = fp"{root}/file"
  file.write("secret")
  file.chmod(0o400)
  let result = run_applet(ctx, root, ["-u", "-f", "file"])?
  assert result.status == 0, result.stderr
  assert ! file.exists()?
}

test test_shred_known_random_source_keeps_the_twenty_pass_order { |ctx|
  let root = test.temp_dir(ctx, name: "shred-twenty-passes")?
  fp"{root}/Us".write(bytes.from_ints([85 for _ in range(102400)])?)
  fp"{root}/file".write("1")
  let result = run_applet(ctx, root, ["-v", "-u", "-n20", "-s4096", "--random-source=Us", "file"])?
  assert result.status == 0, result.stderr
  let labels = [
    "pass 1/20 (random)", "pass 2/20 (ffffff)", "pass 3/20 (924924)", "pass 4/20 (888888)",
    "pass 5/20 (db6db6)", "pass 6/20 (777777)", "pass 7/20 (492492)", "pass 8/20 (bbbbbb)",
    "pass 9/20 (555555)", "pass 10/20 (aaaaaa)", "pass 11/20 (random)", "pass 12/20 (6db6db)",
    "pass 13/20 (249249)", "pass 14/20 (999999)", "pass 15/20 (111111)", "pass 16/20 (000000)",
    "pass 17/20 (b6db6d)", "pass 18/20 (eeeeee)", "pass 19/20 (333333)", "pass 20/20 (random)",
  ]
  for label in labels {
    assert label in result.stderr, result.stderr
  }
}

test test_shred_non_utf8_path { |ctx|
  let root = test.temp_dir(ctx, name: "shred-path-bytes")?
  let target = Path.parse_bytes(bytes.concat([root.bytes(), b"/file_\xff\xfe"]))?
  target.write("secret")
  let args: List[Union[Str, Path]] = ["-n", "0", "-z", "-x", target]
  let result = run_applet_paths(ctx, root, args)?
  assert result.status == 0, result.stderr
  assert target.read_bytes()? == b"\0\0\0\0\0\0"
}

test test_shred_invalid_sizes_use_plain_diagnostics { |ctx|
  let root = test.temp_dir(ctx, name: "shred-size-diagnostic")?
  fp"{root}/wipe_me".write("keep")
  for size in ["4vv", "vv"] {
    let result = run_applet(ctx, root, ["-s", size, "wipe_me"], diagnostics: "always")?
    assert result.status == 1
    assert result.stderr == f"shred: invalid file size: '{size}'\n", result.stderr
  }
  assert fp"{root}/wipe_me".read_text()? == "keep"
}

test test_shred_random_source_wipes_short_file_before_padding { |ctx|
  let root = test.temp_dir(ctx, name: "shred-short-source")?
  let source = bytes.from_ints([i % 251 for i in range(16384)])?
  fp"{root}/source".write(source)
  let target = fp"{root}/file"
  target.write("a")
  let result = run_applet(ctx, root, ["-vn3", "--random-source=source", "file"])?
  assert result.status == 0, result.stderr
  let block = fs.stat(target)?.blksize
  assert target.read_bytes()? == source[block * 2 + 3..block * 3 + 3]
  assert result.stderr == "shred: file: pass 1/3 (random)...\nshred: file: pass 2/3 (random)...\nshred: file: pass 3/3 (random)...\n"
}

test test_shred_remove_reports_unlink_permission_failure { |ctx|
  let root = test.temp_dir(ctx, name: "shred-remove-permission")?
  let directory = fp"{root}/dir"
  directory.mkdir()
  fp"{directory}/file".write("")
  directory.chmod(0o555)
  defer directory.chmod(0o755)
  let result = run_applet(ctx, root, ["-uv", "dir/file"])?
  assert result.status == 1
  assert result.stderr == "shred: dir/file: removing\nshred: dir/file: failed to remove: Permission denied\n", result.stderr
}
