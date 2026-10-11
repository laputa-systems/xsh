type Outcome = {status: Int, stdout: Str, stderr: Str}

proc run_applet(ctx: TestContext, root: Path, args: List[Str], tempdir: Str = "") [fs, process, error] -> Result[Outcome] {
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/readlink.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), root, {TMPDIR: tempdir, LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

test test_readlink { |ctx|
  let root = test.temp_dir(ctx, name: "readlink")?
  let target = fp"{root}/target.txt"
  let link = fp"{root}/link.txt"
  target.write("ok")
  link.symlink(to: target)
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/readlink.xsh" -- $link
  assert "target.txt" in output
  let resolved = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/readlink.xsh" -- -f $link
  assert resolved.trim() == target.resolve()?.display()
  let resolved_long = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/readlink.xsh" -- --canonicalize $link
  assert resolved_long.trim() == target.resolve()?.display()
}


test test_readlink_missing_components_and_silent_failure { |ctx|
  let root = test.temp_dir(ctx, name: "readlink-missing")?
  fp"{root}/link".symlink(to: p"missing")
  assert run_applet(ctx, root, ["-f", "link"])?.stdout == root.display() + "/missing\n"
  assert run_applet(ctx, root, ["-e", "link"])?.status == 1
  assert run_applet(ctx, root, ["-m", "absent/child/../last"])?.stdout == root.display() + "/absent/last\n"
  let silent = run_applet(ctx, root, ["not-a-link", "link"])?
  assert silent.status == 1
  assert silent.stderr == ""
  assert silent.stdout == "missing\n"
  assert run_applet(ctx, root, ["-z", "link"])?.stdout == "missing\0"
}

test test_readlink_without_a_file_reports_required_argument { |ctx|
  let root = test.temp_dir(ctx, name: "readlink-no-file")?
  let result = run_applet(ctx, root, [])?
  assert result.status == 1
  assert result.stderr == "readlink: missing operand\nTry 'readlink --help' for more information.\n", result.stderr
  assert result.stdout == ""
}

test test_readlink_accepts_non_utf8_symlink_names { |ctx|
  let root = test.temp_dir(ctx, name: "readlink-invalid-utf8")?.resolve()?
  let target = Path.parse_bytes(bytes.concat([root.bytes(), b"/target_file"]))?
  let link = Path.parse_bytes(bytes.concat([root.bytes(), b"/symlink_\xff\xfe"]))?
  target.write("ok")
  link.symlink(to: target)
  let script = fp"{ctx.core_dir}/readlink.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let words: List[Union[Str, Path]] = [ctx.xsh_bin, script, link]
  let status = process.run(process.command_argv(ctx.xsh_bin, words, root, {LC_ALL: "C"}, b"", stdout, stderr))?
  assert status.exit_code()? == 0
  assert stdout.read_bytes()? == bytes.concat([target.bytes(), b"\n"])
  assert stderr.read_bytes()? == b""
}
