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

test test_sort_general_numeric_exponents_and_extremes { |ctx|
  let root = test.temp_dir(ctx, name: "sort-general-numeric")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let input = b"1\n2e3\n1e-5\n0\n-1.7976931348623157e+308\n1.7976931348623157e+308\nNaN\ninf\n-inf\n64e+\n10E\n"
  let result = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-g"], root,
    {LC_ALL: "C"}, input, stdout, stderr))?
  assert result.exit_code()? == 0
  assert stdout.read_bytes()? == b"NaN\n-inf\n-1.7976931348623157e+308\n0\n1e-5\n1\n10E\n64e+\n2e3\n1.7976931348623157e+308\ninf\n"
  assert stderr.read_bytes()?.is_empty()
}

test test_sort_general_numeric_stable_equal_values { |ctx|
  let root = test.temp_dir(ctx, name: "sort-general-numeric-stable")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let input = b"1.0\n1\n1e0\n"
  let result = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-gs"], root,
    {LC_ALL: "C"}, input, stdout, stderr))?
  assert result.exit_code()? == 0
  assert stdout.read_bytes()? == input
  assert stderr.read_bytes()?.is_empty()
}

test test_sort_general_numeric_invalid_values_precede_nan_and_numbers { |ctx|
  let root = test.temp_dir(ctx, name: "sort-general-invalid")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let result = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-g"], root,
    {LC_ALL: "C"}, b"\nword\nNaN\n-inf\n-2\n0\n", stdout, stderr))?
  assert result.exit_code()? == 0
  assert stdout.read_bytes()? == b"\nword\nNaN\n-inf\n-2\n0\n"
  assert stderr.read_bytes()?.is_empty()
}

test test_sort_month_order_stability_and_uniqueness { |ctx|
  let root = test.temp_dir(ctx, name: "sort-month-mode")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let input = b"JAn\nMAY\n000may\nJun\nFeb\nJul 2\nJul 1\nJul 3\n asdf\n"
  for args in [["-M"], ["--month-sort"], ["--sort=month"], ["--sort=mont"], ["--sort=m"]] {
    let result = process.run(process.command_argv(ctx.xsh_bin,
      [ctx.xsh_bin.display(), "--", script.display()] + args, root,
      {LC_ALL: "C"}, input, stdout, stderr))?
    assert result.exit_code()? == 0
    assert stdout.read_bytes()? == b" asdf\n000may\nJAn\nFeb\nMAY\nJun\nJul 1\nJul 2\nJul 3\n"
    assert stderr.read_bytes()?.is_empty()
  }

  let stable = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-Ms"], root,
    {LC_ALL: "C"}, b"Jul 2\nJul 1\nJul 3\n", stdout, stderr))?
  assert stable.exit_code()? == 0
  assert stdout.read_bytes()? == b"Jul 2\nJul 1\nJul 3\n"
  assert stderr.read_bytes()?.is_empty()

  let unique = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-Mu"], root,
    {LC_ALL: "C"}, b"JUNNNN\n\nAPR\nMAY\nJUN\nAUG\n", stdout, stderr))?
  assert unique.exit_code()? == 0
  assert stdout.read_bytes()? == b"\nAPR\nMAY\nJUNNNN\nAUG\n"
  assert stderr.read_bytes()?.is_empty()

  let sorted = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-cM"], root,
    {LC_ALL: "C"}, b"Jan\nFeb\n", stdout, stderr))?
  assert sorted.exit_code()? == 0
  assert stdout.read_bytes()?.is_empty()
  assert stderr.read_bytes()?.is_empty()

  let disorder = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-cM"], root,
    {LC_ALL: "C"}, b"Feb\nJan\n", stdout, stderr))?
  assert disorder.exit_code()? == 1
  assert stdout.read_bytes()?.is_empty()
  assert stderr.read_text()? == "sort: -:2: disorder: Jan\n"

  let duplicate = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-cuM"], root,
    {LC_ALL: "C"}, b"Jan\nJAN\n", stdout, stderr))?
  assert duplicate.exit_code()? == 1
  assert stdout.read_bytes()?.is_empty()
  assert stderr.read_text()? == "sort: -:2: disorder: JAN\n"
}

