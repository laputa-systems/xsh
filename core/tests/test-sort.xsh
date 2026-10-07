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
