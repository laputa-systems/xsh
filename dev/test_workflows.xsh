##! Rust, native XSH, and privileged platform test command policy.
use cargo_steps
use context
use docker
use stage as stages

## Runs the repository's Rust test contract. Tests spawn release binaries
## only, so the integration targets build in the release profile. The unit
## tests spawn nothing and run in debug, where the checks compiled only under
## debug assertions (such as lowering's agreement with the checker) exist.
export proc rust(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit, Error] {
  cargo_steps.run_test(
    stages.command(
      "test-rust",
      ctx.target.triple,
      "cargo",
      [
        "cargo",
        "test",
        "--release",
        "--test",
        "integration",
        "--test",
        "ambient_fs_policy",
        "--test",
        "symbol_plateau",
        "--test",
        "core_compat_boundaries",
        "--",
        "-Zunstable-options",
        "--report-time",
      ],
      ctx.root,
      {},
    ),
  )
  cargo_steps.run_test(
    stages.command(
      "test-rust-debug-assertions",
      ctx.target.triple,
      "cargo",
      ["cargo", "test", "--lib", "--", "-Zunstable-options", "--report-time"],
      ctx.root,
      {},
    ),
  )
}

## Runs only the native XSH test corpus through the release `xsht`, which
## spawns its sibling release `xsh`.
export proc xsh(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit, Error] {
  cargo_steps.run_build(
    stages.command(
      "test-xsh-build",
      ctx.target.triple,
      "cargo",
      [
        "cargo",
        "build",
        "--release",
        "-p",
        "xsh",
        "--bins",
        "-p",
        "xsht",
        "--bin",
        "xsht",
      ],
      ctx.root,
      {},
    ),
  )
  let xsht = fp"{ctx.target_dir}/release/xsht"
  stages.execute(
    stages.command(
      "test-xsh",
      ctx.target.triple,
      xsht.display(),
      [xsht.display(), "test"],
      ctx.root,
      {},
    ),
  )
}

## Runs privileged Linux developer tests through a direct Docker-to-XSH command.
export proc linux_test(ctx: context.Context, ci: Bool) [process, env, error, io] -> Result[Unit, Error] {
  if ci {
    docker.run_internal(ctx, "test-linux-ci", true, [])
  } else {
    docker.run_internal(ctx, "test-linux", true, [])
  }
}

## Runs the selected Darwin CI test contract directly on macOS.
export proc macos_ci(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit, Error] {
  cargo_steps.run_test(
    stages.command(
      "test-macos-ci",
      ctx.target.triple,
      "cargo",
      [
        "cargo",
        "test",
        "--locked",
        "--profile",
        ctx.profile,
        "--features",
        "net tools",
        "--target",
        ctx.target.triple,
        "--",
        "--nocapture",
      ],
      ctx.root,
      {MACOSX_DEPLOYMENT_TARGET: ctx.darwin_deployment_target},
    ),
  )
}
