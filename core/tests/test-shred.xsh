type Ran = {status: Int, stdout: Str, stderr: Str}

proc shred_run(ctx: TestContext, root: Path, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/shred.xsh"
  let argv = [ctx.xsh_bin.display(), script.display(), @args]
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_shred_overwrites_and_reports_passes { |ctx|
  let root = test.temp_dir(ctx, name: "shred")?
  let file = fp"{root}/file"
  file.write("secret contents")
  let result = shred_run(ctx, root, ["-v", "-n2", "--exact", "file"])?
  assert result.status == 0, result.stderr
  assert "pass 1/2" in result.stderr
  assert file.read_bytes()?.len() == 15
  assert file.read_bytes()? != bytes.from_text("secret contents")
}

test test_shred_random_source_and_remove { |ctx|
  let root = test.temp_dir(ctx, name: "shred-source")?
  let source = fp"{root}/random"
  let file = fp"{root}/target"
  var data: List[Int] = []
  for index in range(12288) { data += [index % 256] }
  source.write(bytes.from_ints(data)?)
  file.write("x")
  let result = shred_run(ctx, root, ["-n3", "-s4096", "--random-source=random", "-u", "target"])?
  assert result.status == 0, result.stderr
  assert ! file.exists()?
}

test test_shred_rejects_ambiguous_remove_mode { |ctx|
  let root = test.temp_dir(ctx, name: "shred-invalid")?
  let result = shred_run(ctx, root, ["--remove=wip", "file"])?
  assert result.status == 1
  assert "ambiguous" in result.stderr
}

test test_shred_wipe_rename_uses_shortening_names { |ctx|
  let root = test.temp_dir(ctx, name: "shred-rename")?
  fp"{root}/test".write("content")
  fp"{root}/000".write("")
  let result = shred_run(ctx, root, ["-vu", "test"])?
  assert result.status == 0, result.stderr
  assert "test: renamed to 0000" in result.stderr
  assert "test: renamed to 001" in result.stderr
  assert "test: renamed to 00" in result.stderr
  assert "test: removed" in result.stderr
  assert ! fp"{root}/test".exists()?
}
