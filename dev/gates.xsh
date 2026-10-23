##! Changed-path verification plans and the complete integration gate.
use context
use cargo_steps
use gate_execution
use docker
use docs as documentation
use stage as stages
use stage_contract as contract
use test_workflows as tests

## Gate families owned by the repository's testing contract.
export enum Area { Compile, Syntax, Grammar, Checker, Lowering, Runtime, RouteWitness, Sugar, Tooling, Api, Stdlib, Memory, Docs, Automation, Core, Showcase, Soundness }

## A path that has no known verification owner.
export error GateError = UnmappedPath(path: Str)

## Selected gate families and exact changed native test modules.
export type Selection = {areas: List[Area], native_files: List[Str]}

## Automatic tiers exclude full verification authority.
export enum Tier { Edit, Change, Scenario }

## Commands and aliases cannot grant a manual full request.
export enum CaseExecution { RunCommand(contract.CommandSpec), DocumentationCheck, Alias, ManualFull }

## A declared case owns its command, prerequisites, products and bounded scope.
export type CatalogCase = {
  id: Str,
  tier: Tier,
  timeout_seconds: Int,
  products: List[tests.Product],
  prerequisite_ids: List[Str],
  reasons: List[Str],
  execution: CaseExecution,
}

## A focused plan describes selected cases and unresolved scope.
export type CatalogPlan = {
  tier: Tier,
  cases: List[CatalogCase],
  products: List[tests.Product],
  unresolved: List[Str],
}

enum OutputMode { Execute, InspectText, InspectJson }

enum GateRequest { Focused(Tier, List[Str], List[Str], OutputMode), Full }

## Invalid requests and dependency closures fail before process execution.
export error PlanError = Invalid(detail: Str)

## Compilation and leaf execution remain distinct process boundaries.
export enum CaseAction { Prepare, ExecuteLeaf }

## Focused execution retains raw child status and structured failure causes.
export error ExecutionError = Failed(case_id: Str, raw_status: Int) | TimedOut(case_id: Str, cause: ProcessError) | Cancelled(case_id: Str, raw_status: Int) | Interrupted(case_id: Str, cause: ProcessError) | ProcessFailed(case_id: Str, cause: ProcessError) | BudgetExpired(case_id: Str) | Incomplete(case_id: Str, raw_status: Int)

## Only the sole literal --full requests comprehensive verification.
export pure parse_request(arguments: List[Str]) -> Result[GateRequest, Error] {
  return Full when arguments == ["--full"]
  var tier = Change
  var paths: List[Str] = []
  var case_ids: List[Str] = []
  var inspect = false
  var machine = false
  var index = 0
  while index < arguments.len() {
    let arg = arguments[index]
    match arg {
      "--plan" => inspect = true
      "--json" => machine = true
      "--tier" => {
        return Err(PlanError.Invalid(detail: "--tier requires edit or change")) when index + 1 >= arguments.len()
        index += 1
        tier = match arguments[index] {
          "edit" => Edit
          "change" => Change
          else => { return Err(PlanError.Invalid(detail: "--tier accepts only edit or change")) }
        }
      }
      "--case" => {
        return Err(PlanError.Invalid(detail: "--case requires an ID")) when index + 1 >= arguments.len()
        index += 1
        let id = arguments[index]
        return Err(PlanError.Invalid(detail: "--case requires a non-option ID")) when id == "" or id.starts_with("-")
        return Err(PlanError.Invalid(detail: f"duplicate case ID {id}")) when id in case_ids
        case_ids += [id]
      }
      else => {
        return Err(PlanError.Invalid(detail: f"unsupported gate option {arg}")) when arg.starts_with("-")
        paths += [arg]
      }
    }
    index += 1
  }
  return Err(PlanError.Invalid(detail: "--json requires --plan")) when machine and ! inspect
  let output = if machine { InspectJson } else if inspect { InspectText } else { Execute }
  Focused(tier, paths, case_ids, output)
}

pure resolve_case(catalogue: List[CatalogCase], id: Str, explicit: List[Str], visiting: List[Str], completed: List[CatalogCase]) -> Result[List[CatalogCase], Error] {
  return Err(PlanError.Invalid(detail: f"catalogue cycle at {id}")) when id in visiting
  return completed when id in [entry.id for entry in completed]
  let matches = [entry for entry in catalogue |> where .id == id]
  return Err(PlanError.Invalid(detail: f"case {id} has {matches.len()} declarations")) when matches.len() != 1
  let entry = matches[0]
  match entry.execution {
    ManualFull => { return Err(PlanError.Invalid(detail: f"manual case {id} requires sole --full")) }
    RunCommand(_) | DocumentationCheck | Alias => {}
  }
  return Err(PlanError.Invalid(detail: f"scenario {id} requires explicit --case")) when entry.tier == Scenario and id not in explicit
  return Err(PlanError.Invalid(detail: f"case {id} has no timeout")) when entry.timeout_seconds <= 0
  var resolved = completed
  for dependency in entry.prerequisite_ids {
    resolved = resolve_case(catalogue, dependency, explicit, [@visiting, id], resolved)?
  }
  [@resolved, entry]
}

