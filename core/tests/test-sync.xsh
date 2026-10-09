type Ran = {status: Int, stdout: Str, stderr: Str}

proc sync_run(ctx: TestContext, root: Path, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/sync.xsh"
  let argv = [ctx.xsh_bin.display(), script.display(), @args]
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_sync_global_and_file_modes { |ctx|
  let root = test.temp_dir(ctx, name: "sync")?
  let file = fp"{root}/file"
  file.write("durable")
  let global = sync_run(ctx, root, [])?
  assert global.status == 0, global.stderr
  let data = sync_run(ctx, root, ["--data", "file"])?
  assert data.status == 0, data.stderr
  let filesystem = sync_run(ctx, root, ["--file-system", "file"])?
  assert filesystem.status == 0, filesystem.stderr
}

test test_sync_reports_missing_file_and_rejects_unknown_option { |ctx|
  let root = test.temp_dir(ctx, name: "sync-error")?
  let missing = sync_run(ctx, root, ["missing"])?
  assert missing.status == 1
  assert "error opening" in missing.stderr
  let invalid = sync_run(ctx, root, ["--definitely-invalid"])?
  assert invalid.status == 1
  assert "unrecognized option" in invalid.stderr
}

test test_sync_data_requires_a_file { |ctx|
  let root = test.temp_dir(ctx, name: "sync-data")?
  let result = sync_run(ctx, root, ["--data"])?
  assert result.status == 1
  assert result.stderr == "sync: --data needs at least one argument\n", result.stderr
}
