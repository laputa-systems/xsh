test test_install_follows_source_and_replaces_destination_link { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  source.write("new")
  let source_link = fp"{root}/source-link"
  source_link.symlink(to: source)
  let old = fp"{root}/old"
  old.write("keep")
  let dest = fp"{root}/dest"
  dest.symlink(to: old)
  fs.set_times(source, atime_ns: 1230000000, mtime_ns: 4560000000)
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -pm 640 $source_link $dest
  assert dest.read_text()? == "new"
  assert old.read_text()? == "keep"
  assert fs.stat(dest)?.kind == "file"
  assert fs.stat(dest)?.mode.bit_and(0o7777) == 0o640
  assert fs.stat(dest)?.mtime_ns == 4560000000
}

test test_install_directory_symbolic_mode_and_parents { |ctx|
  let root = test.temp_dir(ctx)?
  let dest = fp"{root}/a/b"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -dm u=rwx,g=rx,o= $dest
  assert dest.is_dir()?
  assert fs.stat(dest)?.mode.bit_and(0o7777) == 0o750
  let source = fp"{root}/source"
  source.write("payload")
  let file = fp"{root}/c/d/file"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -D $source $file
  assert file.read_text()? == "payload"
  assert fs.stat(file)?.mode.bit_and(0o7777) == 0o755
}

test test_install_intermediate_directories_keep_default_mode { |ctx|
  let root = test.temp_dir(ctx)?
  let script = fp"{ctx.core_dir}/install.xsh"
  # The umask is set in a child shell so the expected mode does not depend on
  # the umask of the test runner. Setgid is excluded because new directories
  # inherit it from a setgid parent.
  let default_mode = 0o777.clear_bits(0o002)
  let dest = fp"{root}/a/b/c"
  let directory_command = process.command_argv(p"/bin/sh",
    ["sh", "-c", "umask 002; exec \"$@\"", "sh", ctx.xsh_bin.display(), script.display(), "--mode=200", "-d", dest.display()],
    root, {}, b"", fp"{root}/out", fp"{root}/err")
  assert process.run(directory_command)?.exited_with(0)
  assert fs.stat(fp"{root}/a")?.mode.bit_and(0o777) == default_mode
  assert fs.stat(fp"{root}/a/b")?.mode.bit_and(0o777) == default_mode
  assert fs.stat(dest)?.mode.bit_and(0o7777) == 0o200

  let source = fp"{root}/source"
  source.write("payload")
  let file = fp"{root}/d/e/file"
  let file_command = process.command_argv(p"/bin/sh",
    ["sh", "-c", "umask 002; exec \"$@\"", "sh", ctx.xsh_bin.display(), script.display(), "--mode=200", "-D", source.display(), file.display()],
    root, {}, b"", fp"{root}/out", fp"{root}/err")
  assert process.run(file_command)?.exited_with(0)
  assert fs.stat(fp"{root}/d")?.mode.bit_and(0o777) == default_mode
  assert fs.stat(fp"{root}/d/e")?.mode.bit_and(0o777) == default_mode
  assert fs.stat(file)?.mode.bit_and(0o7777) == 0o200
}

test test_install_failed_directory_chown_clears_inherited_setgid { |ctx|
  if user.current()?.uid == 0 { test.skip("requires an unprivileged test process"); return }
  let root = test.temp_dir(ctx)?
  let parent = fp"{root}/parent"
  parent.mkdir()
  parent.chmod(0o2777)
  let dir = fp"{parent}/newdir"
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -d -m 4755 -o root $dir
  assert result.status.exited_with(1)
  assert dir.is_dir()?
  assert fs.stat(dir)?.mode.bit_and(0o7777) == 0o700
}

test test_install_compare_preserves_inode_and_backup_keeps_previous { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  source.write("payload")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- $source $dest
  let ino = fs.stat(dest)?.ino
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -C $source $dest
  assert fs.stat(dest)?.ino == ino
  source.write("changed")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -b $source $dest
  assert fp"{dest}~".read_text()? == "payload"
  assert dest.read_text()? == "changed"
}

