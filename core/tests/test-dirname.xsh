type Outcome = {status: Int, stdout: Str, stderr: Str}

proc run_applet(ctx: TestContext, root: Path, args: List[Str], tempdir: Str = "") [fs, process, error] -> Result[Outcome] {
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/dirname.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), root, {TMPDIR: tempdir, LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

test test_dirname { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/dirname.xsh" -- /tmp/demo/file.txt
  assert output.trim() == "/tmp/demo"
  let many = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/dirname.xsh" -- /tmp/a/one.txt /tmp/b/two.txt
  assert "/tmp/a" in many
  assert "/tmp/b" in many
}


test test_dirname_lexical_components_and_nul { |ctx|
  let root = test.temp_dir(ctx, name: "dirname-lexical")?
  let result = run_applet(ctx, root, ["-z", "foo/./bar", "foo//bar///", "//", "", "foo/."])?
  assert result.status == 0
  assert result.stdout == "foo/.\0foo\0/\0.\0foo\0"
  assert run_applet(ctx, root, [])?.status == 1
}


test test_dirname_unicode_components { |ctx|
  let root = test.temp_dir(ctx, name: "dirname-unicode")?
  assert run_applet(ctx, root, ["emoji/😀", "😀/.", "😀///file"])?.stdout == "emoji\n😀\n😀\n"
}
