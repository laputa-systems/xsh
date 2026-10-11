##! Changed-path verification plans and the complete integration gate.
use context
use docs as documentation
use docker
use stage as stages
use stage_contract as contract
use test_workflows as tests

## Gate families owned by the repository's testing contract.
export enum Area { Compile, Syntax, Grammar, Checker, Lowering, Runtime, Sugar, Tooling, Api, Stdlib, Memory, Docs, Automation, Core, Showcase, Soundness }

## A path that has no known verification owner.
export error GateError = UnmappedPath(path: Str)

## Selected gate families and exact changed native test modules.
export type Selection = {areas: List[Area], native_files: List[Str]}

## Determines the verification owner from repository-relative source paths.
export pure areas_for(changed_path: Str) -> Result[List[Area], Error] {
  return [] when changed_path.starts_with("dev/compat/")
  return [Syntax] when changed_path.starts_with("tests/fixtures/syntax/") or changed_path.starts_with("tests/fixtures/fmt/")
  return [Checker] when changed_path.starts_with("tests/fixtures/sema/")
  return [Runtime] when changed_path.starts_with("tests/fixtures/runtime/") or changed_path.starts_with("tests/fixtures/interactive-parity/")
  return [Memory] when changed_path.starts_with("tests/fixtures/frontend-indexed/")
  return [Api, Stdlib] when changed_path.starts_with("tests/fixtures/modules/")
  return Err(GateError.UnmappedPath(path: changed_path)) when changed_path.starts_with("tests/fixtures/")
  return [Syntax, Grammar, Docs] when changed_path == "src/syntax/grammar.rs" or changed_path.starts_with("src/syntax/grammar/")
  return [Syntax] when changed_path.starts_with("src/syntax/") or changed_path == "tests/syntax.rs" or changed_path == "tests/grammar.rs"
  return [Memory] when changed_path == "src/frontend_stats.rs"
  return [Checker] when changed_path.starts_with("src/sema/") or changed_path == "src/sema.rs" or changed_path == "tests/sema.rs" or changed_path == "src/loader.rs"
  return [Lowering] when changed_path.starts_with("src/runtime/eval/indexed/") or changed_path == "src/runtime/eval/indexed.rs" or changed_path.starts_with("src/runtime/eval/lower.") or changed_path.starts_with("src/runtime/eval/lower/")
  return [Runtime] when changed_path.starts_with("src/runtime/") or changed_path == "src/runtime.rs" or changed_path.starts_with("src/trace") or changed_path == "src/runner.rs" or changed_path.starts_with("tests/runtime/") or changed_path == "tests/runtime.rs"
  return [Sugar] when changed_path.starts_with("crates/xsht/src/sugar_") or changed_path.starts_with("crates/xsht/src/desugar")
  return [Tooling] when changed_path.starts_with("crates/xsht/src/") or changed_path.starts_with("crates/xsht/tests/")
  return [Api] when changed_path.starts_with("crates/xsh-registry/") or changed_path.starts_with("src/api/") or changed_path == "tests/libxsh_api.rs"
  return [Stdlib, Api] when changed_path == "src/stdlib.rs" or changed_path.starts_with("stdlib/") or changed_path.starts_with("src/modules/") or changed_path == "src/modules.rs"
  return [Docs, Api] when changed_path.starts_with("docs/snippets/api/")
  return [Docs] when changed_path.starts_with("docs/")
  return [Core] when changed_path.starts_with("core/")
  return [Showcase] when changed_path.starts_with("showcase/")
  return [Automation] when changed_path.starts_with("dev/") or changed_path.starts_with("tools/") or changed_path == "Makefile"
  return [Soundness] when changed_path.starts_with("crates/xsh-fuzz/")
  return [Runtime] when changed_path.starts_with("crates/xsh-net/") or changed_path == "tests/linux_priv.rs"
  return [Compile, Api] when changed_path == "Cargo.toml" or changed_path == "Cargo.lock" or changed_path == "build.rs" or changed_path == "rust-toolchain.toml" or changed_path.starts_with(".cargo/")
  return [Compile] when changed_path.starts_with("src/") or changed_path.starts_with("crates/") or changed_path.starts_with("tests/") or changed_path == "AGENTS.md" or changed_path == "TODO.md" or changed_path == "xsht-config.ini" or changed_path == "Dockerfile.test"
  return Err(GateError.UnmappedPath(path: changed_path))
}

