type SortResult = {status: Int, stdout: Bytes, stderr: Str}

proc sort_run_at(ctx: TestContext, root: Path, args: List[Str], input = b"") [fs, process, error] -> Result[SortResult] {
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/sort.xsh".display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

proc sort_run(ctx: TestContext, args: List[Str], input = b"") [fs, process, error] -> Result[SortResult] {
  let root = test.temp_dir(ctx, name: "sort")?
  sort_run_at(ctx, root, args, input)
}

test test_sort_byte_order_and_unique_reverse { |ctx|
  assert sort_run(ctx, [], b"b\na\nb\n")?.stdout == b"a\nb\nb\n"
  assert sort_run(ctx, ["-u", "-r"], b"b\na\nb\n")?.stdout == b"b\na\n"
  assert sort_run(ctx, ["-z"], b"z\0a\0a")?.stdout == b"a\0a\0z\0"
}

test test_sort_numeric_keys_and_key_options { |ctx|
  assert sort_run(ctx, ["-n"], b"10\n-1\n2\n")?.stdout == b"-1\n2\n10\n"
  assert sort_run(ctx, ["-n"], b"-2\n-10\n-1.1\n-1\n0\n0.01\n2\n2.1\n")?.stdout == b"-10\n-2\n-1.1\n-1\n0\n0.01\n2\n2.1\n"
  assert sort_run(ctx, ["-M"], b"DEC\nJAN\nFEB\nunknown\n")?.stdout == b"unknown\nJAN\nFEB\nDEC\n"
  assert sort_run(ctx, ["--sort=v"], b"0.1\n0.02\n0.2\n0.002\n0.3\n")?.stdout == b"0.1\n0.002\n0.02\n0.2\n0.3\n"
  assert sort_run(ctx, ["-t,", "-k2", "-n"], b"b,20\na,3\nc,1\n")?.stdout == b"c,1\na,3\nb,20\n"
  assert sort_run(ctx, ["-k1.2", "-n"], b"19\n21\n")?.stdout == b"21\n19\n"
  assert sort_run(ctx, ["-k1r"], b"19\n21\n3\n")?.stdout == b"3\n21\n19\n"
  assert sort_run(ctx, ["-f"], b"A\na\n_\n")?.stdout == b"A\na\n_\n"
}

test test_sort_check_modes_and_errors { |ctx|
  let sorted = sort_run(ctx, ["-c"], b"a\nb\n")?
  assert sorted.status == 0
  assert sorted.stdout == b""
  assert sorted.stderr == ""

  let disorder = sort_run(ctx, ["-c"], b"b\na\n")?
  assert disorder.status == 1
  assert disorder.stderr == "sort: -:2: disorder: a\n", disorder.stderr

  let silent = sort_run(ctx, ["--check=q"], b"b\na\n")?
  assert silent.status == 1
  assert silent.stderr == ""

  let duplicate = sort_run(ctx, ["-cu"], b"A\nA\n")?
  assert duplicate.status == 1
  assert duplicate.stderr == "sort: -:2: disorder: A\n", duplicate.stderr
}

test test_sort_merge_reads_files_before_output { |ctx|
  let root = test.temp_dir(ctx, name: "sort-merge")?
  fp"{root}/left".write(b"a\nc\n")?
  fp"{root}/right".write(b"b\nd\n")?
  let merged = sort_run_at(ctx, root, ["-m", "left", "right"])?
  assert merged.status == 0
  assert merged.stdout == b"a\nb\nc\nd\n"

  fp"{root}/same".write(b"kiwi\napple\n")?
  let output = sort_run_at(ctx, root, ["-o", "same", "same"])?
  assert output.status == 0
  assert output.stdout == b""
  assert fp"{root}/same".read_bytes()? == b"apple\nkiwi\n"
}

test test_sort_files0_from_and_input_preflight { |ctx|
  let root = test.temp_dir(ctx, name: "sort-files0")?
  fp"{root}/words".write(b"mango\nkiwi")?
  let listed = sort_run_at(ctx, root, ["--files0-from", "-"], b"words\0")?
  assert listed.status == 0
  assert listed.stdout == b"kiwi\nmango\n"

  let preflight = sort_run(ctx, ["/dev/random", "definitely-missing"], b"")?
  assert preflight.status == 2
  assert preflight.stderr == "sort: cannot read: definitely-missing: No such file or directory\n", preflight.stderr
}

test test_sort_accepts_or_rejects_each_option_explicitly { |ctx|
  let parallel = sort_run(ctx, ["--parallel=2"], b"b\na\n")?
  assert parallel.status == 0
  assert parallel.stdout == b"a\nb\n"

  let invalid_parallel = sort_run(ctx, ["--parallel=0"], b"")?
  assert invalid_parallel.status == 2
  assert "invalid --parallel" in invalid_parallel.stderr

  for option in ["-g", "-h", "-R", "--compress-program=gzip"] {
    let rejected = sort_run(ctx, [option], b"")?
    assert rejected.status == 2
    assert "not supported" in rejected.stderr, f"{option}: {rejected.stderr}"
  }

  let buffer = sort_run(ctx, ["-S10"], b"b\na\n")?
  assert buffer.status == 0
  assert buffer.stdout == b"a\nb\n"
  let bad_buffer = sort_run(ctx, ["-S", "100f"], b"")?
  assert bad_buffer.status == 2
  assert "invalid suffix" in bad_buffer.stderr

  let help = sort_run(ctx, ["--help"])?
  assert help.status == 0
  assert help.stdout.utf8()?.starts_with("Usage: sort [OPTION]...")
  let version = sort_run(ctx, ["--version"])?
  assert version.status == 0
  assert version.stdout.utf8()?.starts_with("sort (XSH core)")
}
