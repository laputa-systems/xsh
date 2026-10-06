type KmodRun = {success: Bool, status: Int, stdout: Str, stderr: Str, stdout_bytes: Bytes, stderr_bytes: Bytes}

proc run_modinfo(ctx: TestContext, argv: List[Str], log: Path) -> KmodRun {
  test.linux_fake(ctx, {log: log})
  let source = fp"{ctx.core_dir}/modinfo.xsh".read_text()?
  test.run_script(ctx, source, argv, {XSH_MODULE_PATH: ctx.core_dir}, b"", "modinfo")?
}

test test_modinfo_selected_fields_and_nul_output { |ctx|
  let log = test.temp_file(ctx, name: "modinfo.jsonl", contents: b"")?
  let output = run_modinfo(ctx, ["-F", "license", "demo"], log)
  assert output.success, output.stderr
  assert output.stdout == "GPL\n"
  let filename = run_modinfo(ctx, ["-0", "--filename", "demo"], log)
  assert filename.success, filename.stderr
  assert filename.stdout_bytes == b"/lib/modules/dry-run/demo.ko\0"
}

test test_modinfo_default_and_unsupported_root { |ctx|
  let log = test.temp_file(ctx, name: "modinfo-fields.jsonl", contents: b"")?
  let output = run_modinfo(ctx, ["demo"], log)
  assert output.success, output.stderr
  assert "filename:       /lib/modules/dry-run/demo.ko" in output.stdout
  assert "description:" in output.stdout
  assert "license:" in output.stdout
  let blocked = run_modinfo(ctx, ["--basedir", "/tmp", "demo"], log)
  assert blocked.status == 1
  assert "not supported" in blocked.stderr
}

test test_modinfo_missing_module_in_empty_tree { |ctx|
  guard system.uname()?.sysname == "Linux" else { test.skip("module metadata requires Linux"); return }
  let root = test.temp_dir(ctx, name: "modinfo-empty")?
  let source = fp"{ctx.core_dir}/modinfo.xsh".read_text()?
  let output = test.run_script(ctx, source, ["absent-module"], {XSH_MODULE_PATH: ctx.core_dir, XSH_MODULES_DIR: root}, b"", "modinfo")?
  assert output.status == 1
  assert "not found" in output.stderr
  assert output.stdout == ""
}
