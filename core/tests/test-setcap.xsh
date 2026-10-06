use core.lib.capability

type CapOutput = {status: Int, stdout: Str, stderr: Str}
proc cap_run(ctx: TestContext, root: Path, command: Str, args: List[Str]) [fs, process, error] -> Result[CapOutput] {
  let out = fp"{root}/stdout"; let err = fp"{root}/stderr"; let script = fp"{ctx.core_dir}/{command}.xsh"
  let child = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), root, {LC_ALL: "C"}, b"", out, err)
  let result = process.run(child)?
  {status: result.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?}
}

test test_setcap_read_only_verification_and_invalid_input_never_modify { |ctx|
  let root = test.temp_dir(ctx, name: "setcap")?
  let file = fp"{root}/file"; file.write("keep")
  fp"{root}/link".symlink(to: p"file")
  assert cap_run(ctx, root, "setcap", ["-v", "-r", "file"])?.status == 0
  assert cap_run(ctx, root, "setcap", ["-v", "=", "file"])?.status == 0
  assert cap_run(ctx, root, "setcap", ["-v", "cap_chown=ep", "file"])?.status == 1
  assert cap_run(ctx, root, "setcap", ["cap_chown=e", "file"])?.status != 0
  assert cap_run(ctx, root, "setcap", ["cap_chown=ep", "file", "garbage", "file"])?.status != 0
  assert cap_run(ctx, root, "setcap", ["-v", "=", "link"])?.status == 1
  assert fs.xattr_get(file, "security.capability") is Err(_)
  assert file.read_text()? == "keep"
  assert cap_run(ctx, root, "getcap", ["file"])?.stdout == ""
  assert cap_run(ctx, root, "getcap", ["-v", "file"])?.stdout == "file\n"
  assert "Not a regular file" in cap_run(ctx, root, "getcap", ["-v", "link"])?.stdout
}