## Resolves the entire focused dependency closure before returning commands.
export pure resolve_plan(catalogue: List[CatalogCase], ids: List[Str], tier: Tier) -> Result[CatalogPlan, Error] {
  resolve_selection(catalogue, ids, ids, tier)
}

pure resolve_selection(catalogue: List[CatalogCase], ids: List[Str], explicit_ids: List[Str], tier: Tier) -> Result[CatalogPlan, Error] {
  var resolved: List[CatalogCase] = []
  for id in ids {
    resolved = resolve_case(catalogue, id, explicit_ids, [], resolved)?
  }
  var products: List[tests.Product] = []
  for entry in resolved {
    return Err(PlanError.Invalid(detail: f"case {entry.id} requires Change; Edit cannot escalate")) when tier == Edit and entry.tier == Change
    for product in entry.products {
      products += [product] when product not in products
    }
  }
  {tier: tier, cases: resolved, products: products, unresolved: []}
}

const cargo_caller_native_files = [
  "dev/tests/test-cargo-steps.xsh",
  "dev/tests/test-lifecycle.xsh",
  "dev/tests/test-gates.xsh",
  "dev/tests/test-native-workflows.xsh",
]

const current_metric_native_files = [
  "dev/tests/test-consolidation-metrics-current.xsh",
  "dev/tests/test-consolidation-ratchets-current.xsh",
]

pure cargo_caller_tests(changed_path: Str) -> List[Str] {
  match changed_path {
    "dev/cargo_steps.xsh" => [
      "dev/tests/test-cargo-steps.xsh",
      "dev/tests/test-lifecycle.xsh",
    ]
    "dev/cargo_test_runner.sh" => ["dev/tests/test-cargo-steps.xsh"]
    "dev/test_workflows.xsh" => cargo_caller_native_files
    else => []
  }
}

## Determines the verification owner from repository-relative source paths.
export pure areas_for(changed_path: Str) -> Result[List[Area], Error] {
  return [Syntax] when changed_path.starts_with("tests/fixtures/syntax/") or changed_path.starts_with("tests/fixtures/fmt/")
  return [Checker] when changed_path.starts_with("tests/fixtures/sema/")
  return [Runtime] when changed_path.starts_with("tests/fixtures/runtime/") or changed_path.starts_with("tests/fixtures/interactive-parity/")
  return [Memory] when changed_path.starts_with("tests/fixtures/frontend-indexed/")
  return [Api, Stdlib] when changed_path.starts_with("tests/fixtures/modules/")
  return Err(GateError.UnmappedPath(path: changed_path)) when changed_path.starts_with("tests/fixtures/")
  return [Syntax, Grammar, Docs] when changed_path == "src/syntax/grammar.rs" or changed_path.starts_with("src/syntax/grammar/")
  return [Syntax] when changed_path.starts_with("src/syntax/") or changed_path == "tests/grammar.rs" or changed_path == "crates/xsht/tests/syntax.rs"
  return [Memory] when changed_path == "src/frontend_stats.rs"
  return [Checker] when changed_path.starts_with("src/sema/") or changed_path == "src/sema.rs" or changed_path == "tests/sema.rs" or changed_path == "src/loader.rs"
  return [Lowering] when changed_path.starts_with("src/runtime/eval/indexed/") or changed_path == "src/runtime/eval/indexed.rs" or changed_path.starts_with("src/runtime/eval/lower.") or changed_path.starts_with("src/runtime/eval/lower/")
  return [RouteWitness] when changed_path == "src/runtime/eval/route_witness.rs"
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
  Err(GateError.UnmappedPath(path: changed_path))
}

## Deduplicates gates while retaining the order in which their owners appear.
export pure select(paths: List[Str]) -> Result[Selection, Error] {
  var areas: List[Area] = []
  var native_files = []
  for changed_path in paths {
    if changed_path == "dev/consolidation_metrics_current.xsh" or changed_path == "dev/consolidation/metric-observer-successor.json" or changed_path in current_metric_native_files {
      for file in current_metric_native_files {
        native_files += [file] when file not in native_files
      }
      continue
    }
    if changed_path == "dev/consolidation/current_source_policy.xsh" {
      native_files += ["dev/tests/test-consolidation-ratchets-current.xsh"] when "dev/tests/test-consolidation-ratchets-current.xsh" not in native_files
      continue
    }
    let caller_tests = cargo_caller_tests(changed_path)
    if ! caller_tests.is_empty() {
      for file in caller_tests {
        native_files += [file] when file not in native_files
      }
      continue
    }
    if changed_path.split("/").len() == 3 and changed_path.starts_with("core/tests/test-") and changed_path.ends_with(".xsh") {
      native_files += [changed_path] when changed_path not in native_files
      continue
    }
    if ! changed_path.starts_with("tests/fixtures/") and changed_path.ends_with(".xsh") and (changed_path.starts_with("tests/") or changed_path.starts_with("dev/tests/") or changed_path.starts_with("showcase/tests/")) {
      native_files += [changed_path] when changed_path not in native_files
      continue
    }
    for area in areas_for(changed_path)? {
      areas += [area] when area not in areas
    }
  }
  {areas: areas, native_files: native_files}
}