## Deduplicates gates while retaining the order in which their owners appear.
export pure select(paths: List[Str]) -> Result[Selection, Error] {
  var areas: List[Area] = []
  var native_files: List[Str] = []
  for changed_path in paths {
    if ! changed_path.starts_with("tests/fixtures/") and changed_path.ends_with(".xsh") and (changed_path.starts_with("tests/") or changed_path.starts_with("dev/tests/") or changed_path.starts_with("core/tests/") or changed_path.starts_with("showcase/tests/")) {
      native_files += [changed_path] when changed_path not in native_files
      continue
    }
    for area in areas_for(changed_path)? {
      areas += [area] when area not in areas
    }
  }
  {areas: areas, native_files: native_files}
}

pure rust_gate(ctx: context.Context, profile: tests.TestProfile, name: Str, args: List[Str], debug: Bool = false) -> Result[contract.CommandSpec, Error] {
  let profile_args = if debug { [] } else { ["--profile", tests.profile_name(profile)] }
  stages.command(name, ctx.target.triple, "cargo",
    ["cargo", "test", "--locked", "--target", ctx.target.triple, @profile_args, @args],
    ctx.root, tests.cargo_environment(ctx)?)
}

pure native_gate(ctx: context.Context, profile: tests.TestProfile, filter: Str) -> contract.CommandSpec {
  let xsht = tests.product_path(ctx, profile, "xsht").display()
  stages.command(f"native-{filter}", ctx.target.triple, xsht, [xsht, "test", filter], ctx.root, {})
}

## Maps each selected area to its focused compiler or Rust gate and native
## behavioral module. No area runs the whole native corpus.
export pure lane_plan(ctx: context.Context, profile: tests.TestProfile, selection: Selection) -> Result[List[contract.CommandSpec], Error] {
  let root_target = ["-p", "xsh", "--test", "integration"]
  let xsht_target = ["-p", "xsht", "--test", "integration"]
  let root_lib = ["-p", "xsh", "--lib"]
  var plan = [tests.product_build(ctx, profile)?]
  for area in selection.areas {
    match area {
      Compile => plan += [stages.command("check-compile", ctx.target.triple, "cargo",
        ["cargo", "check", "--locked", "--target", ctx.target.triple, "-p", "xsh", "-p", "xsht", "-p", "xsh-fuzz", "--all-targets"],
        ctx.root, tests.cargo_environment(ctx)?)],
      Syntax => plan += [rust_gate(ctx, profile, "test-syntax", [@root_target, "syntax::", "--", @tests.sibling_syntax_exclusions])?, native_gate(ctx, profile, "tests/xsh/formatter.xsh")],
      Grammar => plan += [rust_gate(ctx, profile, "test-grammar", ["-p", "xsh-fuzz", "--test", "soundness", "generated_programs_are_grammar_sentences"])?, native_gate(ctx, profile, "tests/xsh/tooling-commands.xsh")],
      Checker => plan += [rust_gate(ctx, profile, "test-sema", [@root_target, "sema::"])?, native_gate(ctx, profile, "tests/xsh/checker-")],
      Lowering => plan += [rust_gate(ctx, profile, "test-lowering", [@root_lib, "runtime::eval"], debug: true)?, rust_gate(ctx, profile, "test-type-agreement", [@root_lib, "corpus_lowering_agrees_with_checked_types"], debug: true)?, native_gate(ctx, profile, "tests/xsh/lowering-coverage.xsh")],
      Runtime => plan += [rust_gate(ctx, profile, "test-runtime", [@root_target, "runtime::", "--", "--skip", "runtime::coverage::", "--skip", "runtime::examples::"])?, native_gate(ctx, profile, "tests/xsh/lowering-coverage.xsh")],
      Sugar => plan += [rust_gate(ctx, profile, "test-sugar", ["-p", "xsht", "--lib", "sugar_expansion_tests"], debug: true)?, rust_gate(ctx, profile, "test-desugar-unit", ["-p", "xsht", "--lib", "desugar_tests"], debug: true)?, rust_gate(ctx, profile, "test-desugar", [@xsht_target, "desugar::"])?, native_gate(ctx, profile, "tests/xsh/desugar.xsh")],
      Tooling => plan += [rust_gate(ctx, profile, "test-tooling", [@xsht_target, "--", @tests.sibling_xsht_exclusions])?, native_gate(ctx, profile, "tests/xsh/tooling-")],
      Api => plan += [rust_gate(ctx, profile, "test-libxsh-api", [@root_target, "libxsh_api"])?, rust_gate(ctx, profile, "test-registry", ["-p", "xsh-registry", "--lib"], debug: true)?, rust_gate(ctx, profile, "test-signatures", [@root_lib, "modules::signature"], debug: true)?, rust_gate(ctx, profile, "test-api", [@xsht_target, "api::"])?, native_gate(ctx, profile, "tests/xsh/api-tool.xsh"), stages.command("check-api-snippets", ctx.target.triple, tests.product_path(ctx, profile, "xsht").display(), [tests.product_path(ctx, profile, "xsht").display(), "check", "docs/snippets/api"], ctx.root, {})],
      Stdlib => plan += [rust_gate(ctx, profile, "test-stdlib-catalog", [@root_lib, "stdlib::tests::every_catalog_module_parses_checks_and_lowers"], debug: true)?, native_gate(ctx, profile, "tests/xsh/stdlib")],
      Memory => plan += [rust_gate(ctx, profile, "test-frontend-memory", [@root_lib, "frontend_stats::tests"], debug: true)?, stages.command("frontend-memory-report", ctx.target.triple, tests.product_path(ctx, profile, "xsht").display(), [tests.product_path(ctx, profile, "xsht").display(), "frontend-stats", "--json", "tests/fixtures/frontend-indexed"], ctx.root, {})],
      Docs => {},
      Automation => plan += [native_gate(ctx, profile, "dev/tests")],
      Core => plan += [native_gate(ctx, profile, "core/tests")],
      Showcase => plan += [native_gate(ctx, profile, "showcase/tests")],
      Soundness => plan += [rust_gate(ctx, profile, "test-soundness", ["-p", "xsh-fuzz", "--test", "soundness"]) ?],
    }
  }
  for file in selection.native_files {
    plan += [native_gate(ctx, profile, file)]
  }
  plan
}

