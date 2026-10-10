use core.lib.text_a2 as text

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

test test_sort_ignore_leading_blanks_preserves_utf8 { |ctx|
  let root = test.temp_dir(ctx, name: "sort-leading-blanks-utf8")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let input = b"é\n a\n"
  let result = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-b"], root,
    {LC_ALL: "C"}, input, stdout, stderr))?
  assert result.exit_code()? == 0
  assert stdout.read_bytes()? == b" a\né\n"
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
  for args in [["-dg"], ["-dM"], ["-ig"], ["-iM"], ["-k", "1d,1n"], ["-k", "1i,1M"]] {
    let result = process.run(process.command_argv(ctx.xsh_bin,
      [ctx.xsh_bin.display(), "--", script.display()] + args, root,
      {LC_ALL: "C"}, b"", stdout, stderr))?
    assert result.exit_code()? == 2, args.join(" ")
    assert stderr.read_text()?.starts_with("sort: options '-"), args.join(" ")
    assert stdout.read_bytes()?.is_empty()
  }
  let inherited = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-d", "-k", "1,1n"], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert inherited.exit_code()? == 0, stderr.read_text()?
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

test test_sort_files0_from_reads_raw_file_name_lists { |ctx|
  let root = test.temp_dir(ctx, name: "sort-files0-from")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let first = fp"{root}/first"
  let second = fp"{root}/second"
  let names = fp"{root}/names0"
  first.write("mango\nkiwi")
  second.write("apple\nbanana\n")
  names.write(bytes.concat([bytes.from_text(first.display()), b"\0", bytes.from_text(second.display()), b"\0"]))

  let from_file = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "--files0-from", names.display()], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert from_file.exit_code()? == 0
  assert stdout.read_bytes()? == b"apple\nbanana\nkiwi\nmango\n"
  assert stderr.read_bytes()?.is_empty()

  let from_stdin = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "--files0-from=-"], root,
    {LC_ALL: "C"}, bytes.concat([bytes.from_text(first.display()), b"\0", bytes.from_text(second.display())]), stdout, stderr))?
  assert from_stdin.exit_code()? == 0
  assert stdout.read_bytes()? == b"apple\nbanana\nkiwi\nmango\n"
  assert stderr.read_bytes()?.is_empty()

  let invalid = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "--files0-from=-"], root,
    {LC_ALL: "C"}, b"first\0\0second\0", stdout, stderr))?
  assert invalid.exit_code()? == 2
  assert stdout.read_bytes()?.is_empty()
  assert stderr.read_text()? == "sort: -:2: invalid zero-length file name\n"
}

test test_sort_merge_orders_sorted_inputs_with_sort_options { |ctx|
  let root = test.temp_dir(ctx, name: "sort-merge")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let first = fp"{root}/first"
  let second = fp"{root}/second"

  first.write("alpha\ngamma\n")
  second.write("beta\ndelta\n")
  let merged = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-m", first.display(), second.display()], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert merged.exit_code()? == 0
  assert stdout.read_bytes()? == b"alpha\nbeta\ndelta\ngamma\n"
  assert stderr.read_bytes()?.is_empty()

  let checked = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-m", "-c", first.display(), second.display()], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert checked.exit_code()? == 0
  assert stdout.read_bytes()?.is_empty()
  assert stderr.read_bytes()?.is_empty()

  first.write("9\n5\n1\n")
  second.write("8\n4\n2\n")
  let reversed = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-m", "-r", first.display(), second.display()], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert reversed.exit_code()? == 0
  assert stdout.read_bytes()? == b"9\n8\n5\n4\n2\n1\n"
  assert stderr.read_bytes()?.is_empty()

  first.write("01 apple\n2 pear\n")
  second.write("1 banana\n3 plum\n")
  let stable = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-m", "-s", "-n", "-k1,1", first.display(), second.display()], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert stable.exit_code()? == 0
  assert stdout.read_bytes()? == b"01 apple\n1 banana\n2 pear\n3 plum\n"
  assert stderr.read_bytes()?.is_empty()

  first.write(b"alpha\0gamma\0")
  second.write(b"beta\0delta\0")
  let zero_terminated = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-m", "-z", first.display(), second.display()], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert zero_terminated.exit_code()? == 0
  assert stdout.read_bytes()? == b"alpha\0beta\0delta\0gamma\0"
  assert stderr.read_bytes()?.is_empty()
}

