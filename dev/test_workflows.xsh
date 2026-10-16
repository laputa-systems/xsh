##! Optimized process tests, debug assertions, and native corpus execution.
use cargo_steps
use context
use docker
use stage as stages
use stage_contract as contract
use targets

## External corpus formatting stays available as an explicit test; automatic
## verification covers this repository and individually migrated source files.
export const sibling_syntax_exclusions = ["--skip", "syntax::formatter_is_idempotent_on_laputa_corpus"]

const repository_lint_budget_test_filter = "lint_performance::repository_lint_is_clean_within_wall_budget"

## Test cases omitted from broad XSHT integration runs, including the separately guarded lint budget case.
export const xsht_integration_exclusions = [
  "--skip", "lint_format_invariance::formatting_preserves_lints_on_laputa_corpus",
  "--skip", "examples::example_corpus_is_formatted",
  "--skip", "examples::example_corpus_lints_without_warnings",
  "--skip", repository_lint_budget_test_filter,
]

## The only profiles accepted by process and native tests.
export enum TestProfile { Verification, Release }

## The executable products available to test plans.
export enum Product { Xsh, Xsht, Xshi }

## An explicitly requested profile that cannot run the test contract.
export error TestProfileError = Unsupported(profile: Str)

## Converts the closed test profile into Cargo's profile directory name.
export pure profile_name(profile: TestProfile) -> Str {
  match profile {
    Verification => "verification"
    Release => "release"
  }
}

pure product_name(product: Product) -> Str {
  match product {
    Xsh => "xsh"
    Xsht => "xsht"
    Xshi => "xshi"
  }
}

## Returns every executable that a full process-test plan may run.
export pure all_products() -> List[Product] {
  [Xsh, Xsht, Xshi]
}

## Packaging and tests have independent defaults; an explicit override must
## name a profile whose binaries process tests accept.
export proc execution_profile() [env, error] -> Result[TestProfile, Error] {
  let requested = env.get_or("DIST_PROFILE", "verification")?
  match requested {
    "verification" => Verification
    "release" => Release
    else => Err(TestProfileError.Unsupported(profile: requested))
  }
}

## Resolves a product built with an explicit target and accepted test profile.
export pure product_path(ctx: context.Context, profile: TestProfile, product: Product) -> Path {
  fp"{ctx.target_dir}/{ctx.target.triple}/{profile_name(profile)}/{product_name(product)}"
}

## Every Linux command uses the container's pinned static musl link contract.
export pure cargo_environment(ctx: context.Context) -> Result[Record, Error] {
  return targets.docker_test_env(ctx.target.triple) when ctx.target.os == "linux"

  {MACOSX_DEPLOYMENT_TARGET: ctx.darwin_deployment_target}
}

## Builds the exact release XSHT integration command for the repository lint budget test.
export pure repository_lint_budget_test(ctx: context.Context) -> Result[contract.CommandSpec, Error] {
  stages.command(
    "check-lint",
    ctx.target.triple,
    "cargo",
    [
      "cargo",
      "test",
      "--release",
      "--target",
      ctx.target.triple,
      "-p",
      "xsht",
      "--test",
      "integration",
      repository_lint_budget_test_filter,
      "--",
      "--exact",
      "--test-threads=1",
      "--nocapture",
    ],
    ctx.root,
    cargo_environment(ctx)?,
  )
}

pure product_build_arguments(product: Product) -> List[Str] {
  let name = product_name(product)
  match product {
    Xsh => ["-p", name, "--bins"]
    Xsht => ["-p", name, "--bin", name]
    Xshi => ["-p", name, "--bin", name]
  }
}

## Builds the selected products; an empty selection schedules no Cargo command.
export pure product_builds(
  ctx: context.Context,
  profile: TestProfile,
  products: List[Product],
) -> Result[List[contract.CommandSpec], Error] {
  return [] when products.is_empty()

  let product_arguments = collect {
    for product in products {
      yield @product_build_arguments(product)
    }
  }
  [stages.command(
    "test-products", ctx.target.triple, "cargo",
    ["cargo", "build", "--locked", "--profile", profile_name(profile), "--target", ctx.target.triple, @product_arguments],
    ctx.root, cargo_environment(ctx)?,
  )]
}

## Specifies process boundaries once so ordinary tests and coverage run the
## same targets. The native corpus is a separate step, avoiding a second full
## run through the root integration wrapper.
export pure rust_plan(
  ctx: context.Context,
  profile: TestProfile,
  privileged = false,
) -> Result[List[contract.CommandSpec], Error] {
  let environment = cargo_environment(ctx)?
  let profile_args = ["--locked", "--profile", profile_name(profile), "--target", ctx.target.triple]
  let debug_args = ["--locked", "--target", ctx.target.triple]
  var plan = product_builds(ctx, profile, all_products())?
  for target in [
    "integration",
    "ambient_fs_policy",
    "symbol_plateau",
    "core_compat_boundaries",
    "core_compat_extra",
  ] {
    plan += [stages.command("test-rust", ctx.target.triple, "cargo",
      ["cargo", "test", @profile_args, "-p", "xsh", "--test", target, "--",
        "--skip", "split::", "--skip", "runtime::coverage::"],
      ctx.root, environment)]
  }
  plan += [
    stages.command("test-debug-assertions", ctx.target.triple, "cargo",
      ["cargo", "test", @debug_args, "-p", "xsh", "--lib"], ctx.root, environment),
    stages.command("test-xsht", ctx.target.triple, "cargo",
      ["cargo", "test", @profile_args, "-p", "xsht", "--test", "integration", "--", @sibling_syntax_exclusions, @xsht_integration_exclusions], ctx.root, environment),
    stages.command("test-soundness", ctx.target.triple, "cargo",
      ["cargo", "test", @profile_args, "-p", "xsh-fuzz", "--test", "soundness"], ctx.root, environment),
  ]
  if privileged {
    for target in ["core_compat_extra", "linux_priv"] {
      plan += [stages.command("test-linux-priv", ctx.target.triple, "cargo",
        ["cargo", "test", @profile_args, "-p", "xsh", "--features", "linux-priv-tests", "--test", target,
          "--", "--nocapture", "--skip", "split::",
          "--skip", "cp::privileged::test_cp_recursive_block_device_"], ctx.root, environment)]
    }
  }
  plan
}

