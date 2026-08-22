##! Rustybench invocation policy for XSH's interactive benchmark workflows.
use context

## Resolves a shell-free rustybench command prefix, preserving the legacy override.
export proc command_prefix(ctx: Context) [process, env, error] -> Result[List[Str]] {
  let configured = env.get_or("RUSTYBENCH", "")?.trim()

  if configured != "" {
    return process.argv_words(configured)?
  }

  return [
    "cargo",
    "run",
    "--quiet",
    "--manifest-path",
    fp"${ctx.root}/../../rustybench/Cargo.toml".display(),
    "--",
  ]
}

## Runs the latency and allocation baseline workflow, optionally in fast mode.
export proc benchmark(ctx: Context, fast: Bool) [process, env, error, io] -> Result[Unit] {
  let prefix = command_prefix(ctx)?
  let baseline = if fast {
    fp"${ctx.root}/crates/xshi/benches/fast-baseline.json"
  } else {
    fp"${ctx.root}/crates/xshi/benches/baseline.json"
  }
  var argv = prefix.extend(["baseline", "--root", ctx.root.display(), "--baseline", baseline.display()])

  if fast {
    argv = argv.push("--fast")
  }

  argv = argv.extend(["--", "cargo", "bench", "-p", "xshi", "--bench", "bench", "--features", "benchmark"])
  context.run_stage(if fast { "bench-fast" } else { "bench" }, ctx.target.triple, prefix[0], argv, ctx.root, {})?
}

## Runs rustybench's syscall diagnostic workflow.
export proc syscalls(ctx: Context) [process, env, error, io] -> Result[Unit] {
  let prefix = command_prefix(ctx)?
  let argv = prefix.extend(["syscalls", "--root", ctx.root.display()])
  context.run_stage("bench-syscalls", ctx.target.triple, prefix[0], argv, ctx.root, {})?
}