test test_sort_merge_unique_and_output_file { |ctx|
  let root = test.temp_dir(ctx, name: "sort-merge-unique-output")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let first = fp"{root}/first"
  let second = fp"{root}/second"
  let output = fp"{root}/output"

  first.write("1\n3\n5\n")
  second.write("1.0\n2\n3.0\n")
  let unique = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-m", "-n", "-u", first.display(), second.display()], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert unique.exit_code()? == 0
  assert stdout.read_bytes()? == b"1\n2\n3\n5\n"
  assert stderr.read_bytes()?.is_empty()

  output.write("stale contents that must be truncated\n")
  let redirected = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-m", "-o", output.display(), first.display()], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert redirected.exit_code()? == 0
  assert stdout.read_bytes()?.is_empty()
  assert output.read_bytes()? == b"1\n3\n5\n"
  assert stderr.read_bytes()?.is_empty()
}

test test_sort_batch_size_validation { |ctx|
  let root = test.temp_dir(ctx, name: "sort-batch-size")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"

  let minimum = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "--batch-size=0"], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert minimum.exit_code()? == 2
  assert stderr.read_text()? == "sort: invalid --batch-size argument '0'\nsort: minimum --batch-size argument is '2'\n"
  assert stdout.read_bytes()?.is_empty()

  let invalid = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-m", "--batch-size=a"], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert invalid.exit_code()? == 2
  assert stderr.read_text()? == "sort: invalid --batch-size argument 'a'\n"
  assert stdout.read_bytes()?.is_empty()
}

test test_sort_buffer_size_rejects_percent_after_suffix { |ctx|
  let root = test.temp_dir(ctx, name: "sort-buffer-size-percent")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let result = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "--buffer-size=0x123%"], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert result.exit_code()? == 2
  assert stderr.read_text()? == "sort: invalid --buffer-size argument '0x123%'\n"
  assert stdout.read_bytes()?.is_empty()
}

test test_sort_buffer_size_spills_sorted_runs { |ctx|
  let root = test.temp_dir(ctx, name: "sort-buffer-spill")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let payload = text.padding(200, "x")
  var input_lines: List[Str] = []
  var expected_lines: List[Str] = []
  for index in range(400) { expected_lines += [f"{index:04} {payload}"] }
  for index in range(400) { input_lines += [expected_lines[399 - index]] }
  let input = bytes.from_text(input_lines.join("\n") + "\n")
  let result = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-n", "-S", "1K", "-T", root.display()], root,
    {LC_ALL: "C"}, input, stdout, stderr))?
  assert result.exit_code()? == 0
  assert stdout.read_bytes()? == bytes.from_text(expected_lines.join("\n") + "\n")
  assert stderr.read_bytes()?.is_empty()
}

test test_sort_sigint_removes_external_runs { |ctx|
  let root = test.temp_dir(ctx, name: "sort-buffer-spill-sigint")?
  let input = fp"{root}/input"
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let payload = text.padding(200, "x")
  var lines: List[Str] = []
  for index in range(30000) { lines += [f"{index:05} {payload}"] }
  input.write(lines.join("\n") + "\n")

  let child = spawn run @(["/bin/sh", "-c", "error_file=$1; shift; exec \"$@\" 2>\"$error_file\"", "sort-test",
    stderr.display(), ctx.xsh_bin.display(), "--", script.display(), "-S1", "-T", root.display(),
    "-o", stdout.display(), input.display()]) ?
  var saw_run = false
  for _ in range(500) {
    if fs.files(root, hidden: true)? |> any .name.starts_with("run-") {
      saw_run = true
      break
    }
    time.sleep(10ms)?
  }
  assert saw_run, "sort did not write its first spill run"
  process.kill(child.pid, signal: "INT")?
  assert (wait child?).shell_code()? == 130
  assert stderr.read_text()?.is_empty(), "SIGINT emitted a cleanup diagnostic"
  assert ! (fs.files(root, hidden: true)? |> any .name.starts_with("run-")), "SIGINT left spill files behind"
}

test test_sort_buffer_size_requires_temporary_directory_when_spilling { |ctx|
  let root = test.temp_dir(ctx, name: "sort-buffer-spill-error")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let payload = text.padding(200, "x")
  var input_lines: List[Str] = []
  for index in range(400) { input_lines += [f"{399 - index:04} {payload}"] }
  let input = bytes.from_text(input_lines.join("\n") + "\n")
  let result = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-n", "-S1K", "-T", fp"{root}/missing".display()], root,
    {LC_ALL: "C"}, input, stdout, stderr))?
  assert result.exit_code()? == 2
  assert "cannot create temporary file" in stderr.read_text()?
  assert stdout.read_bytes()?.is_empty()
}

