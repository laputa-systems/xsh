##! Container-internal lifecycle commands invoked directly by Docker through XSH.
use build
use context
use stage as stages
use dist
use targets

## Repairs the mounted target tree ownership after a container lifecycle operation.
export proc repair_target(ctx: context.Context) [process, env, error, io] -> Result[Unit] {
  let uid = env.get_or("HOST_UID", "")?.trim()
  let gid = env.get_or("HOST_GID", "")?.trim()

  if uid == "" or gid == "" {
    return
  }

  stages.execute(
    stages.command(
      "container-ownership-repair",
      ctx.target.triple,
      "chown",
      ["chown", "-R", f"${uid}:${gid}", ctx.target_dir.display()],
      ctx.root,
      {},
    ),
  )?
}

## Builds and verifies distribution products inside the selected Linux container.
export proc container_dist(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit] {
  stages.ensure_dir(ctx.target_dir)?
  defer repair_target(ctx)?
  build.prepare_native_musl(ctx)?
  dist.native_dist(ctx, "DIST_DOCKER_BUILD_STD_FLAGS")?
}

## Runs the privileged developer Linux test sequence inside the container.
export proc linux_developer_test(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit] {
  stages.execute(
    stages.command(
      "linux-git-safe-directory",
      ctx.target.triple,
      "git",
      ["git", "config", "--global", "--add", "safe.directory", "/work"],
      ctx.root,
      {},
    ),
  )?
  stages.execute(
    stages.command(
      "linux-build-test-tools",
      ctx.target.triple,
      "cargo",
      [
        "cargo",
        "build",
        "-p",
        "xsh",
        "-p",
        "xsht",
        "--bin",
        "xsh",
        "--bin",
        "xsh-test-sleeper",
        "--bin",
        "xsht",
      ],
      ctx.root,
      {},
    ),
  )?
  fs.remove(/bin/xsh, missing_ok: true)?
  fs.symlink(fp"${ctx.target_dir}/debug/xsh", /bin/xsh)?
  let stress_repeat = env.get_or("XSH_OS_STRESS_REPEAT", "25")?.trim()
  stages.execute(
    stages.command(
      "linux-rust-tests",
      ctx.target.triple,
      "cargo",
      ["cargo", "test", "--features", "linux-priv-tests"],
      ctx.root,
      {XSH_OS_STRESS_REPEAT: if stress_repeat == "" { "25" } else { stress_repeat }},
    ),
  )?
  let xsht = fp"${ctx.target_dir}/debug/xsht"
  stages.execute(
    stages.command(
      "linux-native-tests",
      ctx.target.triple,
      xsht.display(),
      [xsht.display(), "test"],
      ctx.root,
      {CARGO_BIN_EXE_xsh_test_sleeper: fp"${ctx.target_dir}/debug/xsh-test-sleeper".display()},
    ),
  )?
}

## Runs the selected Linux CI test contract and always repairs mounted output ownership.
export proc linux_ci_test(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit] {
  stages.ensure_dir(ctx.target_dir)?
  defer repair_target(ctx)?
  stages.execute(
    stages.command(
      "linux-git-safe-directory",
      ctx.target.triple,
      "git",
      ["git", "config", "--global", "--add", "safe.directory", "/work"],
      ctx.root,
      {},
    ),
  )?
  let environment = targets.docker_test_env(ctx.target.triple)?
  stages.execute(
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
  )?
}

## Repairs the bind-mounted coverage result directory after container work completes.
export proc repair_coverage(ctx: context.Context) [process, env, error, io] -> Result[Unit] {
  let uid = env.get_or("HOST_UID", "")?.trim()
  let gid = env.get_or("HOST_GID", "")?.trim()

  if uid == "" or gid == "" {
    return
  }

  stages.execute(
    stages.command(
      "coverage-ownership-repair",
      ctx.target.triple,
      "chown",
      ["chown", "-R", f"${uid}:${gid}", ctx.coverage_dir.display()],
      ctx.root,
      {},
    ),
  )?
}

## Runs the existing coverage program from the privileged coverage container.
export proc container_coverage(ctx: context.Context) [process, env, error, io] -> Result[Unit] {
  defer repair_coverage(ctx)?
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
  )?
}
