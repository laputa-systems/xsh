##! Build and non-mutating repository checks for the development lifecycle.
use context

## Prepares the native musl sysroot only on the Linux host/target combination that needs it.
export proc prepare_native_musl(ctx: Context) [fs, process, error] -> Result[Unit] {
  if ctx.host_os != "linux" or ! ctx.static_musl {
    return
  }

  let libc = /usr/lib/libc.so
  let libgcc = /usr/lib/libgcc_s.so.1

  if ! libc.exists()? or ! libgcc.exists()? {
    return
  }

  let sysroot_text: Str = run.text rustc --print sysroot ?
  let sysroot = fp"${sysroot_text.trim()}/lib/rustlib/${ctx.target_triple}/lib"
  fs.remove(fp"${sysroot}/libgcc_s.so", missing_ok: true)?
  fs.remove(fp"${sysroot}/libgcc_s.so.1", missing_ok: true)?
  fs.remove(fp"${sysroot}/libc.so", missing_ok: true)?
  fs.symlink(libgcc, fp"${sysroot}/libgcc_s.so")?
  fs.symlink(libgcc, fp"${sysroot}/libgcc_s.so.1")?
  fs.symlink(libc, fp"${sysroot}/libc.so")?
}

## Builds the repository with the current development Cargo profile.
export proc build(ctx: Context) [fs, process, error, io] -> Result[Unit] {
  prepare_native_musl(ctx)?
  context.run_stage("build", ctx.target_triple, "cargo", ["cargo", "build"], ctx.root, {})?
}

## Runs the repository's non-mutating deprecated-import contract.
export proc check_libxsh_imports(ctx: Context) [process, error] -> Result[Unit] {
  let pattern = "xsh::(source|symbol|syntax|sema|loader|runner|runtime|modules|parse_script_with_module_roots)"
  let result = run.capture --text rg -n $pattern crates/xshi/src crates/xsht/src crates/xsht/tests tests src/entrypoints --glob "*.rs" ?

  if result.stdout.trim() != "" {
    return Err(
      context.ContextError.StageFailed(
        stage: "check-libxsh-imports",
        target: ctx.target_triple,
        detail: "deprecated libxsh implementation import found",
      ),
    )
  }

  if result.status.ok or result.status.exited_with(1) {
    return
  }

  return Err(
    context.ContextError.StageFailed(
      stage: "check-libxsh-imports",
      target: ctx.target_triple,
      detail: f"rg exited ${result.status.exit_code()?}",
    ),
  )
}

## Runs the focused, source-non-mutating development check suite.
export proc check(ctx: Context) [fs, process, error, io] -> Result[Unit] {
  context.run_stage(
    "check-build",
    ctx.target_triple,
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
  )?
  context.run_stage(
    "check-rustfmt",
    ctx.target_triple,
    "cargo",
    ["cargo", "fmt", "--all", "--", "--check"],
    ctx.root,
    {},
  )?
  context.run_stage(
    "check-clippy",
    ctx.target_triple,
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
  )?
  let xsht = fp"${ctx.target_dir}/debug/xsht"
  context.run_stage("check-xsh", ctx.target_triple, xsht.display(), [xsht.display(), "check"], ctx.root, {})?
  context.run_stage(
    "check-xsh-fmt",
    ctx.target_triple,
    xsht.display(),
    [xsht.display(), "fmt", "--check"],
    ctx.root,
    {},
  )?
  context.run_stage("check-xsh-lint", ctx.target_triple, xsht.display(), [xsht.display(), "lint"], ctx.root, {})?
  context.run_stage(
    "check-runnable-corpus",
    ctx.target_triple,
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
  )?
  check_libxsh_imports(ctx)?
  context.run_stage("check-diff", ctx.target_triple, "git", ["git", "diff", "--check"], ctx.root, {})?
}

## Runs the repository-owner-only formatting and autofix workflow.
export proc lint_fix(ctx: Context) [process, error, io] -> Result[Unit] {
  context.run_stage("lint-rustfmt", ctx.target_triple, "cargo", ["cargo", "fmt", "--all"], ctx.root, {})?
  context.run_stage(
    "lint-clippy",
    ctx.target_triple,
    "cargo",
    ["cargo", "clippy", "--fix", "--allow-dirty", "--all-targets", "--all-features", "--quiet"],
    ctx.root,
    {},
  )?
  context.run_stage(
    "lint-build-xsht",
    ctx.target_triple,
    "cargo",
    ["cargo", "build", "-p", "xsht", "--bin", "xsht"],
    ctx.root,
    {},
  )?
  let xsht = fp"${ctx.target_dir}/debug/xsht"
  context.run_stage("lint-xsh", ctx.target_triple, xsht.display(), [xsht.display(), "lint", "--fix"], ctx.root, {})?
  context.run_stage("format-xsh", ctx.target_triple, xsht.display(), [xsht.display(), "fmt"], ctx.root, {})?
}
