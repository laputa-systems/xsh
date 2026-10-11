##! Container-internal lifecycle commands invoked directly by Docker through XSH.
use build
use cargo_steps
use context
use dist
use stage as stages
use targets

## Repairs the mounted target tree ownership after a container lifecycle operation.
export proc repair_target(ctx: context.Context) [process, env, error, io] -> Result[Unit, Error] {
  let uid = env.get_or("HOST_UID", "")?.trim()
  let gid = env.get_or("HOST_GID", "")?.trim()

  return when uid == "" or gid == ""

  stages.execute(
    stages.command(
      "container-ownership-repair",
      ctx.target.triple,
      "chown",
      ["chown", "-R", f"{uid}:{gid}", ctx.target_dir.display()],
      ctx.root,
      {},
    ),
  )
}

## Builds and verifies distribution products inside the selected Linux container.
export proc container_dist(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit, Error] {
  stages.ensure_dir(ctx.target_dir)
  defer repair_target(ctx)
  build.prepare_native_musl(ctx)
  dist.native_dist(ctx, "DIST_DOCKER_BUILD_STD_FLAGS")
}

## Runs the privileged developer Linux test sequence inside the container on
## release binaries, as every test does.
export proc linux_developer_test(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit, Error] {
  stages.execute(
    stages.command(
      "linux-git-safe-directory",
      ctx.target.triple,
      "git",
      ["git", "config", "--global", "--add", "safe.directory", "/work"],
      ctx.root,
      {},
    ),
  )
  cargo_steps.run_build(
    stages.command(
      "linux-build-test-tools",
      ctx.target.triple,
      "cargo",
      [
        "cargo",
        "build",
        "--release",
        "-p",
        "xsh",
        "-p",
        "xsht",
        "--bin",
        "xsh",
        "--bin",
        "xsh-test-helper",
        "--bin",
        "xsht",
      ],
      ctx.root,
      {},
    ),
  )
  p"/bin/xsh".remove()
  p"/bin/xsh".symlink(to: fp"{ctx.target_dir}/release/xsh")
  let stress_repeat = env.get_or("XSH_OS_STRESS_REPEAT", "25")?.trim()
  cargo_steps.run_test(
    stages.command(
      "linux-rust-tests",
      ctx.target.triple,
      "cargo",
      [
        "cargo",
        "test",
        "--release",
        "--features",
        "linux-priv-tests",
        "--test",
        "integration",
        "--test",
        "ambient_fs_policy",
        "--test",
        "symbol_plateau",
        "--test",
        "core_compat_boundaries",
        "--test",
        "core_compat_extra",
        "--test",
        "linux_priv",
      ],
      ctx.root,
      {XSH_OS_STRESS_REPEAT: if stress_repeat == "" { "25" } else { stress_repeat }},
    ),
  )
  cargo_steps.run_test(
    stages.command(
      "linux-rust-unit-tests",
      ctx.target.triple,
      "cargo",
      ["cargo", "test", "--features", "linux-priv-tests", "--lib"],
      ctx.root,
      {},
    ),
  )
  let xsht = fp"{ctx.target_dir}/release/xsht"
  stages.execute(
    stages.command(
      "linux-native-tests",
      ctx.target.triple,
      xsht.display(),
      [xsht.display(), "test"],
      ctx.root,
      {},
    ),
  )
}

## Runs the selected Linux CI test contract and always repairs mounted output ownership.
export proc linux_ci_test(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit, Error] {
  stages.ensure_dir(ctx.target_dir)
  defer repair_target(ctx)
  stages.execute(
    stages.command(
      "linux-git-safe-directory",
      ctx.target.triple,
      "git",
      ["git", "config", "--global", "--add", "safe.directory", "/work"],
      ctx.root,
      {},
    ),
  )
  let environment = targets.docker_test_env(ctx.target.triple)?
  cargo_steps.run_build(
    stages.command(
      "linux-ci-build-products",
      ctx.target.triple,
      "cargo",
      [
        "cargo",
        "build",
        "--locked",
        "--profile",
        ctx.profile,
        "--target",
        ctx.target.triple,
        "-p",
        "xsh",
        "-p",
        "xshi",
        "-p",
        "xsht",
        "--bin",
        "xsh",
        "--bin",
        "xshi",
        "--bin",
        "xsht",
      ],
      ctx.root,
      environment,
    ),
  )
  if ctx.profile == "dev" {
    let debug = fp"{ctx.target_dir}/debug"
    stages.ensure_dir(debug)
    for product in targets.products {
      let destination = fp"{debug}/{product}"
      let source = fp"{ctx.target_dir}/{ctx.target.triple}/debug/{product}"
      if destination != source {
        destination.remove()
        destination.symlink(to: source)
      }
    }
  }

  cargo_steps.run_test(
    stages.command(
      "linux-ci-tests",
      ctx.target.triple,
      "cargo",
      [
        "cargo",
        "test",
        "--locked",
        "--profile",
        ctx.profile,
        "--features",
        "linux-priv-tests net tools",
        "--target",
        ctx.target.triple,
        "--",
        "--nocapture",
      ],
      ctx.root,
      environment,
    ),
  )
}

## Repairs the bind-mounted coverage result directory after container work completes.
export proc repair_coverage(ctx: context.Context) [process, env, error, io] -> Result[Unit, Error] {
  let uid = env.get_or("HOST_UID", "")?.trim()
  let gid = env.get_or("HOST_GID", "")?.trim()

  return when uid == "" or gid == ""

  stages.execute(
    stages.command(
      "coverage-ownership-repair",
      ctx.target.triple,
      "chown",
      ["chown", "-R", f"{uid}:{gid}", ctx.coverage_dir.display()],
      ctx.root,
      {},
    ),
  )
}

## Runs the existing coverage program from the privileged coverage container.
export proc container_coverage(ctx: context.Context) [process, env, error, io] -> Result[Unit, Error] {
  defer repair_coverage(ctx)
  stages.execute(
    stages.command(
      "coverage-container",
      ctx.target.triple,
      "cargo",
      [
        "cargo",
        "run",
        "--quiet",
        "-p",
        "xsh",
        "--bin",
        "xsh",
        "--",
        "tools/cov-linux.xsh",
      ],
      ctx.root,
      {},
    ),
  )
}