test test_sort_merge_batch_size_merges_multiple_passes { |ctx|
  let root = test.temp_dir(ctx, name: "sort-merge-batches")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let inputs = ["a\ng\n", "b\nh\n", "c\ni\n", "d\nj\n", "e\nk\n", "f\nl\n"]
  var paths: List[Str] = []
  for index in range(inputs.len()) {
    let input_path = fp"{root}/input{index}"
    input_path.write(inputs[index])
    paths += [input_path.display()]
  }
  let result = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-m", "--batch-size=2"] + paths, root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert result.exit_code()? == 0
  assert stdout.read_bytes()? == b"a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\n"
  assert stderr.read_bytes()?.is_empty()
}

test test_sort_separator_null_and_invalid_character_counts { |ctx|
  let root = test.temp_dir(ctx, name: "sort-separator")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let input = b"a\0z\nb\0a\n"
  let null_separator = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-k2", "-t", "\\0"], root,
    {LC_ALL: "C"}, input, stdout, stderr))?
  assert null_separator.exit_code()? == 0
  assert stdout.read_bytes()? == b"b\0a\na\0z\n"
  assert stderr.read_bytes()?.is_empty()

  let multiple_keys = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-k1,1", "-k3,3", "-t", "\\0"], root,
    {LC_ALL: "C"}, b"z\0a\0b\nz\0b\0a\na\0z\0z\n", stdout, stderr))?
  assert multiple_keys.exit_code()? == 0
  assert stdout.read_bytes()? == b"a\0z\0z\nz\0b\0a\nz\0a\0b\n"
  assert stderr.read_bytes()?.is_empty()

  let attached_equals = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-t=", "-k2"], root,
    {LC_ALL: "C"}, b"a=b=c\nb=a=d\n", stdout, stderr))?
  assert attached_equals.exit_code()? == 0
  assert stdout.read_bytes()? == b"b=a=d\na=b=c\n"
  assert stderr.read_bytes()?.is_empty()

  for separator in ["==", "=a"] {
    let invalid = process.run(process.command_argv(ctx.xsh_bin,
      [ctx.xsh_bin.display(), "--", script.display(), "-t", separator, "-k2"], root,
      {LC_ALL: "C"}, b"", stdout, stderr))?
    assert invalid.exit_code()? == 2
    let expected = if separator == "==" { "sort: separator must be exactly one character long: '=='\n" } else { "sort: separator must be exactly one character long: '=a'\n" }
    assert stderr.read_text()? == expected
    assert stdout.read_bytes()?.is_empty()
  }
}

test test_sort_key_option_suffixes_and_blank_boundaries { |ctx|
  let root = test.temp_dir(ctx, name: "sort-key-options")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let numeric_input = b"aa 3 cc\ndd 1 ff\ngg 2 cc\n"
  for args in [["-k", "2,2n"], ["-k", "2n,2"], ["-k", "2,2", "-n"]] {
    let result = process.run(process.command_argv(ctx.xsh_bin,
      [ctx.xsh_bin.display(), "--", script.display()] + args, root,
      {LC_ALL: "C"}, numeric_input, stdout, stderr))?
    assert result.exit_code()? == 0
    assert stdout.read_bytes()? == b"dd 1 ff\ngg 2 cc\naa 3 cc\n"
    assert stderr.read_bytes()?.is_empty()
  }

  let blank_start_input = b"aa   3 cc\ndd  1 ff\ngg         2 cc\n"
  for args in [["-k", "2b,2"], ["-k", "2,2", "-b"]] {
    let result = process.run(process.command_argv(ctx.xsh_bin,
      [ctx.xsh_bin.display(), "--", script.display()] + args, root,
      {LC_ALL: "C"}, blank_start_input, stdout, stderr))?
    assert result.exit_code()? == 0
    assert stdout.read_bytes()? == b"dd  1 ff\ngg         2 cc\naa   3 cc\n"
    assert stderr.read_bytes()?.is_empty()
  }

  let blank_end = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-k", "1,2.1b", "-s"], root,
    {LC_ALL: "C"}, b"a  b\na b\na   b\n", stdout, stderr))?
  assert blank_end.exit_code()? == 0
  assert stdout.read_bytes()? == b"a   b\na  b\na b\n"
  assert stderr.read_bytes()?.is_empty()
}

