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
  assert "shred: test: removing\nshred: test: renamed to 0000\nshred: 0000: renamed to 001\nshred: 001: renamed to 00\nshred: 00: renamed to 0\nshred: test: removed\n" in result.stderr, result.stderr
  assert ! fp"{root}/test".exists()?
}

test test_shred_zero_sized_proc_file_reaches_removal { |ctx|
  let target = /proc/self/mem
  if ! target.exists()? { return }
  let root = test.temp_dir(ctx, name: "shred-proc")?
  let result = shred_run(ctx, root, ["-u", target.display()])?
  assert result.status == 1, result.stderr
  assert "Couldn't rename to" in result.stderr, result.stderr
  assert "cannot read" not in result.stderr, result.stderr
}

test test_shred_dash_targets_stdout { |ctx|
  let root = test.temp_dir(ctx, name: "shred-stdout")?
  let result = shred_run(ctx, root, ["-u", "-"])?
  assert result.status == 0, result.stderr
  assert result.stdout == "", result.stdout
  assert result.stderr == "", result.stderr
  assert fp"{root}/out".exists()?
  let sized = shred_run(ctx, root, ["-n0", "-s4", "-z", "-"])?
  assert sized.status == 0, sized.stderr
  assert sized.stdout.byte_len() == 4, f"{sized.stdout.byte_len()}"
}
