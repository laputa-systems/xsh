type Outcome = {status: Int, stdout: Str, stderr: Str}

proc run_applet(ctx: TestContext, root: Path, args: List[Str], tempdir: Str = "") [fs, process, error] -> Result[Outcome] {
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/truncate.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), root, {TMPDIR: tempdir, LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

test test_truncate_relative_sizes_and_reference { |ctx|
  let root = test.temp_dir(ctx, name: "truncate")?
  let file = fp"{root}/file"
  file.write("abcdefghij")
  assert run_applet(ctx, root, ["-s", "-3", "file"])?.status == 0
  assert file.read_text()? == "abcdefg"
  assert run_applet(ctx, root, ["-s", "%4", "file"])?.status == 0
  assert fs.stat(file)?.size == 8
  assert run_applet(ctx, root, ["-s", "/3", "file"])?.status == 0
  assert fs.stat(file)?.size == 6
  assert run_applet(ctx, root, ["-r", "file", "-s", "+2", "new"])?.status == 0
  assert fs.stat(fp"{root}/new")?.size == 8
  assert run_applet(ctx, root, ["-s", "2K", "file"])?.status == 0
  assert fs.stat(file)?.size == 2048
}

test test_truncate_invalid_sizes_preserve_files_and_no_create { |ctx|
  let root = test.temp_dir(ctx, name: "truncate-invalid")?
  let file = fp"{root}/file"
  file.write("keep")
  assert run_applet(ctx, root, ["-s", "9223372036854775808", "new"])?.status == 1
  assert ! fp"{root}/new".exists()?
  assert run_applet(ctx, root, ["-s", "/0", "file"])?.status == 1
  assert file.read_text()? == "keep"
  assert run_applet(ctx, root, ["-c", "-s", "4", "absent"])?.status == 0
  assert ! fp"{root}/absent".exists()?
  assert run_applet(ctx, root, ["-s", "2", "missing/file", "file"])?.status == 1
  assert file.read_text()? == "ke"
}


test test_truncate_unit_only_radices_and_relative_reference_requirement { |ctx|
  let root = test.temp_dir(ctx, name: "truncate-unit")?
  assert run_applet(ctx, root, ["-s", "K", "file"])?.status == 0
  assert fs.stat(fp"{root}/file")?.size == 1024
  assert run_applet(ctx, root, ["-s", "0x10", "file"])?.status == 0
  assert fs.stat(fp"{root}/file")?.size == 16
  assert run_applet(ctx, root, ["-s", "020", "file"])?.status == 0
  assert fs.stat(fp"{root}/file")?.size == 16
  assert run_applet(ctx, root, ["-r", "file", "-s", "2", "new"])?.status == 1
  assert ! fp"{root}/new".exists()?
  assert run_applet(ctx, root, ["-s", "1b", "new"])?.status == 1
}


test test_truncate_unicode_invalid_size_is_an_operand_error { |ctx|
  let root = test.temp_dir(ctx, name: "truncate-unicode-size")?
  let result = run_applet(ctx, root, ["-s", "😀", "file"])?
  assert result.status == 1
  assert "Invalid number" in result.stderr
  assert ! fp"{root}/file".exists()?
}

test test_truncate_without_size_or_reference_reports_required_argument { |ctx|
  let root = test.temp_dir(ctx, name: "truncate-no-size")?
  let result = run_applet(ctx, root, ["file"])?
  assert result.status == 1
  assert "error: the following required arguments were not provided:" in result.stderr, result.stderr
  assert result.stdout == ""
}
