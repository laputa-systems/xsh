##! Container-internal lifecycle commands invoked directly by Docker through XSH.

use build
use context
use dist
use targets

## Repairs the mounted target tree ownership after a container lifecycle operation.
export proc repair_target(ctx: Context) [process, env, error, io] -> Result[Unit] {
  let uid = env.get_or("HOST_UID", "")?.trim()
  let gid = env.get_or("HOST_GID", "")?.trim()

  if uid == "" or gid == "" {
    return
  }

  context.run_stage("container-ownership-repair", ctx.target_triple, "chown", ["chown", "-R", f"${uid}:${gid}", ctx.target_dir.display()], ctx.root, {})?
}

## Builds and verifies distribution products inside the selected Linux container.
export proc container_dist(ctx: Context) [fs, env, process, error, io] -> Result[Unit] {
  context.ensure_dir(ctx.target_dir)?
  defer repair_target(ctx)?
  build.prepare_native_musl(ctx)?
  dist.native_dist(ctx, "DIST_DOCKER_BUILD_STD_FLAGS")?
}

## Runs the privileged developer Linux test sequence inside the container.
export proc linux_developer_test(ctx: Context) [fs, env, process, error, io] -> Result[Unit] {
  context.run_stage("linux-git-safe-directory", ctx.target_triple, "git", ["git", "config", "--global", "--add", "safe.directory", "/work"], ctx.root, {})?
  context.run_stage(
    "linux-build-test-tools",
    ctx.target_triple,
    "cargo",
    ["cargo", "build", "-p", "xsh", "-p", "xsht", "--bin", "xsh", "--bin", "xsh-test-sleeper", "--bin", "xsht"],
    ctx.root,
    {},
  )?
  fs.remove(p"/bin/xsh", missing_ok: true)?
  fs.symlink(fp"${ctx.target_dir}/debug/xsh", p"/bin/xsh")?
  let stress_repeat = env.get_or("XSH_OS_STRESS_REPEAT", "25")?.trim()
  context.run_stage(
    "linux-rust-tests",
    ctx.target_triple,
    "cargo",
    ["cargo", "test", "--features", "linux-priv-tests"],
    ctx.root,
    {XSH_OS_STRESS_REPEAT: if stress_repeat == "" { "25" } else { stress_repeat }},
  )?
  let xsht = fp"${ctx.target_dir}/debug/xsht"
  context.run_stage(
    "linux-native-tests",
    ctx.target_triple,
    xsht.display(),
    [xsht.display(), "test"],
    ctx.root,
    {CARGO_BIN_EXE_xsh_test_sleeper: fp"${ctx.target_dir}/debug/xsh-test-sleeper".display()},
  )?
}

## Runs the selected Linux CI test contract and always repairs mounted output ownership.
export proc linux_ci_test(ctx: Context) [fs, env, process, error, io] -> Result[Unit] {
  context.ensure_dir(ctx.target_dir)?
  defer repair_target(ctx)?
  context.run_stage("linux-git-safe-directory", ctx.target_triple, "git", ["git", "config", "--global", "--add", "safe.directory", "/work"], ctx.root, {})?
  let command_env = targets.docker_test_env(ctx.target_triple)?
  context.run_stage(
    "linux-ci-tests",
    ctx.target_triple,
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
      ctx.target_triple,
      "--",
      "--nocapture",
    ],
    ctx.root,
    command_env,
  )?
}

## Repairs the bind-mounted coverage result directory after container work completes.
export proc repair_coverage(ctx: Context) [process, env, error, io] -> Result[Unit] {
  let uid = env.get_or("HOST_UID", "")?.trim()
  let gid = env.get_or("HOST_GID", "")?.trim()

  if uid == "" or gid == "" {
    return
  }

  context.run_stage("coverage-ownership-repair", ctx.target_triple, "chown", ["chown", "-R", f"${uid}:${gid}", ctx.coverage_dir.display()], ctx.root, {})?
}

## Runs the existing coverage program from the privileged coverage container.
export proc container_coverage(ctx: Context) [process, env, error, io] -> Result[Unit] {
  defer repair_coverage(ctx)?
  context.run_stage("coverage-container", ctx.target_triple, "cargo", ["cargo", "run", "--quiet", "-p", "xsh", "--bin", "xsh", "--", "tools/cov-linux.xsh"], ctx.root, {})?
}