proc docs_gate(ctx: context.Context, profile: tests.TestProfile) [fs, process, env, error, io] -> Result[Unit, Error] {
  documentation.check(ctx.root, documentation.DocTools(
    xsh: tests.product_path(ctx, profile, "xsh"), xsht: tests.product_path(ctx, profile, "xsht")))
}

## Automatic repository checks exclude the compatibility directory while its
## sources are under active change. Explicit compatibility tools remain available.
export pure batch_checks(ctx: context.Context, profile: tests.TestProfile) -> List[contract.CommandSpec] {
  let xsht = tests.product_path(ctx, profile, "xsht").display()
  let xsh = tests.product_path(ctx, profile, "xsh").display()
  [
    stages.command("batch-check", ctx.target.triple, xsht, [xsht, "check"], ctx.root, {}),
    stages.command("batch-lint", ctx.target.triple, xsht, [xsht, "lint"], ctx.root, {}),
    stages.command("batch-diff", ctx.target.triple, "git", ["git", "diff", "--check"], ctx.root, {}),
    stages.command("batch-ratchets", ctx.target.triple, xsh,
      [xsh, "dev/consolidation/metrics.xsh", "check", "--root", ctx.root.display(),
        "--source-root", "src", "--xsht-source-root", "crates/xsht/src",
        "--baseline", "dev/consolidation/baseline.json", "--envelope", "dev/consolidation/envelope.json",
        "--output", ".work/consolidation/current.json", "--audit", ".work/consolidation/rises.json"], ctx.root, {}),
  ]
}

## Runs the complete sequence at an integration boundary without source-tree
## formatters, autofixers, or paused compatibility ratchets.
export proc batch(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit, Error] {
  let profile = tests.execution_profile()?
  tests.full(ctx, profile, privileged: ctx.target.os == "linux")
  tests.run_commands(batch_checks(ctx, profile))
  docs_gate(ctx, profile)
}

## Linux public gates enter the pinned container; internal operations call the
## plans directly, so there is no recursive Docker dispatch inside the image.
export proc execute(ctx: context.Context, requested: List[Str]) [fs, process, env, error, io] -> Result[Unit, Error] {
  if ctx.target.os == "linux" {
    let profile = tests.execution_profile()?
    let selected = {...ctx, profile: tests.profile_name(profile)}
    let batch_requested = requested == ["--batch"]
    return docker.run_internal(selected, if batch_requested { "gate-batch" } else { "gate-lane" },
      batch_requested, if batch_requested { [] } else { requested })
  }
  return batch(ctx) when requested == ["--batch"]

  lane(ctx, requested)
}

## Uses explicit changed paths, or Git's tracked and untracked changes when
## none are supplied. Unknown paths fail before a compiler or test is started.
export proc lane(ctx: context.Context, requested: List[Str]) [fs, process, env, error, io] -> Result[Unit, Error] {
  var paths = requested
  if paths.is_empty() {
    let tracked = run.text git diff --name-only HEAD
    let untracked = run.text git ls-files --others --exclude-standard
    paths = [@tracked.lines(), @untracked.lines()] |> where . != ""
  }
  let selection = select(paths)?
  if selection.areas.is_empty() and selection.native_files.is_empty() {
    print "no active verification owners selected"
    return
  }
  let profile = tests.execution_profile()?
  tests.run_commands(lane_plan(ctx, profile, selection)?)
  docs_gate(ctx, profile) when Docs in selection.areas
}