test test_sort_general_numeric_hexadecimal_values { |ctx|
  let root = test.temp_dir(ctx, name: "sort-general-hex")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let input = b"0x123\n0x0\n0x2p10\n0x9p-10\n"
  let result = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-g"], root,
    {LC_ALL: "C"}, input, stdout, stderr))?
  assert result.exit_code()? == 0
  assert stdout.read_bytes()? == b"0x0\n0x9p-10\n0x123\n0x2p10\n"
  assert stderr.read_bytes()?.is_empty()
}

test test_sort_general_numeric_mode_aliases_and_conflict { |ctx|
  let root = test.temp_dir(ctx, name: "sort-general-mode")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  for mode in ["--sort=g", "--sort=general", "--sort=general-numeric", "--sort=general-numeri"] {
    let result = process.run(process.command_argv(ctx.xsh_bin,
      [ctx.xsh_bin.display(), "--", script.display(), mode], root,
      {LC_ALL: "C"}, b"1e2\n2\n", stdout, stderr))?
    assert result.exit_code()? == 0
    assert stdout.read_bytes()? == b"2\n1e2\n"
    assert stderr.read_bytes()?.is_empty()
  }

  let conflict = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-ng"], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert conflict.exit_code()? == 2
  assert stderr.read_text()? == "sort: options '-gn' are incompatible\n"
  assert stdout.read_bytes()?.is_empty()
}

test test_sort_general_numeric_unique_compares_values { |ctx|
  let root = test.temp_dir(ctx, name: "sort-general-unique")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let result = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-gu"], root,
    {LC_ALL: "C"}, b"1.0\n1\n1e0\n2\n", stdout, stderr))?
  assert result.exit_code()? == 0
  assert stdout.read_bytes()? == b"1.0\n2\n"
  assert stderr.read_bytes()?.is_empty()
}

test test_sort_numeric_uses_decimal_values_and_prefixes { |ctx|
  let root = test.temp_dir(ctx, name: "sort-numeric-decimal")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let input = b"8.013\n1.444\n1.58590\n-8.90880\n1.040000000\n-.05\n10x\n2x\n"
  let result = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-n"], root,
    {LC_ALL: "C"}, input, stdout, stderr))?
  assert result.exit_code()? == 0
  assert stdout.read_bytes()? == b"-8.90880\n-.05\n1.040000000\n1.444\n1.58590\n2x\n8.013\n10x\n"
  assert stderr.read_bytes()?.is_empty()

  let debug = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "--debug", "-n"], root,
    {LC_ALL: "C"}, b"  2x\n  1x\n", stdout, stderr))?
  assert debug.exit_code()? == 0
  assert stdout.read_bytes()? == b"  1x\n  _\n____\n  2x\n  _\n____\n"
  assert stderr.read_bytes()?.is_empty()
}

test test_sort_numeric_preserves_decimal_precision_and_uniqueness { |ctx|
  let root = test.temp_dir(ctx, name: "sort-numeric-precision")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let precise = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-n"], root,
    {LC_ALL: "C"}, b"1.0000000000000000002\n1.0000000000000000001\n", stdout, stderr))?
  assert precise.exit_code()? == 0
  assert stdout.read_bytes()? == b"1.0000000000000000001\n1.0000000000000000002\n"
  assert stderr.read_bytes()?.is_empty()

  let unique = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-nu"], root,
    {LC_ALL: "C"}, b"1e0\n1.0\n1\n", stdout, stderr))?
  assert unique.exit_code()? == 0
  assert stdout.read_bytes()? == b"1e0\n"
  assert stderr.read_bytes()?.is_empty()
}

