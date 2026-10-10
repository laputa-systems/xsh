test test_rm_force_recursive { |ctx|
  let root = test.temp_dir(ctx, name: "rm")?
  let dir = fp"{root}/dir"
  dir.mkdir()
  fp"{dir}/nested.txt".write("nested")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/rm.xsh" -- -rf $dir fp"{root}/missing"
  assert ! dir.exists()?
}

test test_rm_accepts_non_utf8_operands { |ctx|
  let root = test.temp_dir(ctx, name: "rm-raw-path")?
  let file = Path.parse_bytes(bytes.concat([root.bytes(), b"/file\xff"]))?
  file.write("remove")
  let directory = Path.parse_bytes(bytes.concat([root.bytes(), b"/directory\xfe"]))?
  let child = Path.parse_bytes(bytes.concat([directory.bytes(), b"/child\xfd"]))?
  directory.mkdir()
  child.write("remove")
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/rm.xsh"
  let file_status = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin, p"--", script, p"--", file], root, {LC_ALL: "C"}, b"", stdout, stderr))?
  assert file_status.exited_with(0), stderr.read_text()?
  assert fs.stat(file) is Err(_)
  let missing_status = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin, p"--", script, p"--", file], root, {LC_ALL: "C"}, b"", stdout, stderr))?
  let missing_error = stderr.read_text()?
  assert missing_status.exited_with(1)
  assert r"\377" in missing_error
  assert "No such file or directory" in missing_error

  let directory_status = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin, p"--", script, p"-r", p"--", directory], root, {LC_ALL: "C"}, b"", stdout, stderr))?
  assert directory_status.exited_with(0), stderr.read_text()?
  assert fs.stat(directory) is Err(_)
  assert fs.stat(child) is Err(_)
}

test test_rm_progress_options { |ctx|
  let root = test.temp_dir(ctx, name: "rm-progress")?
  let file = fp"{root}/file"
  file.write("remove")
  let short = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/rm.xsh" -- -g $file
  assert short.status.exited_with(0), short.stderr
  assert short.stderr == ""
  assert ! file.exists()?

  let tree = fp"{root}/tree"
  let nested = fp"{tree}/nested"
  nested.mkdir()
  fp"{nested}/child".write("remove")
  let recursive = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/rm.xsh" -- --progress -r $tree
  assert recursive.status.exited_with(0), recursive.stderr
  assert recursive.stderr == ""
  assert ! tree.exists()?

  let missing = fp"{root}/missing"
  let failed = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/rm.xsh" -- --progress $missing
  assert failed.status.exited_with(1)
  assert failed.stdout == ""
  assert "No such file or directory" in failed.stderr

  let verbose = fp"{root}/verbose"
  verbose.write("remove")
  let reported = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/rm.xsh" -- -gv $verbose
  assert reported.status.exited_with(0), reported.stderr
  assert verbose.basename() in reported.stdout
  assert reported.stderr == ""
}

test test_rm_symlinks_do_not_remove_referents { |ctx|
  let root = test.temp_dir(ctx, name: "rm-symlink")?
  let directory = fp"{root}/directory"
  directory.mkdir()
  fp"{directory}/content".write("retained")
  let link = fp"{root}/link"
  link.symlink(to: p"directory")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/rm.xsh" -- -r $link
  assert fs.stat(link) is Err(_)
  assert fp"{directory}/content".read_text()? == "retained"
  let broken = fp"{root}/broken"
  broken.symlink(to: p"missing")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/rm.xsh" -- $broken
  assert fs.stat(broken) is Err(_)
}

test test_rm_recursive_includes_hidden_files { |ctx|
  let root = test.temp_dir(ctx, name: "rm-hidden")?
  let target = fp"{root}/target"
  target.mkdir()
  fp"{target}/.hidden".write("hidden")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/rm.xsh" -- -r $target
  assert ! target.exists()?
}

test test_rm_continues_after_failure_and_protects_dot { |ctx|
  let root = test.temp_dir(ctx, name: "rm-errors")?
  let good = fp"{root}/good"
  good.write("remove")
  let missing = fp"{root}/missing"
  let failed = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/rm.xsh" -- $missing $good
  assert failed.status.exited_with(1)
  assert ! good.exists()?
  let dot = fp"{root}/."
  let refused = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/rm.xsh" -- -r $dot
  assert refused.status.exited_with(1)
  assert root.exists()?
}

