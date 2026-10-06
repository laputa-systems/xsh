type AppletOutput = {status: Int, stdout: Bytes, stderr: Str}

proc attr_run(ctx: TestContext, root: Path, name: Str, args: List[Str]) [fs, process, error] -> Result[AppletOutput] {
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/{name}.xsh"
  let command = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), root, {LC_ALL: "C"}, b"", out, err)
  let status = process.run(command)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_setfattr_binary_encodings_remove_and_raw { |ctx|
  let root = test.temp_dir(ctx, name: "setfattr")?
  let file = fp"{root}/file"
  file.write("content")
  let probe = fs.xattr_set(file, "user.probe", b"")
  if let Err(failure) = probe {
    if failure.errno == 95 or failure.errno == 45 { test.skip("xattrs unavailable on fixture filesystem"); return }
    test.fail(failure.message)
  }
  assert attr_run(ctx, root, "setfattr", ["-n", "user.binary", "-v", "0x00ff41", "file"])?.status == 0
  assert fs.xattr_get(file, "user.binary")? == b"\0\xffA"
  assert attr_run(ctx, root, "setfattr", ["-n", "user.binary", "-v", "0sAP9B", "file"])?.status == 0
  assert fs.xattr_get(file, "user.binary")? == b"\0\xffA"
  assert attr_run(ctx, root, "setfattr", ["--raw", "-n", "user.binary", "-v", "0x00", "file"])?.status == 0
  assert fs.xattr_get(file, "user.binary")? == b"0x00"
  assert attr_run(ctx, root, "setfattr", ["-x", "user.binary", "file"])?.status == 0
  assert fs.xattr_get(file, "user.binary") is Err(_)
  assert file.read_text()? == "content"
}

test test_setfattr_bad_encoding_preserves_value_and_operands_continue { |ctx|
  let root = test.temp_dir(ctx, name: "setfattr-errors")?
  let file = fp"{root}/file"
  file.write("content")
  fs.xattr_set(file, "user.value", b"keep")
  assert attr_run(ctx, root, "setfattr", ["-n", "user.value", "-v", "0x0", "file"])?.status == 1
  assert fs.xattr_get(file, "user.value")? == b"keep"
  assert attr_run(ctx, root, "setfattr", ["-n", "user.value", "-v", "changed", "absent", "file"])?.status == 1
  assert fs.xattr_get(file, "user.value")? == b"changed"
}


test test_setfattr_restore_dump_preserves_escaped_names_and_binary_values { |ctx|
  let root = test.temp_dir(ctx, name: "setfattr-restore")?
  let file = fp"{root}/line\nname"
  file.write("content")
  fs.xattr_set(file, "user.a=b", b"\0\xff")
  fs.xattr_set(file, "user.text", b"line\nquote\"slash\\")
  let dump = attr_run(ctx, root, "getfattr", ["--absolute-names", "-d", file.display()])?
  assert dump.status == 0, dump.stderr
  fp"{root}/dump".write(dump.stdout)
  fs.xattr_remove(file, "user.a=b")
  fs.xattr_remove(file, "user.text")
  let restored = attr_run(ctx, root, "setfattr", ["--restore=dump"])?
  assert restored.status == 0, restored.stderr
  assert fs.xattr_get(file, "user.a=b")? == b"\0\xff"
  assert fs.xattr_get(file, "user.text")? == b"line\nquote\"slash\\"
  fp"{root}/dump".write("# file: " + file.display().replace("\n", with: "\\012") + "\nuser.empty\n\n")
  assert attr_run(ctx, root, "setfattr", ["--restore=dump"])?.status == 0
  assert fs.xattr_get(file, "user.empty")? == b""
}


test test_setfattr_restore_forced_text_preserves_non_utf8_payload { |ctx|
  let root = test.temp_dir(ctx, name: "setfattr-restore-bytes")?
  let file = fp"{root}/file"
  file.write("content")
  fs.xattr_set(file, "user.binary", b"\xff\x01\n")
  let dump = attr_run(ctx, root, "getfattr", ["--absolute-names", "-d", "-e", "text", file.display()])?
  assert dump.status == 0, dump.stderr
  fp"{root}/dump".write(dump.stdout)
  fs.xattr_remove(file, "user.binary")
  let restored = attr_run(ctx, root, "setfattr", ["--restore=dump"])?
  assert restored.status == 0, restored.stderr
  assert fs.xattr_get(file, "user.binary")? == b"\xff\x01\n"
}


test test_setfattr_raw_also_controls_restore_values { |ctx|
  let root = test.temp_dir(ctx, name: "setfattr-restore-raw")?
  fp"{root}/file".write("content")
  fp"{root}/dump".write("# file: file\nuser.raw=0x00ff\n\n")
  let output = attr_run(ctx, root, "setfattr", ["--raw", "--restore=dump"])?
  assert output.status == 0, output.stderr
  assert fs.xattr_get(fp"{root}/file", "user.raw")? == b"0x00ff"
}