test test_sort_numeric_unique_debug_annotates_only_the_numeric_key { |ctx|
  let root = test.temp_dir(ctx, name: "sort-numeric-unique-debug")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let result = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-nu", "--debug"], root,
    {LC_ALL: "C"}, b"4\n2\n4\n", stdout, stderr))?
  assert result.exit_code()? == 0
  assert stdout.read_bytes()? == b"2\n_\n4\n_\n"
  assert stderr.read_bytes()?.is_empty()
}

test test_sort_numeric_unique_keeps_first_equal_line_when_reversed { |ctx|
  let root = test.temp_dir(ctx, name: "sort-numeric-unique-first")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let input = b"1\n00000001\n576,446.890\n576,446.88800000\n"
  let result = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-nur"], root,
    {LC_ALL: "C"}, input, stdout, stderr))?
  assert result.exit_code()? == 0
  assert stdout.read_bytes()? == b"576,446.890\n1\n"
  assert stderr.read_bytes()?.is_empty()
}

test test_sort_numeric_key_character_offset { |ctx|
  let root = test.temp_dir(ctx, name: "sort-numeric-key-offset")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let result = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-n", "-k1.2"], root,
    {LC_ALL: "C"}, b"19\n21\n", stdout, stderr))?
  assert result.exit_code()? == 0
  assert stdout.read_bytes()? == b"21\n19\n"
  assert stderr.read_bytes()?.is_empty()

  let reverse_key = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-n", "-k1r"], root,
    {LC_ALL: "C"}, b"19\n21\n3\n", stdout, stderr))?
  assert reverse_key.exit_code()? == 0
  assert stdout.read_bytes()? == b"3\n21\n19\n"
  assert stderr.read_bytes()?.is_empty()
}

test test_sort_human_numeric_units_and_aliases { |ctx|
  let root = test.temp_dir(ctx, name: "sort-human-numeric")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let input = b"1G\n1000M\n999K\n1K\n1\n"
  for option in ["-h", "--human-numeric-sort", "--sort=human-numeric", "--sort=human-numeri", "--sort=human", "--sort=h"] {
    let result = process.run(process.command_argv(ctx.xsh_bin,
      [ctx.xsh_bin.display(), "--", script.display(), option], root,
      {LC_ALL: "C"}, input, stdout, stderr))?
    assert result.exit_code()? == 0
    assert stdout.read_bytes()? == b"1\n1K\n999K\n1000M\n1G\n"
    assert stderr.read_bytes()?.is_empty()
  }
}

test test_sort_human_numeric_unique_and_stable_zeros { |ctx|
  let root = test.temp_dir(ctx, name: "sort-human-numeric-zero")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let stable_input = b"0M\n0K\n-0K\n-P\n-0M\n"
  let stable = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-hs"], root,
    {LC_ALL: "C"}, stable_input, stdout, stderr))?
  assert stable.exit_code()? == 0
  assert stdout.read_bytes()? == stable_input
  assert stderr.read_bytes()?.is_empty()

  let unique = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-hu"], root,
    {LC_ALL: "C"}, b"0M\n0K\n-P\n1K\n", stdout, stderr))?
  assert unique.exit_code()? == 0
  assert stdout.read_bytes()? == b"0M\n1K\n"
  assert stderr.read_bytes()?.is_empty()
}

test test_sort_human_numeric_option_conflicts { |ctx|
  let root = test.temp_dir(ctx, name: "sort-human-numeric-conflicts")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  for pair in [["-hn", "-hn"], ["-hg", "-gh"], ["-hd", "-dh"], ["-hi", "-hi"]] {
    let result = process.run(process.command_argv(ctx.xsh_bin,
      [ctx.xsh_bin.display(), "--", script.display(), pair[0]], root,
      {LC_ALL: "C"}, b"", stdout, stderr))?
    assert result.exit_code()? == 2
    assert stderr.read_text()? == f"sort: options '{pair[1]}' are incompatible\n"
    assert stdout.read_bytes()?.is_empty()
  }
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
