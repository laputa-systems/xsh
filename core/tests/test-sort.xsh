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

test test_sort_fold_case_uses_raw_line_as_tie_breaker { |ctx|
  let root = test.temp_dir(ctx, name: "sort-fold-case-tie")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let input = b"A\na\n_\n"
  let result = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-f"], root,
    {LC_ALL: "C"}, input, stdout, stderr))?
  assert result.exit_code()? == 0
  assert stdout.read_bytes()? == b"A\na\n_\n"
  assert stderr.read_bytes()?.is_empty()

  let unique_input = b"a\n_\n"
  let unique = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-fu"], root,
    {LC_ALL: "C"}, unique_input, stdout, stderr))?
  assert unique.exit_code()? == 0
  assert stdout.read_bytes()? == b"a\n_\n"
  assert stderr.read_bytes()?.is_empty()
}

test test_sort_dictionary_and_nonprinting_order { |ctx|
  let root = test.temp_dir(ctx, name: "sort-character-order")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let dictionary_input = b"./bbc\nbbd\nbbb\n"
  let dictionary = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-d"], root,
    {LC_ALL: "C"}, dictionary_input, stdout, stderr))?
  assert dictionary.exit_code()? == 0
  assert stdout.read_bytes()? == b"bbb\n./bbc\nbbd\n"
  assert stderr.read_bytes()?.is_empty()

  let nonprinting_input = bytes.from_text("a👦🏻aa\naaaa\n")
  let nonprinting = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-i"], root,
    {LC_ALL: "C"}, nonprinting_input, stdout, stderr))?
  assert nonprinting.exit_code()? == 0
  assert stdout.read_bytes()? == nonprinting_input
  assert stderr.read_bytes()?.is_empty()
}

test test_sort_dictionary_and_nonprinting_conflict_with_numeric { |ctx|
  let root = test.temp_dir(ctx, name: "sort-character-order-conflict")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  for args in [["-dn"], ["-in"]] {
    let result = process.run(process.command_argv(ctx.xsh_bin,
      [ctx.xsh_bin.display(), "--", script.display()] + args, root,
      {LC_ALL: "C"}, b"", stdout, stderr))?
    assert result.exit_code()? == 2
    assert stderr.read_text()? == f"sort: options '{args[0]}' are incompatible\n"
    assert stdout.read_bytes()?.is_empty()
  }
}

test test_sort_numeric_rejects_leading_plus_and_uses_line_tie_break { |ctx|
  let root = test.temp_dir(ctx, name: "sort-numeric-plus")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let result = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-n"], root,
    {LC_ALL: "C"}, b"+2\n+1\n+10\n", stdout, stderr))?
  assert result.exit_code()? == 0
  assert stdout.read_bytes()? == b"+1\n+10\n+2\n"
  assert stderr.read_bytes()?.is_empty()
}

test test_sort_stable_preserves_equal_primary_keys { |ctx|
  let root = test.temp_dir(ctx, name: "sort-stable-tie")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let numeric = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-ns"], root,
    {LC_ALL: "C"}, b"1\n01\n", stdout, stderr))?
  assert numeric.exit_code()? == 0
  assert stdout.read_bytes()? == b"1\n01\n"
  assert stderr.read_bytes()?.is_empty()

  let folded = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-fs"], root,
    {LC_ALL: "C"}, b"a\nA\n", stdout, stderr))?
  assert folded.exit_code()? == 0
  assert stdout.read_bytes()? == b"a\nA\n"
  assert stderr.read_bytes()?.is_empty()
}