pure rust_gate(ctx: context.Context, profile: tests.TestProfile, name: Str, args: List[Str], debug = false) -> Result[contract.CommandSpec, Error] {
  let profile_args = if debug { [] } else { ["--profile", tests.profile_name(profile)] }
  stages.command(name, ctx.target.triple, "cargo",
    ["cargo", "test", "--locked", "--target", ctx.target.triple, @profile_args, @args],
    ctx.root, tests.cargo_environment(ctx)?)
}

pure native_gate(ctx: context.Context, profile: tests.TestProfile, filter: Str) -> contract.CommandSpec {
  let xsht = tests.product_path(ctx, profile, tests.Xsht).display()
  stages.command(f"native-{filter}", ctx.target.triple, xsht, [xsht, "test", filter], ctx.root, {})
}

pure native_exact_gate(ctx: context.Context, profile: tests.TestProfile, name: Str, exact_test: Str) -> contract.CommandSpec {
  let xsht = tests.product_path(ctx, profile, tests.Xsht).display()
  stages.command(f"native-{name}", ctx.target.triple, xsht, [xsht, "test", "--exact", exact_test], ctx.root, {})
}

pure requires_process_products(area: Area) -> Bool {
  match area {
    Compile => false
    Syntax => true
    Grammar => true
    Checker => true
    Lowering => true
    Runtime => true
    RouteWitness => true
    Sugar => true
    Tooling => true
    Api => true
    Stdlib => true
    Memory => false
    Docs => false
    Automation => true
    Core => true
    Showcase => true
    Soundness => false
  }
}

pure required_products(selection: Selection) -> List[tests.Product] {
  guard selection.native_files.is_empty() else {
    var cargo_caller_only = selection.areas.is_empty()
    for file in selection.native_files {
      cargo_caller_only = false when file not in cargo_caller_native_files
    }
    return [tests.Xsh, tests.Xsht] when cargo_caller_only
    return tests.all_products()
  }
  for area in selection.areas {
    return tests.all_products() when requires_process_products(area)
  }
  return [tests.Xsh, tests.Xsht] when Docs in selection.areas
  return [tests.Xsht] when Memory in selection.areas
  []
}

