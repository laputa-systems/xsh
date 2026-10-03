##! Build and non-mutating repository checks for the development lifecycle.
use context
use docs as documentation
use stage as stages
use targets

## Prepares the native musl sysroot only on the Linux host/target combination that needs it.
export proc prepare_native_musl(ctx: context.Context) [fs, process, error] -> Result[Unit] {
  return when ctx.host_os != targets.Linux or ! ctx.target.static_musl

  let libc = /usr/lib/libc.so
  let libgcc = /usr/lib/libgcc_s.so.1

  return when ! libc.exists()? or ! libgcc.exists()?

  let sysroot_text: Str = run.text rustc --print sysroot ?
  let sysroot = fp"${sysroot_text.trim()}/lib/rustlib/${ctx.target.triple}/lib"
  fs.remove(fp"${sysroot}/libgcc_s.so", missing_ok: true)?
  fs.remove(fp"${sysroot}/libgcc_s.so.1", missing_ok: true)?
  fs.remove(fp"${sysroot}/libc.so", missing_ok: true)?
  fs.symlink(libgcc, fp"${sysroot}/libgcc_s.so")?
  fs.symlink(libgcc, fp"${sysroot}/libgcc_s.so.1")?
  fs.symlink(libc, fp"${sysroot}/libc.so")?
}

## Builds the repository with the current development Cargo profile.
export proc build(ctx: context.Context) [fs, process, error, io] -> Result[Unit] {
  prepare_native_musl(ctx)?
  stages.execute(
    stages.command(
      "build",
      ctx.target.triple,
      "cargo",
      ["cargo", "build"],
      ctx.root,
      {},
    ),
  )?
}

## Runs the repository's non-mutating deprecated-import contract.
export proc check_libxsh_imports(ctx: context.Context) [process, error] -> Result[Unit] {
  let pattern = "xsh::(source|symbol|syntax|sema|loader|runner|runtime|modules|parse_script_with_module_roots)"
  let result = run.capture --text rg -n $pattern crates/xshi/src crates/xsht/src crates/xsht/tests tests src/entrypoints --glob "*.rs" ?

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
      detail: f"rg exited ${result.status.exit_code()?}",
    ),
  )
}

## Compiles the release lint gate before measuring read-only lint on the configured repository corpus,
## then checks generated docs with release binaries.
export proc check_lint(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit] {
  stages.execute(
    stages.command(
      "check-lint",
      ctx.target.triple,
      "cargo",
      [
        "cargo",
        "test",
        "--release",
        "-p",
        "xsht",
        "--test",
        "integration",
        "lint_performance::",
        "--",
        "--test-threads=1",
        "--nocapture",
      ],
      ctx.root,
      {},
    ),
  )?
  documentation.build_release(ctx)?
  check_docs(ctx, documentation.release_tools(ctx))?
}

## Fails on stale generated docs, failing tour snippets, or failing tour project tests.
export proc check_docs(
  ctx: context.Context,
  tools: documentation.DocTools,
) [fs, process, env, error, io] -> Result[Unit] {
  print f"[check-docs target=${ctx.target.triple}] render docs/templates and compare"
  documentation.check(ctx.root, tools)?
}

## Runs the focused, source-non-mutating development check suite.
export proc check(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit] {
  stages.execute(
    stages.command(
      "check-build",
      ctx.target.triple,
      "cargo",
      [
        "cargo",
        "build",
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
      {},
    ),
  )?
  stages.execute(
    stages.command(
      "check-rustfmt",
      ctx.target.triple,
      "cargo",
      ["cargo", "fmt", "--all", "--", "--check"],
      ctx.root,
      {},
    ),
  )?
  stages.execute(
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
      {},
    ),
  )?
  let xsht = fp"${ctx.target_dir}/debug/xsht"
  stages.execute(
    stages.command(
      "check-xsh",
      ctx.target.triple,
      xsht.display(),
      [xsht.display(), "check"],
      ctx.root,
      {},
    ),
  )?
  stages.execute(
    stages.command(
      "check-xsh-fmt",
      ctx.target.triple,
      xsht.display(),
      [xsht.display(), "fmt", "--check"],
      ctx.root,
      {},
    ),
  )?
  stages.execute(
    stages.command(
      "check-xsh-lint",
      ctx.target.triple,
      xsht.display(),
      [xsht.display(), "lint"],
      ctx.root,
      {},
    ),
  )?
  stages.execute(
    stages.command(
      "check-runnable-corpus",
      ctx.target.triple,
      "cargo",
      [
        "cargo",
        "test",
        "--test",
        "integration",
        "runtime::coverage::runnable_xsh_corpus_is_formatted_and_lints_without_warnings",
      ],
      ctx.root,
      {},
    ),
  )?
  check_docs(ctx, documentation.debug_tools(ctx))?
  check_libxsh_imports(ctx)?
  stages.execute(
    stages.command(
      "check-diff",
      ctx.target.triple,
      "git",
      ["git", "diff", "--check"],
      ctx.root,
      {},
    ),
  )?
}

## Runs the repository-owner-only formatting and autofix workflow.
export proc lint_fix(ctx: context.Context) [process, error, io] -> Result[Unit] {
  stages.execute(
    stages.command(
      "lint-rustfmt",
      ctx.target.triple,
      "cargo",
      ["cargo", "fmt", "--all"],
      ctx.root,
      {},
    ),
  )?
  stages.execute(
    stages.command(
      "lint-clippy",
      ctx.target.triple,
      "cargo",
      ["cargo", "clippy", "--fix", "--allow-dirty", "--all-targets", "--all-features", "--quiet"],
      ctx.root,
      {},
    ),
  )?
  stages.execute(
    stages.command(
      "lint-build-xsh",
      ctx.target.triple,
      "cargo",
      ["cargo", "build", "-p", "xsh", "--bin", "xsh"],
      ctx.root,
      {},
    ),
  )?
  stages.execute(
    stages.command(
      "lint-build-xsht",
      ctx.target.triple,
      "cargo",
      ["cargo", "build", "-p", "xsht", "--bin", "xsht"],
      ctx.root,
      {},
    ),
  )?
  let xsht = fp"${ctx.target_dir}/debug/xsht"
  stages.execute(
    stages.command(
      "lint-xsh",
      ctx.target.triple,
      xsht.display(),
      [xsht.display(), "lint", "--fix"],
      ctx.root,
      {},
    ),
  )?
  stages.execute(
    stages.command(
      "format-xsh",
      ctx.target.triple,
      xsht.display(),
      [xsht.display(), "fmt"],
      ctx.root,
      {},
    ),
  )?
}
