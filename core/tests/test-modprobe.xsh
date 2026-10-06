type KmodRun = {success: Bool, status: Int, stdout: Str, stderr: Str, stdout_bytes: Bytes, stderr_bytes: Bytes}

proc run_modprobe(ctx: TestContext, argv: List[Str], log: Path) -> KmodRun {
  test.linux_fake(ctx, {log: log})
  let source = fp"{ctx.core_dir}/modprobe.xsh".read_text()?
  test.run_script(ctx, source, argv, {XSH_MODULE_PATH: ctx.core_dir}, b"", "modprobe")?
}

test test_modprobe_parameters_and_remove_under_fake { |ctx|
  let log = test.temp_file(ctx, name: "modprobe.jsonl", contents: b"")?
  let output = run_modprobe(ctx, ["demo", "debug=1"], log)
  assert output.success, output.stderr
  assert "\"op\":\"modprobe\"" in log.read_text()?
  assert "\"params\":\"debug=1\"" in log.read_text()?
  let removed = run_modprobe(ctx, ["--remove", "first", "second"], log)
  assert removed.success, removed.stderr
  assert "\"remove\":\"true\"" in log.read_text()?
}

test test_modprobe_dry_run_never_inserts { |ctx|
  let log = test.temp_file(ctx, name: "modprobe-dry.jsonl", contents: b"")?
  let output = run_modprobe(ctx, ["--dry-run", "--verbose", "demo", "debug=1"], log)
  assert output.success, output.stderr
  assert "insmod " in output.stdout
  let calls = log.read_text()?
  assert "\"op\":\"module_plan\"" in calls
  assert "\"op\":\"modprobe\"" not in calls
}

test test_modprobe_clustered_short_options_and_rejected_config { |ctx|
  let log = test.temp_file(ctx, name: "modprobe-options.jsonl", contents: b"")?
  let output = run_modprobe(ctx, ["-nv", "demo"], log)
  assert output.success, output.stderr
  assert "insmod " in output.stdout
  let before = log.read_text()?
  let rejected = run_modprobe(ctx, ["--config", "/tmp/custom.conf", "demo"], log)
  assert rejected.status == 1
  assert "not supported" in rejected.stderr
  assert log.read_text()? == before
}

test test_modprobe_dry_run_missing_module_in_empty_tree { |ctx|
  guard system.uname()?.sysname == "Linux" else { test.skip("module planning requires Linux"); return }
  let root = test.temp_dir(ctx, name: "modprobe-empty")?
  let source = fp"{ctx.core_dir}/modprobe.xsh".read_text()?
  let output = test.run_script(ctx, source, ["--dry-run", "absent-module"], {XSH_MODULE_PATH: ctx.core_dir, XSH_MODULES_DIR: root}, b"", "modprobe")?
  assert output.status == 1
  assert "could not be resolved" in output.stderr
  assert output.stdout == ""
  let quiet = test.run_script(ctx, source, ["--quiet", "--dry-run", "absent-module"], {XSH_MODULE_PATH: ctx.core_dir, XSH_MODULES_DIR: root}, b"", "modprobe")?
  assert quiet.status == 1
  assert quiet.stderr == ""
}