test test_rm_mount_policy_on_single_device { |ctx|
  let root = test.temp_dir(ctx, name: "rm-mount-policy")?
  let target = fp"{root}/target"
  target.mkdir()
  fp"{target}/file".write("remove")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/rm.xsh" -- --one-file-system --preserve-root=all -r $target
  assert ! target.exists()?
}

test test_rm_interactive_order_and_declined_tree { |ctx|
  let root = test.temp_dir(ctx, name: "rm-interactive")?
  let file = fp"{root}/file"
  let input = test.temp_file(ctx, name: "answers", contents: b"n\n")?
  file.write("")
  let retained = run.capture --text LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/rm.xsh" -- -fi $file < $input
  assert retained.status.exited_with(0)
  assert file.exists()?
  assert retained.stderr == f"rm: remove regular empty file '{file}'? "
  let removed = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/rm.xsh" -- -if $file < $input
  assert removed.status.exited_with(0)
  assert removed.stderr == ""
  assert ! file.exists()?
  let directory = fp"{root}/directory"
  directory.mkdir()
  fp"{directory}/file".write("retain")
  let declined = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/rm.xsh" -- -ri $directory < $input
  assert declined.status.exited_with(0)
  assert fp"{directory}/file".exists()?
}

test test_rm_interactive_recursive_prompts { |ctx|
  let root = test.temp_dir(ctx, name: "rm-prompts")?
  let outer = fp"{root}/outer"
  let inner = fp"{outer}/inner"
  inner.mkdir()
  let input = test.temp_file(ctx, name: "answers", contents: b"y\ny\ny\n")?
  let removed = run.capture --text LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/rm.xsh" -- -ri $outer < $input
  assert removed.status.exited_with(0), removed.stderr
  assert removed.stderr == f"rm: descend into directory '{outer}'? rm: remove directory '{inner}'? rm: remove directory '{outer}'? "
  assert ! outer.exists()?
}

test test_rm_interactive_once_threshold { |ctx|
  let root = test.temp_dir(ctx, name: "rm-once")?
  let input = test.temp_file(ctx, name: "answers", contents: b"n\n")?
  let files = [fp"{root}/a", fp"{root}/b", fp"{root}/c", fp"{root}/d"]
  for file in files { file.write("") }
  let declined = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/rm.xsh" -- --interactive=once @files < $input
  assert declined.status.exited_with(0)
  assert declined.stderr == "rm: remove 4 arguments? "
  for file in files { assert file.exists()? }
  let removed = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/rm.xsh" -- -I ${files[0]} < $input
  assert removed.status.exited_with(0)
  assert removed.stderr == ""
  assert ! files[0].exists()?
}

test test_rm_posix_operands_do_not_override_interactive_mode { |ctx|
  let root = test.temp_dir(ctx, name: "rm-posix")?
  let file = fp"{root}/file"
  file.write("")
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/rm.xsh"
  let status = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "-i", file.display(), "-f"],
    root, {POSIXLY_CORRECT: "1", LC_ALL: "C"}, b"n\n", stdout, stderr))?
  assert status.exited_with(1)
  assert file.exists()?
  assert "remove regular empty file" in stderr.read_text()?
}

test test_rm_relative_names_and_stdout_failure { |ctx|
  let root = test.temp_dir(ctx, name: "rm-output")?
  let directory = fp"{root}/directory"
  directory.mkdir()
  fp"{directory}/file".write("remove")
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/rm.xsh"
  let prompted = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "-ri", "directory"],
    root, {LC_ALL: "C"}, b"y\ny\ny\n", stdout, stderr))?
  assert prompted.exited_with(0), stderr.read_text()?
  assert stderr.read_text()? == "rm: descend into directory 'directory'? rm: remove regular file 'directory/file'? rm: remove directory 'directory'? "
  directory.mkdir()
  fp"{directory}/file".write("remove")
  let broken = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "-rv", "directory///"],
    root, {LC_ALL: "C"}, b"", p"/dev/full", stderr))?
  assert broken.exited_with(1)
  assert ! directory.exists()?
  assert stderr.read_text()?.split("No space left on device").len() == 2
}