test test_install_backup_cannot_destroy_source { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/file~"
  let dest = fp"{root}/file"
  source.write("source")
  dest.write("destination")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -b $source $dest
  assert status.exited_with(1)
  assert source.read_text()? == "source"
  assert dest.read_text()? == "destination"
}

test test_install_null_device_and_conditional_execute_mode { |ctx|
  let dest = test.temp_path(ctx)
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -m ug+rwX,o+rX /dev/null $dest
  assert dest.read_bytes()? == b""
  assert fs.stat(dest)?.mode.bit_and(0o7777) == 0o664
}

test test_install_directory_continues_after_existing_file { |ctx|
  let root = test.temp_dir(ctx)?
  let existing = fp"{root}/existing"
  existing.write("keep")
  let later = fp"{root}/later"
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -d $existing $later
  assert status.exited_with(1)
  assert existing.read_text()? == "keep"
  assert later.is_dir()?
}

test test_install_compare_checks_bytes_after_first_chunk { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  let chunks = ["x"] |> repeat(65536)
  let prefix = chunks.join("")
  source.write(f"{prefix}a")
  dest.write(f"{prefix}b")
  dest.chmod(0o755)
  let old = fs.stat(dest)?.ino
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -C $source $dest
  assert fs.stat(dest)?.ino != old
  assert dest.read_text()? == f"{prefix}a"
}

test test_install_parallel_publications_do_not_share_staging_names { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  source.write("payload")
  let statuses = range(8) |> par-map(jobs: 4) { |index|
    let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -D $source fp"{root}/shared/child/file-{index}"
    status.exited_with(0)
  }
  for succeeded in statuses { assert succeeded }
  for index in range(8) { assert fp"{root}/shared/child/file-{index}".read_text()? == "payload" }
}

test test_install_directory_trailing_dot_creates_directory { |ctx|
  let root = test.temp_dir(ctx)?
  let directory = fp"{root}/directory/."
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -d $directory
  assert fp"{root}/directory".is_dir()?
}

test test_install_missing_directory_target_does_not_create_it { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  source.write("payload")
  let target = fp"{root}/missing/"
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -D $source $target
  assert status.exited_with(1)
  assert ! target.exists()?
  assert source.read_text()? == "payload"
}

test test_install_compare_copies_fifo_source_symlink_in_bounded_chunks { |ctx|
  let root = test.temp_dir(ctx)?
  let fifo = fp"{root}/fifo"
  fs.mkfifo(fifo, 0o600)
  let source = fp"{root}/source"
  source.symlink(to: fifo)
  let payload = fp"{root}/payload"
  let chunks = ["x"] |> repeat(131072)
  let expected = bytes.from_text(chunks.join("") + "tail")
  payload.write(expected)
  let writer_script = fp"{root}/writer.xsh"
  writer_script.write(f"fp\"{fifo}\".write(fp\"{payload}\".read_bytes()?)\n")
  let writer = spawn run ${ctx.xsh_bin} $writer_script ?
  defer writer.cancel(kill_after: 100ms)
  let dest = fp"{root}/dest"
  dest.write(b"")
  dest.chmod(0o755)
  let script = fp"{ctx.core_dir}/install.xsh"
  let reader_command = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "-C", source.display(), dest.display()],
    root, {}, b"", fp"{root}/out", fp"{root}/err", timeout: 3s)
  let status = process.run(reader_command)?
  assert status.exited_with(0)
  assert (wait writer?).exited_with(0)
  assert dest.read_bytes()? == expected
  assert fs.stat(dest)?.kind == "file"
}

test test_install_reads_stdin_descriptor_alias { |ctx|
  let root = test.temp_dir(ctx)?
  let script = fp"{ctx.core_dir}/install.xsh"
  let dest = fp"{root}/dest"
  let payload = b"stream\0payload\n"
  let command = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "-m", "640", "/dev/fd/0", dest.display()],
    root, {}, payload, fp"{root}/out", fp"{root}/err")
  assert process.run(command)?.exited_with(0)
  assert dest.read_bytes()? == payload
  assert fs.stat(dest)?.mode.bit_and(0o7777) == 0o640
}