## Executes compiler steps with the existing allocator policy. Test execution
## never inherits the allocator selected only for its preceding build.
export proc run_commands(plan: List[contract.CommandSpec]) [fs, process, env, error, io] -> Result[Unit, Error] {
  for spec in plan {
    if spec.executable == "cargo" and spec.argv[1] == "test" {
      cargo_steps.run_test(spec)
    } else if spec.executable == "cargo" and spec.argv[1] in ["build", "check"] {
      cargo_steps.run_build(spec)
    } else {
      stages.execute(spec)
    }
  }
}

## Runs focused native tests, or the whole corpus when the caller requests it.
export proc native(
  ctx: context.Context,
  profile: TestProfile,
  filters: List[Str],
) [process, env, error, io] -> Result[Unit, Error] {
  let xsht = product_path(ctx, profile, Xsht)
  if ctx.target.os == "linux" and unix.id()?.euid == 0 {
    let host_uid = env.get("HOST_UID")?.trim() as Int
    let host_gid = env.get("HOST_GID")?.trim() as Int
    guard host_uid > 0 and host_gid >= 0 else {
      return Err(stages.StageError.Failed(
        stage: "test-xsh",
        target: ctx.target.triple,
        detail: "Linux native tests require a non-root host UID and GID",
      ))
    }

    let scratch = fp"{ctx.root}/.work/tmp/native-tests-{host_uid}-{host_gid}"
    let home = fp"{scratch}/home"
    let temp = fp"{scratch}/tmp"
    let as_host = ["setpriv", f"--reuid={host_uid}", f"--regid={host_gid}", "--clear-groups"]
    stages.execute(stages.command("test-xsh-scratch", ctx.target.triple, "setpriv",
      [@as_host, "/bin/mkdir", "-p", scratch.display()], ctx.root, {}))
    stages.execute(stages.command("test-xsh-scratch-group", ctx.target.triple, "setpriv",
      [@as_host, "/bin/chgrp", f"{host_gid}", scratch.display()], ctx.root, {}))
    stages.execute(stages.command("test-xsh-scratch-mode", ctx.target.triple, "setpriv",
      [@as_host, "/bin/chmod", "700", scratch.display()], ctx.root, {}))
    stages.execute(stages.command("test-xsh-directories", ctx.target.triple, "setpriv",
      [@as_host, "/bin/mkdir", "-p", home.display(), temp.display()], ctx.root, {}))
    stages.execute(stages.command("test-xsh-directory-groups", ctx.target.triple, "setpriv",
      [@as_host, "/bin/chgrp", f"{host_gid}", home.display(), temp.display()], ctx.root, {}))
    stages.execute(stages.command("test-xsh-directory-modes", ctx.target.triple, "setpriv",
      [@as_host, "/bin/chmod", "700", home.display(), temp.display()], ctx.root, {}))
    stages.execute(stages.command("test-xsh", ctx.target.triple, "setpriv",
      [@as_host, xsht.display(), "test", @filters], ctx.root,
      {HOME: home.display(), TMPDIR: temp.display()}))
  } else {
    stages.execute(stages.command("test-xsh", ctx.target.triple, xsht.display(),
      [xsht.display(), "test", @filters], ctx.root, {}))
  }
}

## Runs process and debug-assertion gates, followed by one full native suite.
export proc full(
  ctx: context.Context,
  profile: TestProfile,
  privileged = false,
) [fs, process, env, error, io] -> Result[Unit, Error] {
  run_commands(rust_plan(ctx, profile, privileged)?)
  native(ctx, profile, [])
}

## Runs the complete ordinary test contract in the verification profile by
## default; release remains available for final verification.
export proc rust(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit, Error] {
  full(ctx, execution_profile()?)
}

## Builds optimized products, then runs the native corpus once.
export proc xsh(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit, Error] {
  let profile = execution_profile()?
  run_commands(product_builds(ctx, profile, all_products())?)
  native(ctx, profile, [])
}

## Carries the selected test profile into the direct Docker command explicitly.
export proc linux_test(ctx: context.Context, ci: Bool) [fs, process, env, error, io] -> Result[Unit, Error] {
  let profile = execution_profile()?
  let selected = {...ctx, profile: profile_name(profile)}
  docker.run_internal(selected, if ci { "test-linux-ci" } else { "test-linux" }, true, [])
  docker.run_internal(selected, if ci { "test-linux-ci-split" } else { "test-linux-split" }, true, [])
}

## Runs the same filtered process and native gates on the selected Darwin target.
export proc macos_ci(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit, Error] {
  full(ctx, execution_profile()?)
}
