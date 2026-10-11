type Outcome = {status: Int, stdout: Str, stderr: Str}

proc run_applet(ctx: TestContext, root: Path, args: List[Str], tempdir: Str = "") [fs, process, error] -> Result[Outcome] {
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/pathchk.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), root, {TMPDIR: tempdir, LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

test test_pathchk_portability_and_missing_paths { |ctx|
  let root = test.temp_dir(ctx, name: "pathchk")?
  assert run_applet(ctx, root, ["missing/file"])?.status == 0
  assert run_applet(ctx, root, ["-p", "safe/file-name"])?.status == 0
  assert run_applet(ctx, root, ["-p", "unsafe/$name"])?.status == 1
  assert run_applet(ctx, root, ["-p", "123456789012345"])?.status == 1
  assert run_applet(ctx, root, ["-P", "dir/-file"])?.status == 1
  assert run_applet(ctx, root, ["--portability", ""])?.stderr == "pathchk: empty file name\n"
}

test test_pathchk_without_a_name_reports_required_argument { |ctx|
  let root = test.temp_dir(ctx, name: "pathchk-no-name")?
  let result = run_applet(ctx, root, [])?
  assert result.status == 1
  assert result.stderr == "pathchk: missing operand\nTry 'pathchk --help' for more information.\n", result.stderr
  assert result.stdout == ""
}

test test_pathchk_accepts_non_utf8_path_names { |ctx|
  let root = test.temp_dir(ctx, name: "pathchk-invalid-utf8")?
  let non_utf8_path = Path.parse_bytes(b"\xff\xfe")?
  let script = fp"{ctx.core_dir}/pathchk.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let words: List[Union[Str, Path]] = [ctx.xsh_bin, script, "--", non_utf8_path]
  let status = process.run(process.command_argv(ctx.xsh_bin, words, root, {LC_ALL: "C"}, b"", stdout, stderr))?
  assert status.exit_code()? == 0
  assert stdout.read_bytes()? == b""
  assert stderr.read_bytes()? == b""
}