test test_install_source_symlink_cannot_replace_its_own_target { |ctx|
  let root = test.temp_dir(ctx)?
  let target = fp"{root}/target"
  target.write("keep")
  let source = fp"{root}/source"
  source.symlink(to: target)
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- $source $target
  assert status.exited_with(1)
  assert target.read_text()? == "keep"
  assert source.readlink()? == target
}

test test_install_compare_and_strip_reports_mutual_exclusion { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  let target = fp"{root}/target"
  source.write("payload")
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -C --strip --strip-program=echo $source $target
  assert result.status.exited_with(1)
  assert result.stderr == "install: options --compare (-C) and --strip are mutually exclusive\nTry 'install --help' for more information.\n"
}

test test_install_invalid_octal_mode_names_invalid_mode { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  source.write("payload")
  let target = fp"{root}/target"
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -m 999 $source $target
  assert result.status.exited_with(1)
  assert result.stderr == "install: invalid mode '999'\n"
  assert ! target.exists()?
}

test test_install_invalid_symbolic_mode_names_invalid_mode { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  source.write("payload")
  let target = fp"{root}/target"
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -m "u+rw?x" $source $target
  assert result.status.exited_with(1)
  assert result.stderr == "install: invalid mode 'u+rw?x'\n"
  assert ! target.exists()?
}

test test_install_failed_replacement_names_target_and_preserves_it { |ctx|
  if user.current()?.uid == 0 { test.skip("requires an unprivileged test process"); return }
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  source.write("new")
  let directory = fp"{root}/protected"
  directory.mkdir()
  let target = fp"{directory}/target"
  target.write("old")
  directory.chmod(0o555)
  defer directory.chmod(0o755)

  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- $source $target
  assert result.status.exited_with(1)
  assert f"cannot remove '{target}'" in result.stderr
  assert target.read_text()? == "old"
  assert source.read_text()? == "new"
}

test test_install_accepts_non_utf8_target_directory { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  let directory = Path.parse_bytes(bytes.concat([root.bytes(), b"/target_dir_\xff\xfe"]))?
  source.write("payload")
  directory.mkdir()
  let script = fp"{ctx.core_dir}/install.xsh"
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let command = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin, script, source, "--target-directory", directory], root, {}, b"", out, err)

  assert process.run(command)?.exited_with(0)
  assert fp"{directory}/source".read_text()? == "payload"
}

test test_install_accepts_non_utf8_source_and_destination_paths { |ctx|
  let root = test.temp_dir(ctx)?
  let source = Path.parse_bytes(bytes.concat([root.bytes(), b"/source\xff\xfe"]))?
  let directory = fp"{root}/target_dir"
  let installed = Path.parse_bytes(bytes.concat([directory.bytes(), b"/source\xff\xfe"]))?
  source.write("payload")
  directory.mkdir()
  let script = fp"{ctx.core_dir}/install.xsh"
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let source_command = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin, script, source, directory], root, {}, b"", out, err)

  assert process.run(source_command)?.exited_with(0)
  assert installed.read_text()? == "payload"

  let plain_source = fp"{root}/plain_source"
  let target_directory = Path.parse_bytes(bytes.concat([root.bytes(), b"/install_\xff\xfe"]))?
  let target = Path.parse_bytes(bytes.concat([target_directory.bytes(), b"/target.txt"]))?
  plain_source.write("other")
  let parents_command = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin, script, "-D", plain_source, target], root, {}, b"", out, err)

  assert process.run(parents_command)?.exited_with(0)
  assert target.read_text()? == "other"
}

test test_install_missing_target_directory_value_has_usage_error { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  source.write("payload")
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -T $source -t
  assert result.status.exited_with(1)
  assert result.stderr == "install: option requires an argument -- 't'\nTry 'install --help' for more information.\n"
}

test test_install_parents_reports_long_component_as_directory_creation { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  source.write("payload")
  let long_component = ["d"] |> repeat(4097) |> join("")
  let target = fp"{root}/{long_component}/target"
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -D $source $target
  assert result.status.exited_with(1)
  assert "cannot create directory" in result.stderr
  assert source.read_text()? == "payload"
}

