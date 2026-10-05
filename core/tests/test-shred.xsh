type Outcome = {status: Int, stdout: Str, stderr: Str}

proc run_applet(ctx: TestContext, root: Path, args: List[Str], tempdir: Str = "") [fs, process, error] -> Result[Outcome] {
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/shred.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), root, {TMPDIR: tempdir, LC_ALL: "C"}, b"", out, err)
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
