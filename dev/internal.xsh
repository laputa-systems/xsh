##! Container-internal lifecycle commands invoked directly by Docker through XSH.
use build
use context
use dist
use gates
use stage as stages
use test_workflows

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

## Runs target-correct process, debug, native, and privileged gates.
export proc linux_developer_test(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit, Error] {
  stages.ensure_dir(ctx.target_dir)
  defer repair_target(ctx)
  stages.execute(stages.command("linux-git-safe-directory", ctx.target.triple, "git",
    ["git", "config", "--global", "--add", "safe.directory", ctx.root.display()], ctx.root, {}))
  let repeat = env.get_or("XSH_OS_STRESS_REPEAT", "25")?
  env XSH_OS_STRESS_REPEAT=$repeat {
    test_workflows.full(ctx, test_workflows.execution_profile()?, privileged: true)
  }
}

## Developer and CI Linux tests share the same target and gate contract.
export proc linux_ci_test(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit, Error] {
  linux_developer_test(ctx)
}

proc gate_git_directory(ctx: context.Context) [process, error, io] -> Result[Unit, Error] {
  stages.execute(stages.command("gate-git-safe-directory", ctx.target.triple, "git",
    ["git", "config", "--global", "--add", "safe.directory", ctx.root.display()], ctx.root, {}))
}

## Runs selected Linux lane checks inside the pinned container.
export proc gate_lane(ctx: context.Context, paths: List[Str]) [fs, process, env, error, io] -> Result[Unit, Error] {
  stages.ensure_dir(ctx.target_dir)
  defer repair_target(ctx)
  gate_git_directory(ctx)
  gates.lane(ctx, paths)
}

## Runs the complete Linux batch sequence inside the pinned container.
export proc gate_batch(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit, Error] {
  stages.ensure_dir(ctx.target_dir)
  defer repair_target(ctx)
  gate_git_directory(ctx)
  gates.batch(ctx)
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