test test_install_strip_runs_program_before_publishing { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  source.write("payload")
  let strip = fp"{root}/strip"
  strip.write("#!/bin/sh\n: > \"$1\"\n", mode: 0o755)
  let target = fp"{root}/target"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -s --strip-program $strip $source $target
  assert target.read_bytes()? == b""
  assert source.read_text()? == "payload"
}

test test_install_strip_failure_does_not_publish { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  source.write("payload")
  let strip = fp"{root}/strip"
  strip.write("#!/bin/sh\nexit 1\n", mode: 0o755)
  let target = fp"{root}/target"
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -s --strip-program $strip $source $target
  assert result.status.exited_with(1)
  assert "strip process terminated abnormally" in result.stderr
  assert ! target.exists()?
  assert source.read_text()? == "payload"
}

test test_install_missing_strip_program_reports_lookup_failure { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  source.write("payload")
  let strip = fp"{root}/missing-strip"
  let target = fp"{root}/target"
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -s --strip-program $strip $source $target
  assert result.status.exited_with(1)
  assert result.stderr == f"install: cannot run strip program '{strip}': No such file or directory\n"
  assert ! target.exists()?
}

test test_install_does_not_create_missing_parent_without_parents_option { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  source.write("payload")
  let parent = fp"{root}/missing"
  let target = fp"{parent}/target"
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- $source $target
  assert result.status.exited_with(1)
  assert ! parent.exists()?
  assert source.read_text()? == "payload"
}

test test_install_target_directory_requires_a_directory { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  let target = fp"{root}/target"
  source.write("new")
  target.write("old")
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -t $target $source
  assert result.status.exited_with(1)
  assert result.stderr == f"install: failed to access '{target}': Not a directory\n"
  assert target.read_text()? == "old"
}

test test_install_no_target_directory_rejects_multiple_sources { |ctx|
  let root = test.temp_dir(ctx)?
  let first = fp"{root}/first"
  let second = fp"{root}/second"
  let dest = fp"{root}/dest"
  first.write("first")
  second.write("second")
  dest.mkdir()
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -T $first $second $dest
  assert result.status.exited_with(1)
  assert "extra operand" in result.stderr
  assert "Try 'install --help' for more information." in result.stderr
  assert first.exists()? and second.exists()?
}

test test_install_rejects_unprivileged_extension { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  let target = fp"{root}/target"
  source.write("payload")
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -U --owner=123 $source $target
  assert result.status.exited_with(1)
  assert result.stderr == "install: invalid option -- 'U'\nTry 'install --help' for more information.\n"
  assert ! target.exists()?
}

test test_install_backup_failure_sets_failure_status { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  let target = fp"{root}/target"
  source.write("new")
  target.write("old")
  fp"{target}.backup".mkdir()

  let result = run.capture --text SIMPLE_BACKUP_SUFFIX=.backup ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- --backup $source $target
  assert result.status.exited_with(1)
  assert result.stderr == f"install: cannot backup '{target}': Is a directory\n"
  assert source.read_text()? == "new"
  assert target.read_text()? == "old"
}

test test_install_target_and_no_target_directory_are_mutually_exclusive { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  source.write("payload")
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -T -t $root $source
  assert result.status.exited_with(1)
  assert result.stderr == "install: cannot combine --target-directory (-t) and --no-target-directory (-T)\n"
}

test test_install_context_flags_are_silent_without_selinux { |ctx|
  let root = test.temp_dir(ctx)?
  let script = fp"{ctx.core_dir}/install.xsh"
  for flag in ["-Z", "--context", "--cont"] {
    let source = fp"{root}/source"
    let dest = fp"{root}/dest"
    source.write("data")
    let result = run.capture --text ${ctx.xsh_bin} $script -- $flag $source $dest
    assert result.status.exited_with(0), f"install {flag}: {result.stderr}"
    assert result.stdout == "" and result.stderr == ""
    assert dest.read_text()? == "data"
    dest.remove()?
  }
}