test test_sort_version_order_and_stability { |ctx|
  let root = test.temp_dir(ctx, name: "sort-version-order")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let input = b"0.1\n0.02\n0.2\n0.002\n0.3\n"
  let sorted = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-V"], root,
    {LC_ALL: "C"}, input, stdout, stderr))?
  assert sorted.exit_code()? == 0
  assert stdout.read_bytes()? == b"0.1\n0.002\n0.02\n0.2\n0.3\n"
  assert stderr.read_bytes()?.is_empty()

  let mode = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "--sort=version"], root,
    {LC_ALL: "C"}, input, stdout, stderr))?
  assert mode.exit_code()? == 0
  assert stdout.read_bytes()? == b"0.1\n0.002\n0.02\n0.2\n0.3\n"
  assert stderr.read_bytes()?.is_empty()

  let stable = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-V", "--stable"], root,
    {LC_ALL: "C"}, input, stdout, stderr))?
  assert stable.exit_code()? == 0
  assert stdout.read_bytes()? == input
  assert stderr.read_bytes()?.is_empty()

  let leading_dots = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-V"], root,
    {LC_ALL: "C"}, b".\n..\n.a\na\n0\n", stdout, stderr))?
  assert leading_dots.exit_code()? == 0
  assert stdout.read_bytes()? == b".\n..\n.a\n0\na\n"
  assert stderr.read_bytes()?.is_empty()
}

test test_sort_debug_annotates_ordering_keys { |ctx|
  let root = test.temp_dir(ctx, name: "sort-debug-keys")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let plain = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "--debug"], root,
    {LC_ALL: "C"}, b"b\na\n", stdout, stderr))?
  assert plain.exit_code()? == 0
  assert stdout.read_bytes()? == b"a\n_\nb\n_\n"
  assert stderr.read_bytes()?.is_empty()

  let folded = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-f", "--debug"], root,
    {LC_ALL: "C"}, b"a\nA\n", stdout, stderr))?
  assert folded.exit_code()? == 0
  assert stdout.read_bytes()? == b"A\n_\n_\na\n_\n_\n"
  assert stderr.read_bytes()?.is_empty()

  let version = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-V", "--debug"], root,
    {LC_ALL: "C"}, b"\n\t\t\t1.12.4\n", stdout, stderr))?
  assert version.exit_code()? == 0
  assert stdout.read_bytes()? == b"\n^ no match for key\n^ no match for key\n>>>1.12.4\n_________\n_________\n"
  assert stderr.read_bytes()?.is_empty()
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

test test_sort_output_alias_and_duplicate_rules { |ctx|
  let root = test.temp_dir(ctx, name: "sort-output-options")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let input = test.temp_file(ctx, name: "input", contents: b"b\na\n")?
  let alias_output = fp"{root}/alias-output"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"

  let via_alias = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "--output", alias_output.display(), input.display()], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert via_alias.exit_code()? == 0
  assert stdout.read_bytes()?.is_empty()
  assert alias_output.read_bytes()? == b"a\nb\n"
  assert stderr.read_bytes()?.is_empty()

  let long_dash = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "--output", "--dash-file", input.display()], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert long_dash.exit_code()? == 0
  assert fp"{root}/--dash-file".read_bytes()? == b"a\nb\n"

  let short_dash = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-o", "--another-dash-file", input.display()], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert short_dash.exit_code()? == 0
  assert fp"{root}/--another-dash-file".read_bytes()? == b"a\nb\n"

  let same_output = fp"{root}/same-output"
  let repeated = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-o", same_output.display(), "-o", same_output.display(), input.display()], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert repeated.exit_code()? == 0
  assert same_output.read_bytes()? == b"a\nb\n"

  let first_output = fp"{root}/first-output"
  let second_output = fp"{root}/second-output"
  let conflicting = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-o", first_output.display(), "-o", second_output.display(), input.display()], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert conflicting.exit_code()? == 2
  assert stderr.read_text()? == "sort: multiple output files specified\n"
}

test test_sort_reports_output_open_failure { |ctx|
  let root = test.temp_dir(ctx, name: "sort-output-open-error")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let output = fp"{root}/missing-directory/output"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let result = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-o", output.display()], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert result.exit_code()? == 2
  assert stderr.read_text()? == f"sort: open failed: {output}: No such file or directory\n"
  assert stdout.read_bytes()?.is_empty()
}
