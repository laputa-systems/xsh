test test_sort_unique_reverse { |ctx|
  let input = test.temp_file(ctx, name: "sort.txt", contents: b"b\na\nb\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/sort.xsh" -- -u -r $input
  let output_lines = output.lines().collect()
  assert output_lines[0] == "b"
  assert output_lines[1] == "a"
  let keyed = test.temp_file(ctx, name: "keyed.txt", contents: b"b,20\na,3\nc,1\n")?
  let by_second = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/sort.xsh" -- -t, -k2 -n $keyed
  let by_second_lines = by_second.lines().collect()
  assert by_second_lines[0] == "c,1"
  assert by_second_lines[2] == "b,20"
}

test test_sort_checks_all_input_paths_before_opening_fifo { |ctx|
  let root = test.temp_dir(ctx, name: "sort-input-check")?
  let fifo = fp"{root}/FIFO"
  let missing = fp"{root}/missing"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  fs.mkfifo(fifo, mode: 0o600)?
  let plan = process.command_argv(
    ctx.xsh_bin,
    [ctx.xsh_bin.display(), fp"{ctx.core_dir}/sort.xsh".display(), fifo.display(), missing.display()],
    root,
    {LC_ALL: "C"},
    b"",
    stdout,
    stderr,
    timeout: 2s,
  )
  let status = process.run(plan)?

  assert status.exited_with(2), stderr.read_text()?
  assert stderr.read_text()? == f"sort: cannot read: {missing}: No such file or directory\n"
  assert stdout.read_bytes()?.is_empty()
}

test test_sort_check_reports_first_disorder_and_silent_mode { |ctx|
  let root = test.temp_dir(ctx, name: "sort-check")?
  let input = test.temp_file(ctx, name: "input", contents: b"1\n3\n2\n0\n")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let failed = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "--check=diagnose-first", input.display()], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert failed.exit_code()? == 1
  assert stderr.read_text()? == f"sort: {input}:3: disorder: 2\n"
  assert stdout.read_bytes()?.is_empty()

  let quiet_out = fp"{root}/quiet-stdout"
  let quiet_err = fp"{root}/quiet-stderr"
  let quiet = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-C", input.display()], root,
    {LC_ALL: "C"}, b"", quiet_out, quiet_err))?
  assert quiet.exit_code()? == 1
  assert quiet_err.read_bytes()?.is_empty()
  assert quiet_out.read_bytes()?.is_empty()

  let sorted_input = test.temp_file(ctx, name: "sorted-input", contents: b"1\n2\n3\n")?
  let sorted = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-c", sorted_input.display()], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert sorted.exit_code()? == 0
  assert stderr.read_bytes()?.is_empty()
  assert stdout.read_bytes()?.is_empty()

}

test test_sort_check_conflicting_modes_match_gnu { |ctx|
  let root = test.temp_dir(ctx, name: "sort-check-conflict")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let result = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-c", "-C"], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert result.exit_code()? == 2
  assert stderr.read_text()? == "sort: options '-cC' are incompatible\n"
  assert stdout.read_bytes()?.is_empty()

  let long_check = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "--check", "-C"], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert long_check.exit_code()? == 2
  assert stderr.read_text()? == "sort: options '-cC' are incompatible\n"
}

test test_sort_check_zero_terminated_records { |ctx|
  let root = test.temp_dir(ctx, name: "sort-check-zero")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let input = test.temp_file(ctx, name: "input", contents: b"1\x002\x000\x00")?
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let failed = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-z", "-c", input.display()], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert failed.exit_code()? == 1
  assert stderr.read_bytes()? == bytes.from_text(f"sort: {input}:3: disorder: 0\0")
  assert stdout.read_bytes()?.is_empty()

  let sorted_input = test.temp_file(ctx, name: "sorted-input", contents: b"1\x002\x003\x00")?
  let sorted = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-z", "-c", sorted_input.display()], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert sorted.exit_code()? == 0
  assert stderr.read_bytes()?.is_empty()
}

test test_sort_zero_terminated_output { |ctx|
  let root = test.temp_dir(ctx, name: "sort-zero-output")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let input = test.temp_file(ctx, name: "input", contents: b"b\x00a\x00")?
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let result = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-z", input.display()], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert result.exit_code()? == 0
  assert stdout.read_bytes()? == b"a\x00b\x00"
  assert stderr.read_bytes()?.is_empty()
}

test test_sort_keeps_unterminated_input_files_separate { |ctx|
  let root = test.temp_dir(ctx, name: "sort-input-boundaries")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let first = test.temp_file(ctx, name: "first", contents: b"a\nb")?
  let second = test.temp_file(ctx, name: "second", contents: b"b")?
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let result = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), first.display(), second.display()], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert result.exit_code()? == 0
  assert stdout.read_bytes()? == b"a\nb\nb\n"
  assert stderr.read_bytes()?.is_empty()
}

test test_sort_ignores_leading_blanks_before_raw_tie_break { |ctx|
  let root = test.temp_dir(ctx, name: "sort-leading-blanks")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let input = test.temp_file(ctx, name: "input", contents: b" z\na\nx\n x\n")?
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let result = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-b", input.display()], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert result.exit_code()? == 0
  assert stdout.read_bytes()? == b"a\n x\nx\n z\n"
  assert stderr.read_bytes()?.is_empty()

  let unsorted = test.temp_file(ctx, name: "unsorted", contents: b" z\na\n")?
  let checked = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-b", "-c", unsorted.display()], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert checked.exit_code()? == 1
  assert stderr.read_text()? == f"sort: {unsorted}:2: disorder: a\n"
  assert stdout.read_bytes()?.is_empty()
}

test test_sort_version_option { |ctx|
  let root = test.temp_dir(ctx, name: "sort-version")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let result = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "--version"], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert result.exit_code()? == 0
  assert "sort" in stdout.read_text()?
  assert stderr.read_bytes()?.is_empty()
}
