type Ran = {status: Int, stdout: Str, stderr: Str}

proc truncate_run(ctx: TestContext, root: Path, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/truncate.xsh"
  let argv = [ctx.xsh_bin.display(), script.display(), @args]
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_truncate_absolute_relative_and_reference_sizes { |ctx|
  let root = test.temp_dir(ctx, name: "truncate")?
  let file = fp"{root}/file"
  let reference = fp"{root}/reference"
  file.write("abcdef")
  reference.write("0123456789")

  let absolute = truncate_run(ctx, root, ["--size=10", "file"])?
  assert absolute.status == 0, absolute.stderr
  assert file.metadata()?.size == 10
  let relative = truncate_run(ctx, root, ["--size=-3", "file"])?
  assert relative.status == 0, relative.stderr
  assert file.metadata()?.size == 7
  let based = truncate_run(ctx, root, ["--reference=reference", "--size=+2", "file"])?
  assert based.status == 0, based.stderr
  assert file.metadata()?.size == 12
}

test test_truncate_creates_and_no_create_skips { |ctx|
  let root = test.temp_dir(ctx, name: "truncate-create")?
  let skipped = truncate_run(ctx, root, ["--no-create", "--size=4", "absent"])?
  assert skipped.status == 0, skipped.stderr
  assert ! fp"{root}/absent".exists()?
  let created = truncate_run(ctx, root, ["--size=4", "created"])?
  assert created.status == 0, created.stderr
  assert fp"{root}/created".metadata()?.size == 4
}

test test_truncate_uses_size_suffix_and_rejects_zero_rounding { |ctx|
  let root = test.temp_dir(ctx, name: "truncate-size")?
  let file = fp"{root}/file"
  file.write("x")
  let scaled = truncate_run(ctx, root, ["--size=2kB", "file"])?
  assert scaled.status == 0, scaled.stderr
  assert file.metadata()?.size == 2000
  let invalid = truncate_run(ctx, root, ["--size=%0", "file"])?
  assert invalid.status == 1
}