test test_install_context_value_warns_and_continues { |ctx|
  let root = test.temp_dir(ctx)?
  let script = fp"{ctx.core_dir}/install.xsh"
  let source = fp"{root}/source"
  source.write("data")
  let dest = fp"{root}/dest"
  let result = run.capture --text ${ctx.xsh_bin} $script -- --context=unconfined_u:object_r:user_tmp_t:s0 $source $dest
  assert result.status.exited_with(0)
  assert result.stdout == ""
  assert result.stderr == "install: warning: ignoring --context; it requires an SELinux-enabled kernel\n"
  assert dest.read_text()? == "data"
}

test test_install_empty_context_value_still_warns { |ctx|
  let root = test.temp_dir(ctx)?
  let script = fp"{ctx.core_dir}/install.xsh"
  let source = fp"{root}/source"
  source.write("data")
  let dest = fp"{root}/dest"
  let result = run.capture --text ${ctx.xsh_bin} $script -- --context= $source $dest
  assert result.status.exited_with(0)
  assert result.stderr == "install: warning: ignoring --context; it requires an SELinux-enabled kernel\n"
  assert dest.read_text()? == "data"
}

test test_install_each_context_value_warns_per_occurrence { |ctx|
  let root = test.temp_dir(ctx)?
  let script = fp"{ctx.core_dir}/install.xsh"
  let source = fp"{root}/source"
  source.write("data")
  let dest = fp"{root}/dest"
  let result = run.capture --text ${ctx.xsh_bin} $script -- --context=a -Z --context --context=b $source $dest
  assert result.status.exited_with(0)
  let warning = "install: warning: ignoring --context; it requires an SELinux-enabled kernel\n"
  assert result.stderr == warning + warning
}

test test_install_preserve_context_warns_and_continues { |ctx|
  let root = test.temp_dir(ctx)?
  let script = fp"{ctx.core_dir}/install.xsh"
  let source = fp"{root}/source"
  source.write("data")
  let dest = fp"{root}/dest"
  let result = run.capture --text ${ctx.xsh_bin} $script -- --preserve-context $source $dest
  assert result.status.exited_with(0)
  assert result.stderr == "install: WARNING: ignoring --preserve-context; this kernel is not SELinux-enabled\n"
  assert dest.read_text()? == "data"
}

test test_install_context_warning_precedes_missing_operand_error { |ctx|
  let root = test.temp_dir(ctx)?
  let script = fp"{ctx.core_dir}/install.xsh"
  let result = run.capture --text ${ctx.xsh_bin} $script -- --context=x
  assert result.status.exited_with(1)
  assert result.stderr == "install: warning: ignoring --context; it requires an SELinux-enabled kernel\ninstall: missing file operand\nTry 'install --help' for more information.\n"
}

test test_install_help_before_context_suppresses_warning { |ctx|
  let script = fp"{ctx.core_dir}/install.xsh"
  let result = run.capture --text ${ctx.xsh_bin} $script -- --help --context=x
  assert result.status.exited_with(0)
  assert "Usage: install" in result.stdout
  assert result.stderr == ""
}

test test_install_preserves_timestamps_from_before_source_read { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  let target = fp"{root}/target"
  source.write("payload")
  fs.set_times(source, atime_ns: 1230000000, mtime_ns: 4560000000)
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -p $source $target
  let installed = fs.stat(target)?
  assert installed.atime_ns == 1230000000
  assert installed.mtime_ns == 4560000000
}

test test_install_does_not_trim_attached_mode_whitespace { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  let target = fp"{root}/target"
  source.write("payload")
  for mode in ["-m 0333", "--mode=755 "] {
    let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- $mode $source $target
    assert result.status.exited_with(1)
    assert "invalid mode" in result.stderr
    assert ! target.exists()?
  }
}

test test_install_failed_file_chown_publishes_safe_permissions { |ctx|
  if user.current()?.uid == 0 { test.skip("requires an unprivileged test process"); return }
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  let target = fp"{root}/target"
  source.write("new")
  target.write("old")
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -m 4755 -o root $source $target
  assert result.status.exited_with(1)
  assert result.stderr == f"install: cannot change ownership of '{target}': Operation not permitted\n"
  assert target.read_text()? == "new"
  assert fs.stat(target)?.mode.bit_and(0o7777) == 0o600
}
