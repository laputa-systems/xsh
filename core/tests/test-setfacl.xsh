use core.lib.acl

test test_acl_binary_codec_orders_entries_and_rejects_invalid { |ctx|
  let entries = acl.parse("u::rwx,g::r-x,o::---,u:1001:rw-,m::r--")?
  let payload = acl.encode(entries)?
  assert payload.len() == 44
  assert acl.decode(payload)? == acl.canonical(entries)
  assert acl.encode(acl.parse("u::rwx,g::r-x,o::---,u:1001:rw-")?) is Err(_)
  assert acl.encode(acl.parse("u::rwx,u::r--,g::r-x,o::---")?) is Err(_)
  assert acl.decode(b"\x01\0\0\0") is Err(_)
  assert acl.parse("u:1001:banana") is Err(_)
  assert acl.parse("u:4294967295:r--") is Err(_)
}

test test_acl_mask_calculation_and_mode_projection { |ctx|
  let entries = acl.calculate_mask(acl.parse("u::rw-,u:1001:rwx,g::r--,g:1002:-w-,o::---")?)
  assert acl.mode(entries)? == 0o670
  assert acl.permissions(7) == "rwx"
  assert acl.permissions(0) == "---"
}

type AclOutput = {status: Int, stdout: Str, stderr: Str}
proc acl_run(ctx: TestContext, root: Path, command: Str, args: List[Str]) [fs, process, error] -> Result[AclOutput] {
  let out = fp"{root}/stdout"; let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/{command}.xsh"
  let child = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), root, {LC_ALL: "C"}, b"", out, err)
  let result = process.run(child)?
  {status: result.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?}
}

test test_setfacl_kernel_masks_defaults_and_invalid_records_preserve_files { |ctx|
  let root = test.temp_dir(ctx, name: "acl-kernel")?
  let file = fp"{root}/file"; file.write("keep"); file.chmod(0o640)
  let probe = fs.xattr_set(file, "system.posix_acl_access", acl.encode(acl.calculate_mask(acl.parse("u::rw-,u:12345:r--,g::r--,o::---")?))?)
  if let Err(failure) = probe {
    if (failure.errno ?? -1) in [95, 45, 1] { test.skip(f"ACL fixture unavailable: {failure.message}"); return }
    test.fail(failure.message)
  }
  let before = fs.xattr_get(file, "system.posix_acl_access")?
  assert acl_run(ctx, root, "setfacl", ["-m", "u:12345:bad", "file"])?.status != 0
  assert fs.xattr_get(file, "system.posix_acl_access")? == before
  assert acl_run(ctx, root, "setfacl", ["-m", "u:12345:rwx,m::r--", "file"])?.status == 0
  assert fs.stat(file)?.mode.bit_and(0o777) == 0o640
  let output = acl_run(ctx, root, "getfacl", ["-cn", "file"])?
  assert output.status == 0, output.stderr
  assert "user:12345:rwx" in output.stdout and "#effective:r--" in output.stdout
  assert acl_run(ctx, root, "setfacl", ["--mask", "-m", "u:12345:rwx", "file"])?.status == 0
  assert fs.stat(file)?.mode.bit_and(0o777) == 0o670
  assert acl_run(ctx, root, "setfacl", ["-b", "file"])?.status == 0
  assert acl.read(file)?.len() == 3
  let dir = fp"{root}/dir"; dir.mkdir()
  assert acl_run(ctx, root, "setfacl", ["-d", "-m", "u:12345:r-x", "dir"])?.status == 0
  assert "default:user:12345:r-x" in acl_run(ctx, root, "getfacl", ["-dn", "dir"])?.stdout
  assert acl_run(ctx, root, "setfacl", ["-k", "dir"])?.status == 0
  assert fs.xattr_get(dir, "system.posix_acl_default") is Err(_)
  assert file.read_text()? == "keep"
}

test test_setfacl_test_and_restore_prevalidate_without_mutation { |ctx|
  let root = test.temp_dir(ctx, name: "acl-restore")?
  let file = fp"{root}/file"; file.write("keep"); file.chmod(0o600)
  let preview = acl_run(ctx, root, "setfacl", ["--test", "-m", "u:12345:rX", "file"])?
  assert preview.status == 0, preview.stderr
  assert "u:12345:r--" in preview.stdout
  assert fs.stat(file)?.mode.bit_and(0o777) == 0o600
  fp"{root}/dump".write("# file: file\nuser::rwx\ngroup::r-x\nother::---\n\n# file: file\nuser::banana\n")
  assert acl_run(ctx, root, "setfacl", ["--restore=dump"])?.status != 0
  assert fs.stat(file)?.mode.bit_and(0o777) == 0o600
  fp"{root}/dump".write("# file: file\nuser::rw-\ngroup::r--\nother::---\n")
  assert acl_run(ctx, root, "setfacl", ["--restore=dump"])?.status == 0
  assert fs.stat(file)?.mode.bit_and(0o777) == 0o640
}

test test_setfacl_preserved_masks_stripping_and_operand_scopes { |ctx|
  let root = test.temp_dir(ctx, name: "acl-scopes")?
  let first = fp"{root}/first"; let second = fp"{root}/second"
  first.write("one"); second.write("two"); first.chmod(0o660); second.chmod(0o660)
  assert acl_run(ctx, root, "setfacl", ["-n", "-m", "u:12345:rwx", "first"])?.status == 0
  assert fs.stat(first)?.mode.bit_and(0o777) == 0o660
  assert acl_run(ctx, root, "setfacl", ["-m", "m::r--", "-m", "u:12345:rwx", "first"])?.status == 0
  assert fs.stat(first)?.mode.bit_and(0o777) == 0o640
  assert acl_run(ctx, root, "setfacl", ["-b", "first"])?.status == 0
  assert fs.stat(first)?.mode.bit_and(0o777) == 0o640
  assert acl_run(ctx, root, "setfacl", ["-m", "u:12345:r--", "first", "-m", "u:12346:-w-", "second"])?.status == 0
  assert "user:12346" not in acl_run(ctx, root, "getfacl", ["-cn", "first"])?.stdout
  assert "user:12345" not in acl_run(ctx, root, "getfacl", ["-cn", "second"])?.stdout
}
