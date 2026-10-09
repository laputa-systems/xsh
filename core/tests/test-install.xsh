type Ran = {status: Int, stdout: Str, stderr: Str}

proc install_run(ctx: TestContext, root: Path, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/install.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?.exit_code()?
  Ok({status: status, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_install_copy_mode_and_verbose { |ctx|
  let root = test.temp_dir(ctx, name: "install-copy")?
  let source = fp"{root}/source"
  source.write("new contents")
  let dest = fp"{root}/bin/program"
  let result = install_run(ctx, root, ["-Dv", "-m", "755", source.display(), dest.display()])?
  assert result.status == 0
  assert result.stderr == ""
  assert dest.read_text()? == "new contents"
  assert dest.metadata()?.mode % 4096 == 0o755
  assert "creating directory" in result.stdout
  assert f"'{source}' -> '{dest}'\n" in result.stdout
}

test test_install_directory_parents_and_mode { |ctx|
  let root = test.temp_dir(ctx, name: "install-dirs")?
  let dir = fp"{root}/a/b/c"
  let result = install_run(ctx, root, ["-d", "-m", "750", dir.display()])?
  assert result.status == 0
  assert dir.metadata()?.kind == "dir"
  assert dir.metadata()?.mode % 4096 == 0o750
  assert fp"{root}/a".metadata()?.kind == "dir"
}

test test_install_compare_and_timestamps { |ctx|
  let root = test.temp_dir(ctx, name: "install-compare")?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  source.write("same")
  let first = install_run(ctx, root, ["-C", source.display(), dest.display()])?
  assert first.status == 0
  let before = fs.stat(dest)?.mtime_ns
  let second = install_run(ctx, root, ["-C", source.display(), dest.display()])?
  assert second.status == 0
  assert fs.stat(dest)?.mtime_ns == before

  fs.set_times(source, atime_ns: 1000000000, mtime_ns: 2000000000)?
  let preserved = install_run(ctx, root, ["-p", source.display(), dest.display()])?
  assert preserved.status == 0
  assert fs.stat(dest)?.mtime_ns == 2000000000
}

test test_install_backup_and_target_directory { |ctx|
  let root = test.temp_dir(ctx, name: "install-backup")?
  let source = fp"{root}/source"
  source.write("new")
  let target = fp"{root}/target"
  target.mkdir()
  let dest = fp"{target}/source"
  dest.write("old")
  let result = install_run(ctx, root, ["--backup=numbered", "-t", target.display(), source.display()])?
  assert result.status == 0
  assert dest.read_text()? == "new"
  assert fp"{dest}.~1~".read_text()? == "old"
}

test test_install_diagnostics_and_unsupported_options { |ctx|
  let root = test.temp_dir(ctx, name: "install-errors")?
  let missing = install_run(ctx, root, ["missing", "dest"])?
  assert missing.status == 1
  assert "cannot stat 'missing': No such file or directory" in missing.stderr

  let mode = install_run(ctx, root, ["-m", "999", "missing", "dest"])?
  assert mode.status == 1
  assert "Invalid mode string" in mode.stderr

  let unsupported = install_run(ctx, root, ["-Z", "source", "dest"])?
  assert unsupported.status == 1
  assert "SELinux contexts are not available" in unsupported.stderr
}

test test_install_applet_end_of_options { |ctx|
  let root = test.temp_dir(ctx, name: "install-double-dash")?
  let source = fp"{root}/-source"
  let dest = fp"{root}/-dest"
  source.write("contents")
  let result = install_run(ctx, root, ["--", "-source", "-dest"])?
  assert result.status == 0
  assert dest.read_text()? == "contents"
}