test test_rm_dash_filename_hint_and_option_errors { |ctx|
  let root = test.temp_dir(ctx, name: "rm-dash")?
  fp"{root}/-z".write("retain")
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/rm.xsh"
  let invalid = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "-z"],
    root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: "rm"}, b"", stdout, stderr))?
  assert invalid.exited_with(1)
  assert stderr.read_text()? == "rm: invalid option -- 'z'\nTry 'rm ./-z' to remove the file '-z'.\nTry 'rm --help' for more information.\n"
  assert fp"{root}/-z".exists()?
  let unsafe_abbreviation = run.capture --text ${ctx.xsh_bin} $script -- --no-pre $root
  assert unsafe_abbreviation.status.exited_with(1)
  assert "you may not abbreviate" in unsafe_abbreviation.stderr
}

test test_rm_effective_permissions_and_inaccessible_directory { |ctx|
  let root = test.temp_dir(ctx, name: "rm-permissions")?
  let file = fp"{root}/file"
  let no = test.temp_file(ctx, name: "no", contents: b"n\n")?
  let yes = test.temp_file(ctx, name: "yes", contents: b"y\n")?
  file.write("")
  file.chmod(0)
  let effective_root = unix.id()?.euid == 0
  let automatic = run.capture --text LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/rm.xsh" -- ---presume-input-tty $file < $no
  assert automatic.status.exited_with(0), automatic.stderr
  if effective_root {
    assert ! file.exists()?
    assert automatic.stderr == ""
  } else {
    assert file.exists()?
    assert automatic.stderr == f"rm: remove write-protected regular empty file '{file}'? "
    file.chmod(0o600)
  }
  let directory = fp"{root}/directory"
  directory.mkdir()
  directory.chmod(0)
  let removed = run.capture --text LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/rm.xsh" -- -di $directory < $yes
  assert removed.status.exited_with(0), removed.stderr
  let action = if effective_root { "remove directory" } else { "attempt removal of inaccessible directory" }
  assert removed.stderr == f"rm: {action} '{directory}'? "
  assert ! directory.exists()?
}

test test_rm_unreadable_empty_directory_and_first_failure { |ctx|
  let root = test.temp_dir(ctx, name: "rm-unreadable")?
  let outer = fp"{root}/outer"
  let empty = fp"{outer}/empty"
  empty.mkdir()
  empty.chmod(0)
  let removed = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/rm.xsh" -- -r $outer
  assert removed.status.exited_with(0), removed.stderr
  assert ! outer.exists()?
  outer.mkdir()
  let file = fp"{outer}/file"
  file.write("retained")
  outer.chmod(0o555)
  let failed = run.capture --text LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/rm.xsh" -- -rf $outer
  if unix.id()?.euid == 0 {
    assert failed.status.exited_with(0), failed.stderr
    assert ! outer.exists()?
  } else {
    assert failed.status.exited_with(1)
    assert failed.stderr == f"rm: cannot remove '{file}': Permission denied\n"
    assert file.exists()?
    outer.chmod(0o755)
  }
}

# 5000 levels is far past PATH_MAX and past RLIMIT_NOFILE for one descriptor
# per level: the tree is built through nested rooted capabilities because no
# path to the bottom exists.
test test_rm_recursive_removes_hierarchy_deeper_than_path_max { |ctx|
  let base = test.temp_dir(ctx, name: "rm-deep")?
  let levels = ["a" for _ in range(1000)]
  let chain = fp"{levels.join("/")}"
  var top = fs.open_root(base)?
  top.mkdir(p"deep")?
  var floor = top.open_root(p"deep")?
  for _ in range(5) {
    floor.mkdir(chain, parents: true)?
    floor = floor.open_root(chain)?
  }
  floor.write(p"leaf", "bottom")?
  let deep = fp"{base}/deep"
  let removed = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/rm.xsh" -- -rf $deep
  assert removed.status.exited_with(0), removed.stderr
  assert removed.stdout == ""
  assert removed.stderr == ""
  assert ! deep.exists()?
}
