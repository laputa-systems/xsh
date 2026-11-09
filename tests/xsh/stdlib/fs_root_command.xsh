proc install_shell(base: Path) [fs, error] {
  fp"{base}/root/bin".mkdir(parents: true)
  p"/bin/sh".resolve()?.copy(to: fp"{base}/root/bin/sh")
  fp"{base}/root/bin/sh".chmod(0o755)
}

proc linux_only() [env, error] -> Bool {
  if system.uname()?.sysname != "Linux" {
    test.skip("FsRoot.command is Linux only")
    return false
  }
  true
}

test fs_root_command_runs_with_argv_env_and_cwd { |ctx|
  return unless linux_only()
  let base = test.temp_dir(ctx, name: "root-command-run")?
  install_shell(base)
  fp"{base}/work".mkdir()
  let root = fs.open_root(fp"{base}/root")?
  let plan = root.command(
    p"bin/sh",
    ["sh", "-c", "printf '%s|%s|%s|' \"$0\" \"$1\" \"$MARK\"; pwd", "first", "second"],
    cwd: fp"{base}/work",
    env: {MARK: "marked"},
    stdout: fp"{base}/out",
  )?
  assert process.run(plan)?.ok
  assert fp"{base}/out".read_text()? == f"first|second|marked|{base}/work\n"
  let handle = spawn plan ?
  assert wait handle ?.ok
}

test fs_root_command_takes_a_mixed_str_and_path_argv_beside_named_options { |ctx|
  return unless linux_only()
  let base = test.temp_dir(ctx, name: "root-command-mixed-argv")?
  install_shell(base)
  let root = fs.open_root(fp"{base}/root")?
  let plan = root.command(p"bin/sh", ["sh", "-c", "printf '%s|%s' \"$0\" \"$MARK\"", p"word/path"], env: {MARK: "marked"}, stdout: fp"{base}/out")?
  assert process.run(plan)?.ok
  assert fp"{base}/out".read_text()? == "word/path|marked"
}

test fs_root_command_runs_the_opened_file_after_its_path_is_swapped { |ctx|
  return unless linux_only()
  let base = test.temp_dir(ctx, name: "root-command-swap")?
  install_shell(base)
  fp"{base}/outside".mkdir()
  fp"{base}/outside/sh".write("#!/bin/sh\necho outside > \"$OUT\"\n")
  fp"{base}/outside/sh".chmod(0o755)
  let root = fs.open_root(fp"{base}/root")?
  let argv = ["sh", "-c", "echo inside > \"$OUT\""]
  let rooted = root.command(p"bin/sh", argv, env: {OUT: f"{base}/rooted"})?
  let by_path = process.command_argv(fp"{base}/root/bin/sh", argv, env: {OUT: f"{base}/by-path"})

  root.rename(from: p"bin", to: p"moved")?
  root.symlink(target: fp"{base}/outside", path: p"bin")?

  assert process.run(rooted)?.ok
  assert fp"{base}/rooted".read_text()? == "inside\n"
  assert process.run(by_path)?.ok
  assert fp"{base}/by-path".read_text()? == "outside\n"
  test.error_kind(root.command(p"bin/sh", argv), "fs-root-command")
}

test fs_root_command_refuses_interpreter_scripts_and_non_executables { |ctx|
  return unless linux_only()
  let base = test.temp_dir(ctx, name: "root-command-refuse")?
  fp"{base}/root".mkdir()
  let root = fs.open_root(fp"{base}/root")?
  root.write(p"script", "#!/bin/sh\nexit 0\n")
  root.chmod(p"script", 0o755)
  root.write(p"plain", "data")
  root.chmod(p"plain", 0o644)
  root.mkdir(p"dir")
  test.error_kind(root.command(p"script", ["script"]), "interpreted-executable")
  test.error_kind(root.command(p"plain", ["plain"]), "not-executable")
  test.error_kind(root.command(p"dir", ["dir"]), "not-executable")
  test.error_kind(root.command(p"missing", ["missing"]), "fs-root-command")
}

test fs_root_command_fails_in_consumers_that_resolve_a_pathname { |ctx|
  return unless linux_only()
  let base = test.temp_dir(ctx, name: "root-command-consumer")?
  install_shell(base)
  let root = fs.open_root(fp"{base}/root")?
  let plan = root.command(p"bin/sh", ["sh", "-c", "exit 0"])?
  test.error_kind(unix.spawn_process_group(plan), "rooted-executable-unsupported")
}
