type Ran = {status: Int, stdout: Str, stderr: Str}

proc mktemp_run(ctx: TestContext, root: Path, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/mktemp.xsh"
  let argv = [ctx.xsh_bin.display(), script.display(), "--", @args]
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", TMPDIR: root.display()}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_mktemp_creates_file_with_template { |ctx|
  let root = test.temp_dir(ctx, name: "mktemp")?
  let result = mktemp_run(ctx, root, ["file.XXXXXXXXXX"])?
  assert result.status == 0, result.stderr
  assert result.stdout.starts_with("file.")
  let printed = fp"{result.stdout.trim()}"
  let created = fp"{root}/{printed.basename()}"
  assert created.exists()?
  assert created.metadata()?.mode % 512 == 0o600
}

test test_mktemp_creates_directory_and_honors_suffix { |ctx|
  let root = test.temp_dir(ctx, name: "mktemp-dir")?
  let result = mktemp_run(ctx, root, ["-d", "--suffix", ".work", "job.XXXXXX"])?
  assert result.status == 0, result.stderr
  assert result.stdout.trim().ends_with(".work")
  let printed = fp"{result.stdout.trim()}"
  assert fp"{root}/{printed.basename()}".metadata()?.kind == "dir"
}

test test_mktemp_dry_run_does_not_create_target { |ctx|
  let root = test.temp_dir(ctx, name: "mktemp-dry")?
  let result = mktemp_run(ctx, root, ["-u", "trial.XXXXXX"])?
  assert result.status == 0, result.stderr
  let printed = fp"{result.stdout.trim()}"
  assert ! fp"{root}/{printed.basename()}".exists()?
}

test test_mktemp_rejects_short_template { |ctx|
  let root = test.temp_dir(ctx, name: "mktemp-invalid")?
  let result = mktemp_run(ctx, root, ["short.XX"])?
  assert result.status == 1
  assert "too few X's" in result.stderr
}