test test_sort_key_spec_rejects_invalid_numbers_and_characters { |ctx|
  let root = test.temp_dir(ctx, name: "sort-key-invalid")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let cases = [
    ["1.", "invalid number after '.': invalid count at start of ''"],
    ["1.1x", "stray character in field spec: invalid field specification '1.1x'"],
    ["0.1", "field number is zero: invalid field specification '0.1'"],
    ["1.0", "character offset is zero: invalid field specification '1.0'"],
    ["0", "field number is zero: invalid field specification '0'"],
    ["2.,3", "invalid number after '.': invalid count at start of ',3'"],
    ["2,", "invalid number after ',': invalid count at start of ''"],
    ["1.1,-k0", "invalid number after ',': invalid count at start of '-k0'"],
  ]
  for case in cases {
    let result = process.run(process.command_argv(ctx.xsh_bin,
      [ctx.xsh_bin.display(), "--", script.display(), "-k", case[0]], root,
      {LC_ALL: "C"}, b"", stdout, stderr))?
    assert result.exit_code()? == 2
    assert stderr.read_text()? == f"sort: {case[1]}\n"
    assert stdout.read_bytes()?.is_empty()
  }
}

test test_sort_merge_reports_stdout_write_failure { |ctx|
  if ! p"/dev/full".exists() { test.skip("requires /dev/full"); return }
  let root = test.temp_dir(ctx, name: "sort-merge-write-failure")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let first = fp"{root}/first"
  let second = fp"{root}/second"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  first.write("a\n")
  second.write("b\n")
  let status = process.run(process.command_argv(p"/bin/sh",
    ["sh", "-c", "exec \"$@\" > /dev/full", "sort-full", ctx.xsh_bin.display(), "--", script.display(), "-m", first.display(), second.display()],
    root, {LC_ALL: "C"}, b"", stdout, stderr))?
  assert status.exit_code()? == 2, stderr.read_text()?
  assert stderr.read_text()? == "sort: write failed: 'standard output': No space left on device\n"
  assert stdout.read_bytes()?.is_empty()
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

test test_sort_numeric_key_offset_can_start_inside_utf8 { |ctx|
  let root = test.temp_dir(ctx, name: "sort-numeric-key-utf8-offset")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let result = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-n", "-k1.2"], root,
    {LC_ALL: "C"}, b"é2\na1\n", stdout, stderr))?
  assert result.exit_code()? == 0
  assert stdout.read_bytes()? == b"é2\na1\n"
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

  let unicode = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-k2,2", "--debug"], root,
    {LC_ALL: "C"}, b"é x\n", stdout, stderr))?
  assert unicode.exit_code()? == 0
  assert stdout.read_bytes()? == b"é x\n  __\n____\n"
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

test test_sort_bare_check_leaves_following_operand_as_input { |ctx|
  let root = test.temp_dir(ctx, name: "sort-bare-check")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let input = test.temp_file(ctx, name: "input", contents: b"1\n3\n2\n")?
  let failed = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "--check", input.display()], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert failed.exit_code()? == 1
  assert stderr.read_text()? == f"sort: {input}:3: disorder: 2\n"
  assert stdout.read_bytes()?.is_empty()
}

test test_sort_reports_default_stdout_write_failure { |ctx|
  if ! p"/dev/full".exists() { test.skip("requires /dev/full"); return }
  let root = test.temp_dir(ctx, name: "sort-default-write-failure")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let status = process.run(process.command_argv(p"/bin/sh",
    ["sh", "-c", "exec \"$@\" > /dev/full", "sort-full", ctx.xsh_bin.display(), "--", script.display()],
    root, {LC_ALL: "C"}, b"hello\n", stdout, stderr))?
  assert status.exit_code()? == 2, stderr.read_text()?
  assert stderr.read_text()? == "sort: write failed: 'standard output': No space left on device\n"
  assert stdout.read_bytes()?.is_empty()
}

test test_sort_blank_debug_annotations_skip_leading_blanks { |ctx|
  let root = test.temp_dir(ctx, name: "sort-blank-debug")?
  let script = fp"{ctx.core_dir}/sort.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let input = test.temp_file(ctx, name: "input", contents: b"  a\nb\n")?
  let result = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "-b", "--debug", input.display()], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  assert result.exit_code()? == 0
  assert stderr.read_bytes()?.is_empty()
  assert stdout.read_text()? == "  a\n  _\n___\nb\n_\n_\n"
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
