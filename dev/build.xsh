##! Build and non-mutating repository checks for the development lifecycle.
use cargo_steps
use context
use docs as documentation
use stage as stages
use targets
use test_workflows as tests

## Prepares the native musl sysroot only on the Linux host/target combination that needs it.
export proc prepare_native_musl(ctx: context.Context) [fs, process, error] -> Result[Unit, Error] {
  return when ctx.host_os != .Linux or ! ctx.target.static_musl

  let libc = /usr/lib/libc.so
  let libgcc = /usr/lib/libgcc_s.so.1

  return when ! libc.exists() or ! libgcc.exists()

  let sysroot_text: Str = run.text rustc --print sysroot
  let sysroot = fp"{sysroot_text.trim()}/lib/rustlib/{ctx.target.triple}/lib"
  fp"{sysroot}/libgcc_s.so".remove()
  fp"{sysroot}/libgcc_s.so.1".remove()
  fp"{sysroot}/libc.so".remove()
  fp"{sysroot}/libgcc_s.so".symlink(to: libgcc)
  fp"{sysroot}/libgcc_s.so.1".symlink(to: libgcc)
  fp"{sysroot}/libc.so".symlink(to: libc)
}

## Builds the repository with the current development Cargo profile.
export proc build(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit, Error] {
  prepare_native_musl(ctx)
  cargo_steps.run_build(
    stages.command(
      "build",
      ctx.target.triple,
      "cargo",
      ["cargo", "build"],
      ctx.root,
      {},
    ),
  )
}

## Runs the repository's non-mutating deprecated-import contract.
export proc check_libxsh_imports(ctx: context.Context) [process, error] -> Result[Unit, Error] {
  let pattern = "xsh::(source|symbol|syntax|sema|loader|runner|runtime|modules|parse_script_with_module_roots)"
  let result = run.capture --text rg -n $pattern crates/xshi/src crates/xsht/src crates/xsht/tests tests \
    src/entrypoints --glob "*.rs"

  if result.stdout.trim() != "" {
    return Err(
      stages.StageError.Failed(
        stage: "check-libxsh-imports",
        target: ctx.target.triple,
        detail: "deprecated libxsh implementation import found",
      ),
    )
  }

  return when result.status.ok or result.status.exited_with(1)

  return Err(
    stages.StageError.Failed(
      stage: "check-libxsh-imports",
      target: ctx.target.triple,
      detail: f"rg exited {result.status.exit_code()?}",
    ),
  )
}

## Checks command inventory, native test origins and applet invariants through
## the release native test runner. Metadata checks need no upstream checkout.
export proc check_core_contracts(ctx: context.Context) [process, error, io] -> Result[Unit, Error] {
  let xsht = fp"{ctx.target_dir}/{ctx.target.triple}/release/xsht"
  stages.execute(
    stages.command(
      "check-core-contracts",
      ctx.target.triple,
      xsht.display(),
      [xsht.display(), "test", "-j", "1", "dev/tests/test-repository-contracts.xsh"],
      ctx.root,
      {},
    ),
  )
}

## Guards the repository lint budget test with host readiness checks before core and docs checks.
export proc check_lint(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit, Error] {
  let test_spec = tests.repository_lint_budget_test(ctx)?
  let build_spec = {
    ...test_spec,
    stage: "check-lint-build",
    argv: cargo_steps.test_build_argv(test_spec.argv),
  }
  cargo_steps.run_build(build_spec)
  stages.execute(stages.command(
    "check-lint-ready",
    ctx.target.triple,
    "perf-gate-ready",
    ["perf-gate-ready"],
    ctx.root,
    {},
  ))
  let test_result = stages.execute(test_spec)
  let post_result = stages.execute(stages.command(
    "check-lint-ready-after",
    ctx.target.triple,
    "perf-gate-ready",
    ["perf-gate-ready", "-1"],
    ctx.root,
    {},
  ))
  if let Err(test_error) = test_result {
    if let Err(post_error) = post_result {
      eprint f"post-readiness check failed after lint test failure: {post_error.message}"
    }
    return Err(test_error)
  }
  post_result?
  check_core_contracts(ctx)
  documentation.build_release(ctx)
  check_docs(ctx, documentation.release_tools(ctx))
}

