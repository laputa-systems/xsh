type Ran = {status: Int, stdout: Str, stderr: Str}

proc chroot_run(ctx: TestContext, root: Path, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/chroot.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", out, err))?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_chroot_help_version_and_usage { |ctx|
  let root = test.temp_dir(ctx, name: "chroot-usage")?
  let help = chroot_run(ctx, root, ["--help"])?
  assert help.status == 0
  assert "Usage: chroot" in help.stdout

  let version = chroot_run(ctx, root, ["--version"])?
  assert version.status == 0
  assert version.stdout.starts_with("chroot ")

  let missing = chroot_run(ctx, root, [])?
  assert missing.status == 125
  assert missing.stderr == "chroot: missing operand\nTry 'chroot --help' for more information.\n", missing.stderr

  let unknown = chroot_run(ctx, root, ["--definitely-invalid"])?
  assert unknown.status == 125
  assert "unrecognized option '--definitely-invalid'" in unknown.stderr, unknown.stderr
}

test test_chroot_reports_root_path_errors_before_chrooting { |ctx|
  let root = test.temp_dir(ctx, name: "chroot-errors")?
  let missing = chroot_run(ctx, root, ["missing"])?
  assert missing.status == 125
  assert "cannot chroot to 'missing': No such file or directory" in missing.stderr, missing.stderr

  let file = fp"{root}/not-a-directory"
  file.write("x")
  let not_dir = chroot_run(ctx, root, [file.display()])?
  assert not_dir.status == 125
  assert "Not a directory" in not_dir.stderr, not_dir.stderr

  let skip = chroot_run(ctx, root, ["--skip-chdir", root.display()])?
  assert skip.status == 125
  assert skip.stderr == "chroot: option --skip-chdir only permitted if NEWROOT is old '/'\n", skip.stderr
}
