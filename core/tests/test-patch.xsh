type Ran = {status: Int, stdout: Str, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str], input = b"", vars: Record = {LC_ALL: "C", TERM: "xterm"}, cwd: Path? = null) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "patch-capture")?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/patch.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), cwd ?? root, vars, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8()?, stderr: err.read_text()?})
}

test test_patch_applies_unified_stdin { |ctx|
  let root = test.temp_dir(ctx, name: "patch-root")?
  let target = fp"{root}/file"
  target.write("old\n")
  let patch_text = b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n"
  let result = invoke(ctx, ["-p1", "-d", root.display()], patch_text)?
  assert result.status == 0, result.stderr
  assert target.read_text()? == "new\n"
}

test test_patch_rejects_escape_and_unsupported_dry_run { |ctx|
  let root = test.temp_dir(ctx, name: "patch-root")?
  let escaped = invoke(ctx, ["-d", root.display()], b"--- /dev/null\n+++ ../outside\n@@ -0,0 +1 @@\n+bad\n")?
  assert escaped.status != 0
  assert !fp"{root}/../outside".exists()?
  assert invoke(ctx, ["--dry-run"], b"")?.status != 0
}

test test_patch_named_original_and_input_file { |ctx|
  let root = test.temp_dir(ctx, name: "patch-original")?
  let target = fp"{root}/target"
  target.write("old\n")
  let input = test.temp_file(ctx, name: "patch-input", contents: b"--- old-name\n+++ new-name\n@@ -1 +1 @@\n-old\n+new\n")?
  let result = invoke(ctx, [target.display(), input.display()])?
  assert result.status == 0, result.stderr
  assert target.read_text()? == "new\n"
}

test test_patch_multi_file_failure_keeps_prior_applied_file { |ctx|
  let root = test.temp_dir(ctx, name: "patch-multi")?
  fp"{root}/first".write("old\n")
  fp"{root}/second".write("different\n")
  let input = b"--- first\n+++ first\n@@ -1 +1 @@\n-old\n+new\n--- second\n+++ second\n@@ -1 +1 @@\n-old\n+new\n"
  let result = invoke(ctx, ["-d", root.display()], input)?
  assert result.status == 1
  assert fp"{root}/first".read_text()? == "new\n"
  assert fp"{root}/second".read_text()? == "different\n"
}
