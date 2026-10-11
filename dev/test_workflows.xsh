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

## The external lint comparison copies and rewrites the entire sibling corpus,
## so automatic tooling verification excludes that one expensive test.
export const sibling_xsht_exclusions = ["--skip", "lint_format_invariance::formatting_preserves_lints_on_laputa_corpus"]

## The only profiles accepted by process and native tests.
export enum TestProfile { Verification, Release }

## An explicitly requested profile that cannot run the test contract.
export error TestProfileError = Unsupported(profile: Str)

## Converts the closed test profile into Cargo's profile directory name.
export pure profile_name(profile: TestProfile) -> Str {
  match profile {
    Verification => "verification"
    Release => "release"
  }
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
export pure product_path(ctx: context.Context, profile: TestProfile, name: Str) -> Path {
  fp"{ctx.target_dir}/{ctx.target.triple}/{profile_name(profile)}/{name}"
}

## Every Linux command uses the container's pinned static musl link contract.
export pure cargo_environment(ctx: context.Context) -> Result[Record, Error] {
  return targets.docker_test_env(ctx.target.triple) when ctx.target.os == "linux"

  {MACOSX_DEPLOYMENT_TARGET: ctx.darwin_deployment_target}
}

## Builds every product or helper that a process test may execute.
export pure product_build(ctx: context.Context, profile: TestProfile) -> Result[contract.CommandSpec, Error] {
  stages.command(
    "test-products", ctx.target.triple, "cargo",
    ["cargo", "build", "--locked", "--profile", profile_name(profile), "--target", ctx.target.triple,
      "-p", "xsh", "--bins", "-p", "xsht", "--bin", "xsht", "-p", "xshi", "--bin", "xshi"],
    ctx.root, cargo_environment(ctx)?,
  )
}

## Specifies process boundaries once so ordinary tests and coverage run the
## same targets. The native corpus is a separate step, avoiding a second full
## run through the root integration wrapper.
export pure rust_plan(
  ctx: context.Context,
  profile: TestProfile,
  privileged: Bool = false,
) -> Result[List[contract.CommandSpec], Error] {
  let environment = cargo_environment(ctx)?
  let profile_args = ["--locked", "--profile", profile_name(profile), "--target", ctx.target.triple]
  let debug_args = ["--locked", "--target", ctx.target.triple]
  var plan = [
    product_build(ctx, profile)?,
    stages.command("test-rust", ctx.target.triple, "cargo",
      ["cargo", "test", @profile_args, "-p", "xsh", "--test", "integration", "--test", "ambient_fs_policy",
        "--test", "symbol_plateau", "--", "--skip", "runtime::coverage::", "--skip", "runtime::examples::example_corpus_is_formatted",
        "--skip", "runtime::examples::example_corpus_lints_without_warnings", @sibling_syntax_exclusions],
      ctx.root, environment),
    stages.command("test-debug-assertions", ctx.target.triple, "cargo",
      ["cargo", "test", @debug_args, "-p", "xsh", "--lib"], ctx.root, environment),
    stages.command("test-xsht", ctx.target.triple, "cargo",
      ["cargo", "test", @profile_args, "-p", "xsht", "--test", "integration", "--", @sibling_xsht_exclusions], ctx.root, environment),
    stages.command("test-soundness", ctx.target.triple, "cargo",
      ["cargo", "test", @profile_args, "-p", "xsh-fuzz", "--test", "soundness"], ctx.root, environment),
  ]
  if privileged {
    plan += [stages.command("test-linux-priv", ctx.target.triple, "cargo",
      ["cargo", "test", @profile_args, "-p", "xsh", "--features", "linux-priv-tests", "--test", "linux_priv",
        "--", "--nocapture"], ctx.root, environment)]
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
) [process, error, io] -> Result[Unit, Error] {
  let xsht = product_path(ctx, profile, "xsht")
  stages.execute(stages.command("test-xsh", ctx.target.triple, xsht.display(),
    [xsht.display(), "test", @filters], ctx.root, {}))
}

## Runs process and debug-assertion gates, followed by one full native suite.
export proc full(
  ctx: context.Context,
  profile: TestProfile,
  privileged: Bool = false,
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
  run_commands([product_build(ctx, profile)?])
  native(ctx, profile, [])
}

## Carries the selected test profile into the direct Docker command explicitly.
export proc linux_test(ctx: context.Context, ci: Bool) [fs, process, env, error, io] -> Result[Unit, Error] {
  let profile = execution_profile()?
  let selected = {...ctx, profile: profile_name(profile)}
  docker.run_internal(selected, if ci { "test-linux-ci" } else { "test-linux" }, true, [])
}

## Runs the same filtered process and native gates on the selected Darwin target.
export proc macos_ci(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit, Error] {
  full(ctx, execution_profile()?)
}
