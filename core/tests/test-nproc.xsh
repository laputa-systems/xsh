type Ran = {status: Int, stdout: Str, stderr: Str, bytes: Bytes}

# Runs core/nproc.xsh by its real path (so the invoked name is nproc and
# `lib.gnu` resolves beside it), capturing both streams.
proc applet_run(
  ctx: TestContext,
  args: List[Str],
  vars: Record = {LC_ALL: "C"},
  stdin = b"",
) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "nproc")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/nproc.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, vars, stdin, out, err)
  let status = process.run(plan)?
  let raw = out.read_bytes()?

  Ok({status: status.exit_code()?, stdout: raw.utf8() ?? "", stderr: err.read_text()?, bytes: raw})
}

proc count(ctx: TestContext, args: List[Str], vars: Record = {LC_ALL: "C"}) [fs, process, error] -> Result[Str] {
  Ok(applet_run(ctx, args, vars:)?.stdout.trim())
}

test test_nproc_matches_the_cpu_count { |ctx|
  assert count(ctx, [])? == f"{cpu.count()}"
  assert count(ctx, ["--all"])? == f"{cpu.count()}"
}

test test_nproc_omp_num_threads_overrides_and_omp_thread_limit_caps { |ctx|
  assert count(ctx, [], {OMP_NUM_THREADS: "60"})? == "60"
  assert count(ctx, [], {OMP_NUM_THREADS: " 42 "})? == "42"
  assert count(ctx, [], {OMP_NUM_THREADS: "2,ignored"})? == "2"
  assert count(ctx, [], {OMP_NUM_THREADS: "42", OMP_THREAD_LIMIT: "2"})? == "2"
  assert count(ctx, [], {OMP_NUM_THREADS: "42", OMP_THREAD_LIMIT: "2bad"})? == "42"
  assert count(ctx, [], {OMP_THREAD_LIMIT: "1"})? == "1"
  assert count(ctx, [], {OMP_NUM_THREADS: "1", OMP_THREAD_LIMIT: ""})? == "1"
  assert count(ctx, ["--all"], {OMP_NUM_THREADS: "1"})? == f"{cpu.count()}", "--all disregards OpenMP variables"
}

test test_nproc_unusable_omp_values_fall_back_to_the_cpu_count { |ctx|
  let cpus = f"{cpu.count()}"

  for value in ["incorrectnumber", "0", "", "-3", "x,2"] {
    assert count(ctx, [], {OMP_NUM_THREADS: value})? == cpus, value
  }
}

test test_nproc_oversized_omp_values_clamp_to_the_unsigned_maximum { |ctx|
  assert count(ctx, [], {OMP_NUM_THREADS: "99999999999999999999"})? == "18446744073709551615"
}

test test_nproc_ignore_leaves_at_least_one { |ctx|
  let cpus = cpu.count()

  assert count(ctx, ["--ignore=0"])? == f"{cpus}"
  assert count(ctx, ["--ignore", "99999999999999999999"])? == "1"
  assert count(ctx, ["--ignore= 1"])? == f"{if cpus > 1 { cpus - 1 } else { 1 }}"
  assert count(ctx, ["--ignore=40"], {OMP_NUM_THREADS: "42"})? == "2"
}

test test_nproc_rejects_bad_numbers_and_operands { |ctx|
  let bad = applet_run(ctx, ["--ignore=x"])?
  assert bad.status == 1
  assert bad.stderr == "nproc: invalid number: 'x'\n", bad.stderr

  let extra = applet_run(ctx, ["x"])?
  assert extra.status == 1
  assert extra.stderr == "nproc: extra operand 'x'\nTry 'nproc --help' for more information.\n", extra.stderr
  assert applet_run(ctx, ["--definitely-invalid"])?.status == 1
}

test test_nproc_help_and_version { |ctx|
  assert "Print the number of processing units available" in applet_run(ctx, ["--help"])?.stdout
  assert applet_run(ctx, ["--version"])?.stdout.starts_with("nproc ")
}
