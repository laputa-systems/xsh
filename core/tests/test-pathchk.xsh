type Ran = {status: Int, stdout: Str, stderr: Str}

proc pathchk_run(ctx: TestContext, root: Path, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/pathchk.xsh"
  let argv = [ctx.xsh_bin.display(), script.display(), @args]
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_pathchk_accepts_ordinary_names { |ctx|
  let root = test.temp_dir(ctx, name: "pathchk")?
  let result = pathchk_run(ctx, root, ["ordinary", "subdir/file"])?
  assert result.status == 0, result.stderr
}

test test_pathchk_posix_and_special_modes { |ctx|
  let root = test.temp_dir(ctx, name: "pathchk-portable")?
  let too_long = "abcdefghijklmnop"
  let portable = pathchk_run(ctx, root, ["-p", too_long])?
  assert portable.status == 1
  assert "14 bytes" in portable.stderr
  let leading = pathchk_run(ctx, root, ["-P", "dir/-bad"])?
  assert leading.status == 1
  assert "leading '-'" in leading.stderr
}

test test_pathchk_rejects_non_directory_parent { |ctx|
  let root = test.temp_dir(ctx, name: "pathchk-parent")?
  fp"{root}/file".write("x")
  let result = pathchk_run(ctx, root, ["file/child"])?
  assert result.status == 1
  assert "Not a directory" in result.stderr
}

test test_pathchk_empty_default_name_reports_missing_path { |ctx|
  let root = test.temp_dir(ctx, name: "pathchk-empty")?
  let ordinary = pathchk_run(ctx, root, [""])?
  assert ordinary.status == 1
  assert ordinary.stderr == "pathchk: '': No such file or directory\n", ordinary.stderr
  let portable = pathchk_run(ctx, root, ["-p", ""])?
  assert portable.status == 1
  assert portable.stderr == "pathchk: empty file name\n", portable.stderr
}
