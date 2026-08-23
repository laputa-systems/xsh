##! Rust, native XSH, and privileged platform test command policy.
use context
use stage as stages
use docker

## Runs the repository's ordinary release Rust test contract.
export proc rust(ctx: context.Context) [process, error, io] -> Result[Unit] {
  stages.execute(
    stages.command(
      "test-rust",
      ctx.target.triple,
      "cargo",
      ["cargo", "test", "--release", "--", "-Zunstable-options", "--report-time"],
      ctx.root,
      {},
    ),
  )?
}

## Runs only the native XSH test corpus through its owning `xsht` package and binary.
export proc xsh(ctx: context.Context) [process, error, io] -> Result[Unit] {
  stages.execute(
    stages.command(
      "test-xsh",
      ctx.target.triple,
      "cargo",
      [
        "cargo",
        "run",
        "--release",
        "-p",
        "xsht",
        "--bin",
        "xsht",
        "--",
        "test",
      ],
      ctx.root,
      {},
    ),
  )?
}

## Runs privileged Linux developer tests through a direct Docker-to-XSH command.
export proc linux_test(ctx: context.Context, ci: Bool) [process, env, error, io] -> Result[Unit] {
  if ci {
    docker.run_internal(ctx, "test-linux-ci", true, [])?
  } else {
    docker.run_internal(ctx, "test-linux", true, [])?
  }
}

## Runs the selected Darwin CI test contract directly on macOS.
export proc macos_ci(ctx: context.Context) [process, error, io] -> Result[Unit] {
  stages.execute(
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
  )?
}