## Maps each selected area to its focused compiler or Rust gate and native
## behavioral module. No area runs the whole native corpus.
export pure lane_plan(ctx: context.Context, profile: tests.TestProfile, selection: Selection) -> Result[List[contract.CommandSpec], Error] {
  let root_target = ["-p", "xsh", "--test", "integration"]
  let xsht_target = ["-p", "xsht", "--test", "integration"]
  let root_lib = ["-p", "xsh", "--lib"]
  var plan = tests.product_builds(ctx, profile, required_products(selection))?
  for area in selection.areas {
    match area {
      Compile => plan += [stages.command("check-compile", ctx.target.triple, "cargo",
        ["cargo", "check", "--locked", "--target", ctx.target.triple, "-p", "xsh", "-p", "xsht", "-p", "xsh-fuzz", "--all-targets"],
        ctx.root, tests.cargo_environment(ctx)?)],
      Syntax => plan += [rust_gate(ctx, profile, "test-syntax", [@xsht_target, "syntax::", "--", @tests.sibling_syntax_exclusions])?, rust_gate(ctx, profile, "test-grammar-rust", [@root_target, "grammar::"])?, native_gate(ctx, profile, "tests/xsh/formatter.xsh")],
      Grammar => plan += [rust_gate(ctx, profile, "test-grammar", ["-p", "xsh-fuzz", "--test", "soundness", "generated_programs_are_grammar_sentences"])?, native_gate(ctx, profile, "tests/xsh/tooling-commands.xsh")],
      Checker => plan += [rust_gate(ctx, profile, "test-sema", [@root_target, "sema::"])?, native_gate(ctx, profile, "tests/xsh/checker-")],
      Lowering => plan += [rust_gate(ctx, profile, "test-lowering", [@root_lib, "runtime::eval"], debug: true)?, rust_gate(ctx, profile, "test-type-agreement", [@root_lib, "corpus_annotations_agree_with_binding_facts"], debug: true)?, native_gate(ctx, profile, "tests/xsh/lowering-coverage.xsh")],
      Runtime => plan += [rust_gate(ctx, profile, "test-runtime", [@root_target, "runtime::", "--", "--skip", "runtime::coverage::"])?, native_gate(ctx, profile, "tests/xsh/lowering-coverage.xsh")],
      RouteWitness => {
        plan += [rust_gate(ctx, profile, "test-route-witness-unit", [@root_lib, "runtime::eval::route_witness::tests"], debug: true)?]
        plan += [rust_gate(ctx, profile, "test-route-witness-inherited-env", [@root_lib, "runtime_env_route_witness_tests"], debug: true)?]
        plan += [rust_gate(ctx, profile, "test-route-witness-cli", [@xsht_target, "route_witness"])?] when Tooling not in selection.areas
        plan += [native_exact_gate(ctx, profile, "route-witness-env", "tests/xsh/stdlib/env.xsh::test_route_witness_prefix_names_are_ordinary_environment_entries")] when "tests/xsh/stdlib/env.xsh" not in selection.native_files
      },
      Sugar => plan += [rust_gate(ctx, profile, "test-sugar", ["-p", "xsht", "--lib", "sugar_expansion_tests"], debug: true)?, rust_gate(ctx, profile, "test-desugar-unit", ["-p", "xsht", "--lib", "desugar_tests"], debug: true)?, rust_gate(ctx, profile, "test-desugar", [@xsht_target, "desugar::"])?, native_gate(ctx, profile, "tests/xsh/desugar.xsh")],
      Tooling => plan += [rust_gate(ctx, profile, "test-tooling", [@xsht_target, "--", @tests.sibling_syntax_exclusions, @tests.xsht_integration_exclusions])?, native_gate(ctx, profile, "tests/xsh/tooling-")],
      Api => plan += [rust_gate(ctx, profile, "test-libxsh-api", [@root_target, "libxsh_api"])?, rust_gate(ctx, profile, "test-registry", ["-p", "xsh-registry", "--lib"], debug: true)?, rust_gate(ctx, profile, "test-signatures", [@root_lib, "modules::signature"], debug: true)?, rust_gate(ctx, profile, "test-api", [@xsht_target, "api::"])?, native_gate(ctx, profile, "tests/xsh/api-tool.xsh"), stages.command("check-api-snippets", ctx.target.triple, tests.product_path(ctx, profile, tests.Xsht).display(), [tests.product_path(ctx, profile, tests.Xsht).display(), "check", "docs/snippets/api"], ctx.root, {})],
      Stdlib => plan += [rust_gate(ctx, profile, "test-stdlib-catalog", [@root_lib, "stdlib::tests::every_catalog_module_parses_checks_and_lowers"], debug: true)?, native_gate(ctx, profile, "tests/xsh/stdlib")],
      Memory => plan += [rust_gate(ctx, profile, "test-frontend-memory", [@root_lib, "frontend_stats::tests"], debug: true)?, stages.command("frontend-memory-report", ctx.target.triple, tests.product_path(ctx, profile, tests.Xsht).display(), [tests.product_path(ctx, profile, tests.Xsht).display(), "frontend-stats", "--json", "tests/fixtures/frontend-indexed"], ctx.root, {})],
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

proc docs_gate(ctx: context.Context, profile: tests.TestProfile) [fs, process, env, error, io] {
  documentation.check(ctx.root, documentation.DocTools(
    xsh: tests.product_path(ctx, profile, tests.Xsh), xsht: tests.product_path(ctx, profile, tests.Xsht)))
}

## Automatic repository checks include the retained command contracts and
## source-bound consolidation ratchets.
export pure batch_checks(ctx: context.Context, profile: tests.TestProfile) -> List[contract.CommandSpec] {
  let xsht = tests.product_path(ctx, profile, tests.Xsht).display()
  let xsh = tests.product_path(ctx, profile, tests.Xsh).display()
  [
    stages.command("batch-check", ctx.target.triple, xsht, [xsht, "check"], ctx.root, {}),
    stages.command("batch-lint", ctx.target.triple, xsht, [xsht, "lint"], ctx.root, {}),
    stages.command("batch-diff", ctx.target.triple, "git", ["git", "diff", "--check"], ctx.root, {}),
    stages.command("batch-ratchets", ctx.target.triple, xsh,
      [xsh, "dev/consolidation/metrics.xsh", "check-current", "--root", ctx.root.display(),
        "--source-root", "src", "--xsht-source-root", "crates/xsht/src",
        "--baseline", "dev/consolidation/baseline-blanket-scope.json", "--baseline-provenance", "dev/consolidation/blanket-metric-provenance-source-advisory.json",
        "--envelope", "dev/consolidation/envelope-upstream.json",
        "--output", ".work/consolidation/current.json", "--audit", ".work/consolidation/rises.json"], ctx.root, {}),
  ]
}

## Runs the complete sequence at an integration boundary without source-tree
## formatters or autofixers.
export proc batch(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit, Error] {
  let profile = tests.execution_profile()?
  tests.run_commands(tests.rust_plan(ctx, profile, privileged: ctx.target.os == "linux")?)
  tests.native(ctx, profile, ["--jobs", "4"])
  tests.run_commands(batch_checks(ctx, profile))
  docs_gate(ctx, profile)
}

pure tier_name(tier: Tier) -> Str {
  match tier { Edit => "edit", Change => "change", Scenario => "scenario" }
}

pure declared_case(spec: contract.CommandSpec, products: List[tests.Product], reasons: List[Str]) -> CatalogCase {
  {id: spec.stage, tier: Change, timeout_seconds: 300, products: products,
    prerequisite_ids: [], reasons: reasons, execution: RunCommand(spec)}
}

## Derives cases from the established owner plans without a second test registry.
export pure catalogue(ctx: context.Context, profile: tests.TestProfile, selection: Selection, reasons: List[Str]) -> Result[List[CatalogCase], Error] {
  var entries: List[CatalogCase] = []
  let selected_stages = [spec.stage for spec in lane_plan(ctx, profile, selection)?]
  for area in selection.areas {
    let owner = Selection(areas: [area], native_files: [])
    for spec in lane_plan(ctx, profile, owner)? {
      continue when spec.stage == "test-products" or spec.stage not in selected_stages
      if spec.stage not in [entry.id for entry in entries] {
        entries += [declared_case(spec, required_products(owner), reasons)]
      }
    }
  }
  if Docs in selection.areas {
    entries += [CatalogCase(id: "check-docs", tier: Change, timeout_seconds: 300,
      products: [tests.Xsh, tests.Xsht], prerequisite_ids: [], reasons: reasons,
      execution: DocumentationCheck)]
  }
  for file in selection.native_files {
    let owner = Selection(areas: [], native_files: [file])
    let spec = native_gate(ctx, profile, file)
    entries += [declared_case(spec, required_products(owner), reasons)] when spec.stage not in [entry.id for entry in entries]
  }
  entries += [CatalogCase(id: "showcase/tokei/correctness", tier: Edit,
    timeout_seconds: 30, products: [tests.Xsh, tests.Xsht], prerequisite_ids: [],
    reasons: ["showcase/tokei.xsh output and classification contract"],
    execution: RunCommand(native_gate(ctx, profile, "showcase/tests/test-tokei.xsh")))]
  entries += [CatalogCase(id: "core/lsmem/correctness", tier: Edit,
    timeout_seconds: 30, products: [tests.Xsh, tests.Xsht], prerequisite_ids: [],
    reasons: ["core/lsmem.xsh rooted memory inventory contract"],
    execution: RunCommand(native_gate(ctx, profile, "core/tests/test-lsmem.xsh")))]
  entries += [CatalogCase(id: "full", tier: Change, timeout_seconds: 1,
    products: [], prerequisite_ids: [], reasons: ["explicit comprehensive verification"], execution: ManualFull)]
  entries
}

pure all_areas() -> List[Area] {
  [Compile, Syntax, Grammar, Checker, Lowering, Runtime, RouteWitness, Sugar, Tooling, Api, Stdlib, Memory, Docs, Automation, Core, Showcase, Soundness]
}

## Plans known ownership, discovers owned native-file IDs, and records unknown scope.
export proc plan_paths(ctx: context.Context, profile: tests.TestProfile, paths: List[Str], explicit_ids: List[Str], tier: Tier) [fs, error] -> Result[CatalogPlan, Error] {
  var chosen: List[CatalogCase] = []
  var unresolved: List[Str] = []
  var selected_ids = explicit_ids
  var known_paths: List[Str] = []
  for changed_path in paths {
    if changed_path.starts_with("/") or ".." in changed_path.split("/") {
      unresolved += [f"path outside repository scope: {changed_path}"]
      continue
    }
    if changed_path in ["showcase/tokei.xsh", "core/lsmem.xsh"] {
      let id = if changed_path == "showcase/tokei.xsh" { "showcase/tokei/correctness" } else { "core/lsmem/correctness" }
      selected_ids += [id] when id not in selected_ids
      continue
    }
    match select([changed_path]) {
      Ok(_) => known_paths += [changed_path]
      Err(_) => unresolved += [f"unknown verification owner: {changed_path}"]
    }
  }
  let selection = select(known_paths)?
  for entry in catalogue(ctx, profile, selection, [f"changed {changed_path}" for changed_path in known_paths])? {
    continue when entry.id in ["full", "showcase/tokei/correctness", "core/lsmem/correctness"]
    chosen += [entry]
    selected_ids += [entry.id] when entry.id not in selected_ids
  }
  for area in all_areas() {
    for entry in catalogue(ctx, profile, Selection(areas: [area], native_files: []), ["explicit case selection"])? {
      chosen += [entry] when entry.id not in [known.id for known in chosen]
    }
  }
  var seen_ids: List[Str] = []
  for id in explicit_ids {
    return Err(PlanError.Invalid(detail: f"duplicate case ID {id}")) when id in seen_ids
    seen_ids += [id]
    continue when ! id.starts_with("native-") or ! id.ends_with(".xsh")
    let file = id[7..]
    return Err(PlanError.Invalid(detail: f"invalid native case path {file}")) when file.starts_with("/") or ".." in file.split("/")
    let owner = match select([file]) {
      Ok(selection) => selection
      Err(_) => { return Err(PlanError.Invalid(detail: f"native case {id} has no file owner")) }
    }
    return Err(PlanError.Invalid(detail: f"native case {id} has no file owner")) when file not in owner.native_files
    let native_path = fp"{ctx.root}/{file}"
    return Err(PlanError.Invalid(detail: f"native case {id} is missing")) when ! native_path.exists()?
    let resolved_path = native_path.resolve()?
    let owned_root = ctx.root.resolve()?.display() + "/"
    return Err(PlanError.Invalid(detail: f"native case {id} escapes the owned source root")) when ! resolved_path.display().starts_with(owned_root)
    return Err(PlanError.Invalid(detail: f"native case {id} is not a file")) when resolved_path.metadata()?.kind != "file"
    if id not in [entry.id for entry in chosen] {
      for entry in catalogue(ctx, profile, Selection(areas: [], native_files: [file]), ["explicit native file selection"])? {
        chosen += [entry] when entry.id == id
      }
    }
  }
  let resolved = resolve_selection(chosen, selected_ids, explicit_ids, tier)?
  {...resolved, unresolved: unresolved}
}

pure product_name(product: tests.Product) -> Str {
  match product { tests.Xsh => "xsh", tests.Xsht => "xsht", tests.Xshi => "xshi" }
}

pure case_argv(execution: CaseExecution) -> List[Str] {
  match execution { RunCommand(spec) => spec.argv, DocumentationCheck => [], Alias => [], ManualFull => [] }
}

proc inspect_plan(plan: CatalogPlan, machine: Bool) [error, io] {
  let cases = [
    {id: entry.id, tier: tier_name(entry.tier), timeout_seconds: entry.timeout_seconds,
      reasons: entry.reasons, prerequisite_ids: entry.prerequisite_ids,
      products: [product_name(product) for product in entry.products], argv: case_argv(entry.execution)}
    for entry in plan.cases
  ]
  if machine {
    print json.encode({schema_version: 1, kind: "verification_plan", tier: tier_name(plan.tier),
      selection_complete: plan.unresolved.is_empty(), cases: cases, unresolved: plan.unresolved,
      products: [product_name(product) for product in plan.products],
      full: "not_run", execution_authority: false}, pretty: true)?
    return
  }
  print f"selected {tier_name(plan.tier)} scope: {plan.cases.len()} cases; Full not run"
  for entry in plan.cases { print f"{entry.id}: {entry.reasons.join("; ")}" }
  for gap in plan.unresolved { print f"incomplete: {gap}" }
}

proc changed_paths(ctx: context.Context) [fs, process, env, error] -> Result[List[Str], Error] {
  cd (ctx.root) {
    let tracked = run.text git -c f"safe.directory={ctx.root}" diff --name-only HEAD
    let untracked = run.text git -c f"safe.directory={ctx.root}" ls-files --others --exclude-standard
    return [@tracked.lines(), @untracked.lines()] |> where . != ""
  }
}

proc focused(ctx: context.Context, tier: Tier, supplied: List[Str], ids: List[Str], output: OutputMode) [fs, process, env, time, error, io] -> Result[Unit, Error] {
  let paths = if supplied.is_empty() and ids.is_empty() { changed_paths(ctx)? } else { supplied }
  let profile = tests.execution_profile()?
  let plan = plan_paths(ctx, profile, paths, ids, tier)?
  inspect_plan(plan, output == InspectJson)
  if ! plan.unresolved.is_empty() { exit 2 }
  return when output != Execute
  if ctx.target.os == "linux" {
    var forwarded = ["--tier", tier_name(tier), @paths]
    for id in ids { forwarded += ["--case", id] }
    return container_status(docker.run_internal_status({...ctx, profile: tests.profile_name(profile)}, "gate-lane", false, forwarded)?)
  }
  execute_plan(ctx, profile, plan, paths)
}

pure automatic_budget_ns(tier: Tier) -> Int {
  match tier { Edit => 30000000000, Change => 300000000000, Scenario => 0 }
}

proc leaf_adapter(ctx: context.Context, profile: tests.TestProfile, action: Str, id: Str, paths: List[Str]) [process, error] -> Result[contract.CommandSpec, Error] {
  let xsh = applet.current_exe()?.display()
  stages.command(f"case-{action}-{id}", ctx.target.triple, xsh,
    [xsh, "dev/gate_case.xsh", "--", action, id, @paths], ctx.root,
    {TARGET: ctx.target.triple, DIST_PROFILE: tests.profile_name(profile), CARGO_TARGET_DIR: ctx.target_dir.display()})
}

## Revalidates the full selected closure, then prepares or executes only one leaf.
export proc run_case(ctx: context.Context, action: CaseAction, id: Str, paths: List[Str]) [fs, process, env, error, io] -> Result[Unit, Error] {
  let profile = tests.execution_profile()?
  let plan = plan_paths(ctx, profile, paths, [id], Change)?
  return Err(PlanError.Invalid(detail: "unresolved leaf scope")) when ! plan.unresolved.is_empty()
  let entries = [entry for entry in plan.cases |> where .id == id]
  return Err(PlanError.Invalid(detail: f"case {id} is not uniquely selected")) when entries.len() != 1
  let entry = entries[0]
  match entry.execution {
    RunCommand(spec) => {
      match action {
        Prepare => {
          guard spec.executable == "cargo" and spec.argv[1] == "test" else {
            return Err(PlanError.Invalid(detail: "only Cargo test harnesses require leaf preparation"))
          }
          let argv = harness_build_argv(spec.argv)
          return cargo_steps.run_build({...spec, stage: f"{spec.stage}-compile", argv: [@argv, "--message-format=json-render-diagnostics"]})
        }
        ExecuteLeaf => {
          return Err(PlanError.Invalid(detail: "prepared Cargo harnesses execute directly, never through Cargo")) when spec.executable == "cargo"
          return tests.run_commands([spec])
        }
      }
    }
    DocumentationCheck => {
      return Err(PlanError.Invalid(detail: "documentation is not a Cargo harness")) when action == Prepare
      return docs_gate(ctx, profile)
    }
    Alias => return
    ManualFull => { return Err(PlanError.Invalid(detail: "manual leaf cannot execute in a focused request")) }
  }
}

pure harness_build_argv(argv: List[Str]) -> List[Str] {
  var build = ["cargo", "test"]
  var index = 2
  while index < argv.len() {
    let word = argv[index]
    break when word == "--"
    if word in ["--target", "--profile", "-p", "--package", "--test", "--bin", "--features"] {
      build += [word, argv[index + 1]]
      index += 2
      continue
    }
    build += [word] when word.starts_with("-")
    index += 1
  }
  cargo_steps.test_build_argv(build)
}

pure harness_arguments(argv: List[Str]) -> List[Str] {
  var arguments: List[Str] = []
  var index = 2
  while index < argv.len() {
    let word = argv[index]
    if word == "--" { return [@arguments, @(argv |> drop(index + 1))] }
    if word in ["--target", "--profile", "-p", "--package", "--test", "--bin", "--features"] {
      index += 2
      continue
    }
    arguments += [word] when ! word.starts_with("-")
    index += 1
  }
  arguments
}

proc prepared_harness(spec: contract.CommandSpec, output: Path, target_dir: Path) [fs, error] -> Result[contract.CommandSpec, Error] {
  var executable: Str? = null
  var finished = false
  for line in output.read_lines()? {
    continue when line.starts_with("[")
    let record = json.decode(line)?.require(Record)?
    let reason = record.get("reason")?.require(Str)?
    if reason == "build-finished" {
      return Err(PlanError.Invalid(detail: "duplicate Cargo build completion")) when finished
      finished = record.get("success")?.require(Bool)?
      return Err(PlanError.Invalid(detail: "Cargo preparation did not succeed")) when ! finished
    }
    if reason == "compiler-artifact" {
      let profile = record.get("profile")?.require(Record)?
      if profile.get("test")?.require(Bool)? {
        let candidate = record.get("executable")?.require(Str?)?
        if candidate != null {
          return Err(PlanError.Invalid(detail: "multiple Cargo test harnesses selected")) when executable != null
          executable = candidate
        }
      }
    }
  }
  return Err(PlanError.Invalid(detail: "Cargo preparation omitted its test executable or completion")) when executable == null or ! finished
  let executable_path = fp"{executable ?? ""}".resolve()?
  let owned = target_dir.resolve()?.display() + "/"
  return Err(PlanError.Invalid(detail: "Cargo test executable escapes owned products")) when ! executable_path.display().starts_with(owned)
  {...spec, executable: executable_path.display(), argv: [executable_path.display(), @harness_arguments(spec.argv)]}
}

proc execute_plan(ctx: context.Context, profile: tests.TestProfile, plan: CatalogPlan, paths: List[Str]) [fs, process, env, time, error, io] -> Result[Unit, Error] {
  print "compilation: preparing selected executable products outside the runtime budget"
  tests.run_commands(tests.product_builds(ctx, profile, plan.products)?)
  var remaining_ns = automatic_budget_ns(plan.tier)
  var harnesses: Map[Str] = {}
  for entry in plan.cases {
    var commands: List[contract.CommandSpec] = []
    match entry.execution {
      RunCommand(spec) => {
        if spec.executable == "cargo" and spec.argv[1] == "test" {
          let key = harness_build_argv(spec.argv).join("\0")
          if let Ok(executable) = harnesses.get(key) {
            commands = [{...spec, executable: executable, argv: [executable, @harness_arguments(spec.argv)]}]
          } else {
            stages.ensure_dir(fp"{ctx.root}/.work/tmp")
            let scratch = fs.tempdir_in(fp"{ctx.root}/.work/tmp")?
            let output = fp"{scratch.host_path()?}/cargo.jsonl"
            print f"compilation: preparing {entry.id} outside the runtime budget"
            let prepare = leaf_adapter(ctx, profile, "prepare", entry.id, paths)?
            let status = process.run(process.command_argv(prepare.executable, prepare.argv,
              cwd: prepare.cwd, env: prepare.environment, stdout: output))?
            let raw = status.shell_code()?
            return Err(ExecutionError.Cancelled(case_id: entry.id, raw_status: raw)) when raw == 130
            return Err(ExecutionError.Failed(case_id: entry.id, raw_status: raw)) when ! status.ok
            let prepared = prepared_harness(spec, output, ctx.target_dir)?
            harnesses[key] = prepared.executable
            commands = [prepared]
          }
        } else if spec.executable == "cargo" and spec.argv[1] in ["build", "check"] {
          print f"compilation: {entry.id} outside the runtime budget"
          tests.run_commands([spec])
        } else {
          commands = [spec]
        }
      }
      DocumentationCheck => commands = [leaf_adapter(ctx, profile, "execute", entry.id, paths)?]
      Alias => {}
      ManualFull => { return Err(PlanError.Invalid(detail: "manual request in focused execution")) }
    }
    for command in commands {
      let available_ns = if entry.tier == Scenario { entry.timeout_seconds * 1000000000 } else { remaining_ns }
      return Err(ExecutionError.BudgetExpired(case_id: entry.id)) when available_ns < 1000000
      let limit = gate_execution.limit_ms(entry.timeout_seconds, available_ns)?
      let started = time.monotonic()?
      let outcome = gate_execution.execute(command, limit)?
      let elapsed = time.monotonic()? - started
      remaining_ns -= elapsed when entry.tier != Scenario
      match outcome {
        gate_execution.Finished(status) => {
          let raw = status.shell_code()?
          return Err(ExecutionError.Cancelled(case_id: entry.id, raw_status: raw)) when raw == 130
          return Err(ExecutionError.Failed(case_id: entry.id, raw_status: raw)) when ! status.ok
        }
        gate_execution.TimedOut(cause) => { return Err(ExecutionError.TimedOut(case_id: entry.id, cause: cause)) }
        gate_execution.Cancelled(cause) => { return Err(ExecutionError.Interrupted(case_id: entry.id, cause: cause)) }
        gate_execution.CouldNotExecute(cause) => { return Err(ExecutionError.ProcessFailed(case_id: entry.id, cause: cause)) }
      }
    }
  }
}

## Parses authority before entering Docker or launching any verification command.
export proc execute(ctx: context.Context, requested: List[Str]) [fs, process, env, time, error, io] -> Result[Unit, Error] {
  present(execute_request(ctx, requested))
}

proc present(result: Result[Unit, Error]) [error, io] -> Result[Unit, Error] {
  match result {
    Ok(_) => return
    Err(PlanError.Invalid {detail: detail}) => { print f"invalid gate plan: {detail}"; exit 2 }
    Err(ExecutionError.Failed {case_id: id, raw_status: raw}) => { print f"failed: {id} raw_status={raw}"; exit 1 }
    Err(ExecutionError.TimedOut {case_id: id, cause: cause}) => { print f"incomplete: {id} timed out: {cause.message}"; exit 2 }
    Err(ExecutionError.BudgetExpired {case_id: id}) => { print f"incomplete: runtime budget exhausted before {id}"; exit 2 }
    Err(ExecutionError.Cancelled {case_id: id, raw_status: raw}) => { print f"cancelled: {id} raw_status={raw}"; exit 130 }
    Err(ExecutionError.Interrupted {case_id: id, cause: cause}) => { print f"cancelled: {id} cause={cause.message}"; exit 130 }
    Err(ExecutionError.Incomplete {case_id: id, raw_status: raw}) => { print f"incomplete: {id} raw_status={raw}"; exit 2 }
    Err(ExecutionError.ProcessFailed {case_id: id, cause: cause}) => { print f"failed: {id} execution_error={cause.message}"; exit 1 }
    Err(stages.StageError.Failed {stage: stage, target: target, detail: detail}) => { print f"failed: {stage} target={target} execution_error={detail}"; exit 1 }
    Err(stages.StageError.MissingTool {tool: tool}) => { print f"failed: missing tool {tool}"; exit 1 }
    Err(cause) => { print f"failed: {cause.message}"; exit 1 }
  }
}

proc container_status(status: Status) [error] -> Result[Unit, Error] {
  let raw = status.shell_code()?
  return Err(ExecutionError.Cancelled(case_id: "container", raw_status: raw)) when raw == 130
  return Err(ExecutionError.Incomplete(case_id: "container", raw_status: raw)) when raw == 2
  return Err(ExecutionError.Failed(case_id: "container", raw_status: raw)) when ! status.ok
}

proc execute_request(ctx: context.Context, requested: List[Str]) [fs, process, env, time, error, io] -> Result[Unit, Error] {
  match parse_request(requested)? {
    Full => {
      if ctx.target.os == "linux" {
        let profile = tests.execution_profile()?
        return container_status(docker.run_internal_status({...ctx, profile: tests.profile_name(profile)}, "gate-full", true, [])?)
      }
      return batch(ctx)
    }
    Focused(tier, paths, ids, output) => return focused(ctx, tier, paths, ids, output)
  }
}

## Internal focused execution validates the same request and closure before effects.
export proc lane(ctx: context.Context, requested: List[Str]) [fs, process, env, time, error, io] -> Result[Unit, Error] {
  present(lane_request(ctx, requested))
}

proc lane_request(ctx: context.Context, requested: List[Str]) [fs, process, env, time, error, io] -> Result[Unit, Error] {
  match parse_request(requested)? {
    Full => { return Err(PlanError.Invalid(detail: "focused dispatch cannot request Full")) }
    Focused(tier, supplied, ids, output) => {
      let paths = if supplied.is_empty() and ids.is_empty() { changed_paths(ctx)? } else { supplied }
      let profile = tests.execution_profile()?
      let plan = plan_paths(ctx, profile, paths, ids, tier)?
      inspect_plan(plan, output == InspectJson)
      if ! plan.unresolved.is_empty() { exit 2 }
      return when output != Execute
      return execute_plan(ctx, profile, plan, paths)
    }
  }
}