## Fails on stale generated docs, failing tour snippets, or failing tour project tests.
export proc check_docs(
  ctx: context.Context,
  tools: documentation.DocTools,
) [fs, process, env, error, io] -> Result[Unit, Error] {
  print f"[check-docs target={ctx.target.triple}] render docs/templates and compare"
  documentation.check(ctx.root, tools)
}

## Runs the focused, source-non-mutating development check suite. Its tools
## and tests run release binaries, as every test does.
export proc check(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit, Error] {
  cargo_steps.run_build(
    stages.command(
      "check-build",
      ctx.target.triple,
      "cargo",
      [
        "cargo",
        "build",
        "--release",
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
      tests.cargo_environment(ctx)?,
    ),
  )
  stages.execute(
    stages.command(
      "check-rustfmt",
      ctx.target.triple,
      "cargo",
      ["cargo", "fmt", "--all", "--", "--check"],
      ctx.root,
      tests.cargo_environment(ctx)?,
    ),
  )
  cargo_steps.run_build(
    stages.command(
      "check-clippy",
      ctx.target.triple,
      "cargo",
      [
        "cargo",
        "clippy",
        "--all-targets",
        "--all-features",
        "--quiet",
        "--",
        "-D",
        "warnings",
      ],
      ctx.root,
      tests.cargo_environment(ctx)?,
    ),
  )
  let xsht = fp"{ctx.target_dir}/{ctx.target.triple}/release/xsht"
  stages.execute(
    stages.command(
      "check-xsh",
      ctx.target.triple,
      xsht.display(),
      [xsht.display(), "check"],
      ctx.root,
      tests.cargo_environment(ctx)?,
    ),
  )
  stages.execute(
    stages.command(
      "check-xsh-fmt",
      ctx.target.triple,
      xsht.display(),
      [xsht.display(), "fmt", "--check"],
      ctx.root,
      tests.cargo_environment(ctx)?,
    ),
  )
  stages.execute(
    stages.command(
      "check-xsh-lint",
      ctx.target.triple,
      xsht.display(),
      [xsht.display(), "lint"],
      ctx.root,
      tests.cargo_environment(ctx)?,
    ),
  )
  check_core_contracts(ctx)
  check_docs(ctx, documentation.release_tools(ctx))
  check_libxsh_imports(ctx)
  stages.execute(
    stages.command(
      "check-diff",
      ctx.target.triple,
      "git",
      ["git", "diff", "--check"],
      ctx.root,
      tests.cargo_environment(ctx)?,
    ),
  )
}

## Runs the repository-owner-only formatting and autofix workflow.
export proc lint_fix(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit, Error] {
  stages.execute(
    stages.command(
      "lint-rustfmt",
      ctx.target.triple,
      "cargo",
      ["cargo", "fmt", "--all"],
      ctx.root,
      {},
    ),
  )
  cargo_steps.run_build(
    stages.command(
      "lint-clippy",
      ctx.target.triple,
      "cargo",
      ["cargo", "clippy", "--fix", "--allow-dirty", "--all-targets", "--all-features", "--quiet"],
      ctx.root,
      {},
    ),
  )
  cargo_steps.run_build(
    stages.command(
      "lint-build-xsh",
      ctx.target.triple,
      "cargo",
      ["cargo", "build", "-p", "xsh", "--bin", "xsh"],
      ctx.root,
      {},
    ),
  )
  cargo_steps.run_build(
    stages.command(
      "lint-build-xsht",
      ctx.target.triple,
      "cargo",
      ["cargo", "build", "-p", "xsht", "--bin", "xsht"],
      ctx.root,
      {},
    ),
  )
  let xsht = fp"{ctx.target_dir}/debug/xsht"
  stages.execute(
    stages.command(
      "lint-xsh",
      ctx.target.triple,
      xsht.display(),
      [xsht.display(), "lint", "--fix"],
      ctx.root,
      {},
    ),
  )
  stages.execute(
    stages.command(
      "format-xsh",
      ctx.target.triple,
      xsht.display(),
      [xsht.display(), "fmt"],
      ctx.root,
      {},
    ),
  )
}
