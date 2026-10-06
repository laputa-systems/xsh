type AppletOutput = {status: Int, stdout: Bytes, stderr: Str}

proc attr_run(ctx: TestContext, root: Path, name: Str, args: List[Str]) [fs, process, error] -> Result[AppletOutput] {
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/{name}.xsh"
  let command = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), root, {LC_ALL: "C"}, b"", out, err)
  let status = process.run(command)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_getfattr_dump_encodings_sorting_and_only_values { |ctx|
  let root = test.temp_dir(ctx, name: "getfattr")?
  let file = fp"{root}/file"
  file.write("content")
  fs.xattr_set(file, "user.z", b"\0\xffA")
  fs.xattr_set(file, "user.a", b"hello")
  assert attr_run(ctx, root, "getfattr", ["-d", "file"])?.stdout == b"# file: file\nuser.a=\"hello\"\nuser.z=0sAP9B\n\n"
  assert attr_run(ctx, root, "getfattr", ["-n", "user.z", "-e", "hex", "file"])?.stdout == b"# file: file\nuser.z=0x00ff41\n\n"
  assert attr_run(ctx, root, "getfattr", ["-n", "user.z", "--only-values", "file"])?.stdout == b"\0\xffA"
  assert attr_run(ctx, root, "getfattr", ["-m", "^user\\.a$", "file"])?.stdout == b"# file: file\nuser.a\n\n"
}

test test_getfattr_recursive_follow_modes_and_cycles { |ctx|
  let root = test.temp_dir(ctx, name: "getfattr-recursive")?
  fp"{root}/dir".mkdir()
  let file = fp"{root}/dir/file"
  file.write("content")
  fs.xattr_set(file, "user.value", b"data")
  fp"{root}/alias".symlink(to: p"dir")
  fp"{root}/dir/loop".symlink(to: p".")
  let hybrid = attr_run(ctx, root, "getfattr", ["-R", "-d", "alias"])?
  assert hybrid.status == 0, hybrid.stderr
  assert "alias/file" in (hybrid.stdout.utf8() ?? "")
  assert attr_run(ctx, root, "getfattr", ["-R", "-P", "-d", "alias"])?.stdout == b""
  let cycle = attr_run(ctx, root, "getfattr", ["-R", "-L", "-d", "dir"])?
  assert cycle.status == 1
  assert "cycle" in cycle.stderr
}


test test_getfattr_explicit_nofollow_keeps_target_attributes_separate { |ctx|
  let root = test.temp_dir(ctx, name: "getfattr-nofollow")?
  let file = fp"{root}/file"
  file.write("content")
  fs.xattr_set(file, "user.value", b"target")
  fp"{root}/link".symlink(to: p"file")
  assert attr_run(ctx, root, "getfattr", ["-n", "user.value", "--only-values", "link"])?.stdout == b"target"
  let nofollow = attr_run(ctx, root, "getfattr", ["-h", "-n", "user.value", "link"])?
  assert nofollow.status == 1
  assert "No such attribute" in nofollow.stderr
  assert fs.xattr_get(file, "user.value")? == b"target"
}


test test_getfattr_option_values_do_not_change_walk_policy { |ctx|
  let root = test.temp_dir(ctx, name: "getfattr-option-values")?
  fp"{root}/dir".mkdir()
  fp"{root}/dir/file".write("content")
  fs.xattr_set(fp"{root}/dir/file", "user.-P", b"data")
  fp"{root}/alias".symlink(to: p"dir")
  let output = attr_run(ctx, root, "getfattr", ["-R", "-d", "-m", "-P", "alias"])?
  assert output.status == 0, output.stderr
  assert "alias/file" in (output.stdout.utf8() ?? "")
  let attached = attr_run(ctx, root, "getfattr", ["-R", "-d", "-m-P", "alias"])?
  assert attached.stdout == output.stdout
}


test test_getfattr_absolute_name_warning_and_raw_value_output { |ctx|
  let root = test.temp_dir(ctx, name: "getfattr-absolute")?
  let file = fp"{root}/file"
  file.write("content")
  fs.xattr_set(file, "user.value", b"data")
  let plain = attr_run(ctx, root, "getfattr", ["--only-values", "-n", "user.value", file.display()])?
  assert plain.stdout == b"data"
  assert "Removing leading '/'" in plain.stderr
  let absolute = attr_run(ctx, root, "getfattr", ["--absolute-names", "--only-values", "-n", "user.value", file.display()])?
  assert absolute.stdout == b"data"
  assert absolute.stderr == ""
}
