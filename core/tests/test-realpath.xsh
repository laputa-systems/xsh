type Outcome = {status: Int, stdout: Str, stderr: Str}

proc run_applet(ctx: TestContext, root: Path, args: List[Str], tempdir: Str = "") [fs, process, error] -> Result[Outcome] {
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/realpath.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), root, {TMPDIR: tempdir, LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

test test_realpath { |ctx|
  let root = test.temp_dir(ctx, name: "realpath")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/realpath.xsh" -- $root
  assert output.trim() == root.resolve()?.display()
}


test test_realpath_physical_logical_relative_and_missing { |ctx|
  let root = test.temp_dir(ctx, name: "realpath-resolution")?
  fp"{root}/dir/inner".mkdir(parents: true)
  fp"{root}/alias".symlink(to: p"dir/inner")
  assert run_applet(ctx, root, ["alias/.."])?.stdout == root.display() + "/dir\n"
  assert run_applet(ctx, root, ["-L", "alias/.."])?.stdout == root.display() + "\n"
  assert run_applet(ctx, root, ["--relative-to=dir", "dir/inner"])?.stdout == "inner\n"
  assert run_applet(ctx, root, ["-m", "--relative-to=dir", "absent"])?.stdout == "../absent\n"
  assert run_applet(ctx, root, ["-e", "absent"])?.status == 1
  assert run_applet(ctx, root, ["-s", "alias"])?.stdout == root.display() + "/alias\n"
}


test test_realpath_canonical_modes_in_order_and_trailing_slashes { |ctx|
  let root = test.temp_dir(ctx, name: "realpath-trailing")?
  fp"{root}/file".write("data")
  fp"{root}/link".symlink(to: p"absent")
  assert run_applet(ctx, root, ["link/"])?.stdout == root.display() + "/absent\n"
  assert run_applet(ctx, root, ["-e", "-m", "absent/child"])?.status == 0
  assert run_applet(ctx, root, ["-m", "-e", "absent/child"])?.status == 1
  assert run_applet(ctx, root, ["-s", "file/."])?.status == 1
  assert run_applet(ctx, root, ["-m", "file/child"])?.stdout == root.display() + "/file/child\n"
}


test test_realpath_empty_relative_options_fail { |ctx|
  let root = test.temp_dir(ctx, name: "realpath-relative-empty")?
  assert run_applet(ctx, root, ["--relative-to=", "."])?.status == 1
  assert run_applet(ctx, root, ["--relative-base=", "--relative-to=.", "."])?.status == 1
}

test test_realpath_accepts_non_utf8_existing_paths { |ctx|
  let root = test.temp_dir(ctx, name: "realpath-invalid-utf8")?.resolve()?
  let target = Path.parse_bytes(bytes.concat([root.bytes(), b"/test_\xff\xfe.txt"]))?
  target.write("ok")
  let script = fp"{ctx.core_dir}/realpath.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let words: List[Union[Str, Path]] = [ctx.xsh_bin, script, target]
  let status = process.run(process.command_argv(ctx.xsh_bin, words, root, {LC_ALL: "C"}, b"", stdout, stderr))?
  assert status.exit_code()? == 0
  assert stdout.read_bytes()? == bytes.concat([target.bytes(), b"\n"])
  assert stderr.read_bytes()? == b""

  let relative_words: List[Union[Str, Path]] = [ctx.xsh_bin, script, "--", "--relative-to=.", target]
  let relative_status = process.run(process.command_argv(ctx.xsh_bin, relative_words, root, {LC_ALL: "C"}, b"", stdout, stderr))?
  assert relative_status.exit_code()? == 0
  assert stdout.read_bytes()? == b"test_\xff\xfe.txt\n"
}
