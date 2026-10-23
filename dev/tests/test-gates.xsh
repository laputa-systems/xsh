use docker
use fixtures
use gates
use gate_execution
use internal as container_internal
use test_workflows as tests
use unix

# Combining owners preserves their required gates without scheduling a second
# copy for every changed file in the same source family.
test test_changed_paths_select_and_deduplicate_their_verification_owners {
  let selected = gates.select(["src/sema/check/records.rs", "src/sema/check/results.rs", "src/runtime/eval/indexed/full.rs", "tests/xsh/retry.xsh", "tests/xsh/retry.xsh"])?
  assert selected.areas == [gates.Checker, gates.Lowering]
  assert selected.native_files == ["tests/xsh/retry.xsh"]
  assert gates.areas_for("src/syntax/grammar.rs")? == [gates.Syntax, gates.Grammar, gates.Docs]
  assert gates.areas_for("crates/xsht/tests/syntax.rs")? == [gates.Syntax]
  assert gates.areas_for("tests/grammar.rs")? == [gates.Syntax]
  assert gates.areas_for("docs/snippets/api/fs.xsh")? == [gates.Docs, gates.Api]
  let source_policy = gates.select(["dev/consolidation/current_source_policy.xsh"])?
  assert source_policy.areas.is_empty()
  assert source_policy.native_files == ["dev/tests/test-consolidation-ratchets-current.xsh"]
  match gates.select(["unowned-family/behavior.rs"]) {
    Ok(_) => test.fail("a changed path without a gate owner was accepted")
    Err(error) => assert error.message == "GateError.UnmappedPath"
  }
}

test test_cargo_callers_select_their_exact_native_tests_and_products {
  let ctx = fixtures.linux_context(/repo)?
  let cases = [
    {
      path: "dev/cargo_steps.xsh",
      files: ["dev/tests/test-cargo-steps.xsh", "dev/tests/test-lifecycle.xsh"],
    },
    {
      path: "dev/cargo_test_runner.sh",
      files: ["dev/tests/test-cargo-steps.xsh"],
    },
    {
      path: "dev/test_workflows.xsh",
      files: [
        "dev/tests/test-cargo-steps.xsh",
        "dev/tests/test-lifecycle.xsh",
        "dev/tests/test-gates.xsh",
        "dev/tests/test-native-workflows.xsh",
      ],
    },
  ]

  for case in cases {
    let selection = gates.select([case.path])?
    assert selection.areas.is_empty()
    assert selection.native_files == case.files

    let plan = gates.lane_plan(ctx, tests.Verification, selection)?
    var expected_stages = ["test-products"]
    for file in case.files {
      expected_stages += [f"native-{file}"]
    }
    assert [spec.stage for spec in plan] == expected_stages
    let product_build = plan[0]
    assert product_build.argv[1..] == [
      "build", "--locked", "--profile", "verification", "--target", ctx.target.triple,
      "-p", "xsh", "--bins", "-p", "xsht", "--bin", "xsht",
    ]
    let native = [spec for spec in plan |> where .stage.starts_with("native-")]
    assert [spec.argv[2] for spec in native] == case.files
  }

  let combined = gates.select([
    "dev/test_workflows.xsh",
    "dev/cargo_steps.xsh",
    "dev/cargo_test_runner.sh",
  ])?
  assert combined.areas.is_empty()
  assert combined.native_files == [
    "dev/tests/test-cargo-steps.xsh",
    "dev/tests/test-lifecycle.xsh",
    "dev/tests/test-gates.xsh",
    "dev/tests/test-native-workflows.xsh",
  ]

  let broad = gates.select(["dev/release.xsh"])?
  assert broad.areas == [gates.Automation]
  assert broad.native_files.is_empty()
  let broad_plan = gates.lane_plan(ctx, tests.Verification, broad)?
  assert [spec.stage for spec in broad_plan] == ["test-products", "native-dev/tests"]
  assert "xshi" in broad_plan[0].argv
}

test test_current_metric_attestation_changes_select_both_native_contracts {
  let files = [
    "dev/tests/test-consolidation-metrics-current.xsh",
    "dev/tests/test-consolidation-ratchets-current.xsh",
  ]
  let paths = [
    "dev/consolidation_metrics_current.xsh",
    "dev/consolidation/metric-observer-successor.json",
    @files,
  ]
  let ctx = fixtures.linux_context(/repo)?
  for changed_path in paths {
    let selected = gates.select([changed_path])?
    assert selected.areas.is_empty()
    assert selected.native_files == files
    let plan = gates.lane_plan(ctx, tests.Verification, selected)?
    assert [spec.stage for spec in plan] == [
      "test-products",
      "native-dev/tests/test-consolidation-metrics-current.xsh",
      "native-dev/tests/test-consolidation-ratchets-current.xsh",
    ]
  }
  let combined = gates.select([@paths, @paths])?
  assert combined.areas.is_empty()
  assert combined.native_files == files
}

test test_route_witness_leaf_selects_its_direct_unit_cli_and_native_cases {
  let ctx = fixtures.linux_context(/repo)?
  let selection = gates.select(["src/runtime/eval/route_witness.rs"])?
  assert selection.areas == [gates.RouteWitness]
  assert selection.native_files.is_empty()

  let plan = gates.lane_plan(ctx, tests.Verification, selection)?
  let stages = [spec.stage for spec in plan]
  assert stages == [
    "test-products",
    "test-route-witness-unit",
    "test-route-witness-inherited-env",
    "test-route-witness-cli",
    "native-route-witness-env",
  ]
  assert "test-runtime" not in stages

  let product_build = [spec for spec in plan |> where .stage == "test-products"][0]
  assert product_build.argv[1..] == [
    "build", "--locked", "--profile", "verification", "--target", ctx.target.triple,
    "-p", "xsh", "--bins", "-p", "xsht", "--bin", "xsht", "-p", "xshi", "--bin", "xshi",
  ]

  let unit = [spec for spec in plan |> where .stage == "test-route-witness-unit"][0]
  assert "--lib" in unit.argv
  assert "runtime::eval::route_witness::tests" in unit.argv
  assert "--profile" not in unit.argv

  let inherited = [spec for spec in plan |> where .stage == "test-route-witness-inherited-env"][0]
  assert "--lib" in inherited.argv
  assert "runtime_env_route_witness_tests" in inherited.argv
  assert "--profile" not in inherited.argv

  let cli_gate = [spec for spec in plan |> where .stage == "test-route-witness-cli"][0]
  assert cli_gate.argv[1..] == [
    "test", "--locked", "--target", ctx.target.triple, "--profile", "verification",
    "-p", "xsht", "--test", "integration", "route_witness",
  ]

  let native = [spec for spec in plan |> where .stage.starts_with("native-route-witness-")]
  assert native.len() == 1
  assert native[0].argv[1..] == [
    "test", "--exact",
    "tests/xsh/stdlib/env.xsh::test_route_witness_prefix_names_are_ordinary_environment_entries",
  ]
}

test test_route_witness_selector_preserves_broad_shared_runtime_gates {
  let ctx = fixtures.linux_context(/repo)?
  let selected = gates.select(["src/runtime/eval.rs"])?
  assert selected.areas == [gates.Runtime]
  let plan = gates.lane_plan(ctx, tests.Verification, selected)?
  let runtime = [spec for spec in plan |> where .stage == "test-runtime"]
  assert runtime.len() == 1
  assert "runtime::" in runtime[0].argv
  assert "--skip" in runtime[0].argv
}

test test_compile_and_soundness_leaf_gates_skip_product_builds {
  let ctx = fixtures.linux_context(/repo)?

  let compile = gates.lane_plan(ctx, tests.Verification, gates.select(["src/frontend/source.rs"])?)?
  assert [spec.stage for spec in compile] == ["check-compile"]

  let soundness = gates.lane_plan(ctx, tests.Verification, gates.select(["crates/xsh-fuzz/src/driver.rs"])?)?
  assert [spec.stage for spec in soundness] == ["test-soundness"]

  let no_products = tests.product_builds(ctx, tests.Verification, [])?
  assert no_products.is_empty()
}

test test_memory_and_documentation_gates_build_only_their_cli_products {
  let ctx = fixtures.linux_context(/repo)?

  let memory = gates.lane_plan(ctx, tests.Verification, gates.select(["src/frontend_stats.rs"])?)?
  assert [spec.stage for spec in memory] == ["test-products", "test-frontend-memory", "frontend-memory-report"]
  assert memory[0].argv[1..] == [
    "build", "--locked", "--profile", "verification", "--target", ctx.target.triple,
    "-p", "xsht", "--bin", "xsht",
  ]

  let documentation = gates.lane_plan(ctx, tests.Verification, gates.select(["docs/TESTING.md"])?)?
  assert [spec.stage for spec in documentation] == ["test-products"]
  assert documentation[0].argv[1..] == [
    "build", "--locked", "--profile", "verification", "--target", ctx.target.triple,
    "-p", "xsh", "--bins", "-p", "xsht", "--bin", "xsht",
  ]
}

test test_route_witness_selector_avoids_duplicate_cli_and_native_runs {
  let ctx = fixtures.linux_context(/repo)?
  let selected = gates.select([
    "src/runtime/eval/route_witness.rs",
    "crates/xsht/tests/cli.rs",
    "tests/xsh/stdlib/env.xsh",
  ])?
  let plan = gates.lane_plan(ctx, tests.Verification, selected)?
  let stages = [spec.stage for spec in plan]
  assert "test-tooling" in stages
  assert "test-route-witness-cli" not in stages
  assert "native-route-witness-env" not in stages
  assert "native-tests/xsh/stdlib/env.xsh" in stages
}

test test_process_plan_uses_the_selected_target_profile_and_required_oracles {
  for ctx in [fixtures.linux_context(/repo)?, fixtures.linux_aarch64_context(/repo)?] {
    for profile in [tests.Verification, tests.Release] {
      let plan = tests.rust_plan(ctx, profile, privileged: true)?
      let stages = [spec.stage for spec in plan]
      assert "test-xsht" in stages
      assert "test-soundness" in stages
      assert "test-linux-priv" in stages
      assert "test-debug-assertions" in stages
      let privileged = [spec for spec in plan |> where .stage == "test-linux-priv"]
      assert [spec.argv[12] for spec in privileged] == ["core_compat_extra", "linux_priv"]
      for spec in privileged {
        assert spec.argv[11] == "--test"
        assert spec.argv[spec.argv.len() - 4..] == [
          "--skip", "split::", "--skip", "cp::privileged::test_cp_recursive_block_device_",
        ]
      }
      let ordinary = [spec for spec in plan |> where .stage == "test-rust"]
      assert [spec.argv[10] for spec in ordinary] == [
        "integration", "ambient_fs_policy", "symbol_plateau", "core_compat_boundaries", "core_compat_extra",
      ]
      for spec in ordinary {
        assert spec.argv[9] == "--test"
        assert spec.argv[spec.argv.len() - 4..] == ["--skip", "split::", "--skip", "runtime::coverage::"]
      }
      for spec in plan {
        assert "--target" in spec.argv
        assert ctx.target.triple in spec.argv
        assert ! spec.environment.keys().is_empty()
        if spec.stage == "test-debug-assertions" {
          assert "--lib" in spec.argv
          assert "--profile" not in spec.argv
          assert "--release" not in spec.argv
        } else {
          assert tests.profile_name(profile) in spec.argv
          assert "--lib" not in spec.argv
        }
      }
      let product_build = [spec for spec in plan |> where .stage == "test-products"][0]
      assert product_build.argv[1..] == [
        "build", "--locked", "--profile", tests.profile_name(profile), "--target", ctx.target.triple,
        "-p", "xsh", "--bins", "-p", "xsht", "--bin", "xsht", "-p", "xshi", "--bin", "xshi",
      ]
      let xsht_integration = [spec for spec in plan |> where .stage == "test-xsht"][0]
      assert xsht_integration.argv[xsht_integration.argv.len() - 8..] == [
        "--skip", "lint_format_invariance::formatting_preserves_lints_on_laputa_corpus",
        "--skip", "examples::example_corpus_is_formatted",
        "--skip", "examples::example_corpus_lints_without_warnings",
        "--skip", "lint_performance::repository_lint_is_clean_within_wall_budget",
      ]
      assert tests.xsht_integration_exclusions == xsht_integration.argv[xsht_integration.argv.len() - 8..]
      assert "lint_performance::lint_gate_terminates_and_reaps_process_at_wall_deadline" not in tests.xsht_integration_exclusions
      let product = tests.product_path(ctx, profile, tests.Xsht)
      assert product == fp"/repo/target/{ctx.target.triple}/{tests.profile_name(profile)}/xsht"
    }
  }
}

test test_lane_native_selection_never_requests_a_full_native_suite {
  let ctx = fixtures.linux_context(/repo)?
  let selection = gates.select(["src/sema/check/records.rs", "tests/xsh/retry.xsh"])?
  let plan = gates.lane_plan(ctx, tests.Verification, selection)?
  let native = [spec for spec in plan |> where .executable.ends_with("/xsht")]
  assert native.len() == 2
  for spec in native {
    assert spec.argv.len() == 3
    assert spec.argv[1] == "test"
    assert spec.argv[2] != ""
  }
  assert native[1].argv[2] == "tests/xsh/retry.xsh"

  let native_file = gates.lane_plan(ctx, tests.Verification, gates.select(["tests/xsh/retry.xsh"])?)?
  assert [spec.stage for spec in native_file] == ["test-products", "native-tests/xsh/retry.xsh"]
  assert native_file[0].argv[1..] == [
    "build", "--locked", "--profile", "verification", "--target", ctx.target.triple,
    "-p", "xsh", "--bins", "-p", "xsht", "--bin", "xsht", "-p", "xshi", "--bin", "xshi",
  ]
}

test test_core_test_files_select_one_file_and_shared_paths_keep_core_coverage {
  let ctx = fixtures.linux_context(/repo)?
  for test_file in ["core/tests/test-taskset.xsh", "core/tests/test-lsmem.xsh"] {
    let leaf = gates.select([test_file])?
    assert leaf.areas.is_empty()
    assert leaf.native_files == [test_file]

    let leaf_plan = gates.lane_plan(ctx, tests.Verification, leaf)?
    assert [spec.stage for spec in leaf_plan] == ["test-products", f"native-{test_file}"]
    let native = leaf_plan[1]
    assert native.argv[1..] == ["test", test_file]
  }

  for changed_path in ["core/lsmem.xsh", "core/tests/support/uu.xsh"] {
    let shared = gates.select([changed_path])?
    assert shared.areas == [gates.Core]
    assert shared.native_files.is_empty()

    let shared_plan = gates.lane_plan(ctx, tests.Verification, shared)?
    assert [spec.stage for spec in shared_plan] == ["test-products", "native-core/tests"]
    assert shared_plan[1].argv[1..] == ["test", "core/tests"]
  }
}

test test_explicit_unsupported_test_profiles_fail_instead_of_using_a_default {
  for requested in ["dist", "dev", "", "custom"] {
    env DIST_PROFILE=$requested {
      match tests.execution_profile() {
        Ok(_) => test.fail(f"unsupported profile {requested} was accepted")
        Err(error) => assert error.message == "TestProfileError.Unsupported"
      }
    }
  }
  env DIST_PROFILE=verification { assert tests.execution_profile()? == .Verification }
  env DIST_PROFILE=release { assert tests.execution_profile()? == .Release }
}


test test_fixtures_select_their_owner_instead_of_running_as_native_modules {
  let selected = gates.select(["tests/fixtures/syntax/valid/literal.xsh", "tests/fixtures/sema/invalid/type.xsh", "tests/fixtures/runtime/loop.xsh", "tests/fixtures/frontend-indexed/stages.xsh"])?
  assert selected.areas == [gates.Syntax, gates.Checker, gates.Runtime, gates.Memory]
  assert selected.native_files.is_empty()
  assert gates.areas_for("src/runtime/eval/lower/calls.rs")? == [gates.Lowering]
  assert gates.areas_for("src/syntax/grammar/productions.rs")? == [gates.Syntax, gates.Grammar, gates.Docs]
}

test test_catalog_and_xsht_exclusions_preserve_the_selected_verification_scope {
  let ctx = fixtures.linux_context(/repo)?
  let selected = gates.select(["src/stdlib.rs", "crates/xsht/src/lint.rs", "crates/xsht/tests/syntax.rs"])?
  let plan = gates.lane_plan(ctx, tests.Verification, selected)?
  let catalog = [spec for spec in plan |> where .stage == "test-stdlib-catalog"]
  assert catalog.len() == 1
  assert "--lib" in catalog[0].argv
  assert "--profile" not in catalog[0].argv
  assert "stdlib::tests::every_catalog_module_parses_checks_and_lowers" in catalog[0].argv
  for spec in plan {
    if spec.stage == "test-tooling" {
      assert "lint_format_invariance::formatting_preserves_lints_on_laputa_corpus" in spec.argv
      assert "syntax::formatter_is_idempotent_on_laputa_corpus" in spec.argv
      assert "lint_performance::repository_lint_is_clean_within_wall_budget" in spec.argv
      assert "--skip" in spec.argv
    }
    if spec.stage == "test-syntax" {
      assert "xsht" in spec.argv
      assert "integration" in spec.argv
      assert "syntax::formatter_is_idempotent_on_laputa_corpus" in spec.argv
      assert "--skip" in spec.argv
    }
    if spec.stage == "test-grammar-rust" {
      assert "xsh" in spec.argv
      assert "grammar::" in spec.argv
    }
  }
}

test test_batch_automatic_checks_use_retained_repository_gates {
  let ctx = fixtures.linux_context(/repo)?
  let checks = gates.batch_checks(ctx, tests.Verification)
  assert [spec.stage for spec in checks] == ["batch-check", "batch-lint", "batch-diff", "batch-ratchets"]
  let ratchets = checks[3]
  assert ratchets.argv[1] == "dev/consolidation/metrics.xsh"
  assert ratchets.argv[2] == "check-current"
  assert ["--root", ctx.root.display(), "--source-root", "src", "--xsht-source-root", "crates/xsht/src"] == ratchets.argv[3..9]
  assert ["--baseline", "dev/consolidation/baseline-blanket-scope.json", "--baseline-provenance", "dev/consolidation/blanket-metric-provenance-source-advisory.json", "--envelope", "dev/consolidation/envelope-upstream.json", "--output", ".work/consolidation/current.json", "--audit", ".work/consolidation/rises.json"] == ratchets.argv[9..]
  for spec in checks {
    assert spec.executable != "python3"
    assert [arg for arg in spec.argv |> where .starts_with("dev/compat/")].is_empty()
  }
  let retained = gates.select(["dev/checks/check_inventory.py"])?
  assert retained.areas == [gates.Automation]
  assert retained.native_files.is_empty()
}

test test_batch_preserves_rust_plan_and_bounds_native_case_admission { |ctx|
  let fixture_root = test.temp_dir(ctx, name: "batch-native-admission")?
  fixture_root.chmod(0o777)
  let work_root = fp"{fixture_root}/.work"
  work_root.mkdir()
  work_root.chmod(0o2777)
  let target_dir = fp"{fixture_root}/target"
  target_dir.mkdir()
  let product_dir = fp"{target_dir}/x86_64-unknown-linux-musl/verification"
  product_dir.parent().mkdir()
  product_dir.mkdir()
  let observed = fp"{fixture_root}/native-argv"
  fp"{product_dir}/xsht".write(
    f"""#!{ctx.xsh_bin}
p"{observed}".write(args.join("|"))?
""",
    mode: 0o755,
  )
  let workflow_context = fixtures.linux_context(fixture_root, "verification")?
  let rust_plan = tests.rust_plan(workflow_context, tests.Verification, privileged: true)?
  assert [spec.stage for spec in rust_plan] == [
    "test-products",
    "test-rust", "test-rust", "test-rust", "test-rust", "test-rust",
    "test-soundness", "test-debug-assertions", "test-xsht",
    "test-linux-priv", "test-linux-priv",
  ]

  if unix.id()?.euid == 0 {
    env HOST_UID="1000" HOST_GID="1000" {
      tests.native(workflow_context, tests.Verification, ["--jobs", "4"])
    }
  } else {
    tests.native(workflow_context, tests.Verification, ["--jobs", "4"])
  }
  assert observed.read_text()? == "test|--jobs|4"
}

test test_docker_gate_evaluates_context_root_path_expression { |ctx|
  let repository = fs.cwd()?
  let tools = test.temp_dir(ctx, name: "docker-gate-tools")?
  let cargo_home = fp"{tools}/cargo"
  let cargo_bin = fp"{cargo_home}/bin"
  cargo_home.mkdir()
  cargo_bin.mkdir()
  fp"{cargo_bin}/kache".write(
    f"""#!{ctx.xsh_bin}
exit 0
""",
    mode: 0o755,
  )
  let log = fp"{tools}/docker.log"
  fp"{tools}/docker".write(
    f"""#!{ctx.xsh_bin}
let log = p"{log}"
let previous = if log.exists()? {{ log.read_text()? }} else {{ "" }}
log.write(previous + args.join(" ") + "\\n")?
""",
    mode: 0o755,
  )
  let inherited_path = env.get_or("PATH", "")?
  let result = test.expect(
    ctx,
    f"""
use docker
use fixtures

proc main() [fs, process, env, error, io] -> Result[Unit] {{
  let ctx = fixtures.linux_context(p"{repository}")?
  docker.run_internal(ctx, "gate-lane", false, ["tests/xsh/float-equality.xsh"])?
}}

main()?
""",
    status: 0,
    stdout: [
      "docker-image-inspect target=x86_64-unknown-linux-musl",
      "docker-gate-lane target=x86_64-unknown-linux-musl",
    ],
    args: [],
    env: {
      PATH: f"{tools}:{inherited_path}",
      CARGO_HOME: cargo_home,
      XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_TEST_IMAGE_BUILD: "0",
      XSH_TEST_IMAGE: "xsh-test:cwd-regression",
    },
  )?
  assert "docker-image-inspect" in result.stdout
  assert "docker-gate-lane" in result.stdout
  assert log.read_lines()?.len() == 2
  assert f"{repository}/dev/musl-linker.sh:/usr/local/bin/xsh-mold-linker:ro" in log.read_text()?
}

test test_focused_catalogue_rejects_manual_closure_cycles_and_missing_cases {
  let ctx = fixtures.linux_context(/repo)?
  let command = gates.CatalogCase(
    id: "safe", tier: gates.Change, limit: gates.RuntimeSeconds(30),
    products: [tests.Xsht], prerequisite_ids: [], reasons: ["fixture"],
    execution: gates.RunCommand({stage: "safe", target: ctx.target.triple,
      executable: "never-start", argv: ["never-start"], cwd: ctx.root, environment: {}}),
  )
  let manual = {...command, id: "full", limit: gates.ManualOwner, execution: gates.ManualFull}
  let alias = {...command, id: "alias", prerequisite_ids: ["full"], execution: gates.Alias}
  let transit = {...command, id: "transit", prerequisite_ids: ["alias"]}
  let cycle = {...command, id: "cycle", prerequisite_ids: ["cycle"]}
  let scenario = {...command, id: "scenario", tier: gates.Scenario}
  let implicit_scenario = {...command, id: "implicit-scenario", prerequisite_ids: ["scenario"]}
  let catalogue = [command, manual, alias, transit, cycle, scenario, implicit_scenario]
  for id in ["full", "alias", "transit", "cycle", "missing", "implicit-scenario"] {
    match gates.resolve_plan(catalogue, ["safe", id], gates.Change) {
      Ok(_) => test.fail(f"invalid closure {id} was accepted")
      Err(error) => assert error.message == "PlanError.Invalid"
    }
  }
  let plan = gates.resolve_plan(catalogue, ["safe", "safe"], gates.Change)?
  assert [entry.id for entry in plan.cases] == ["safe"]
  assert plan.products == [tests.Xsht]
  assert gates.resolve_plan(catalogue, ["scenario"], gates.Change)?.cases[0].tier == gates.Scenario
}

test test_gate_request_rejects_all_nonliteral_full_routes {
  for arguments in [
    ["--batch"], ["--full", "src/sema.rs"], ["--plan", "--full"],
    ["--tier", "full"], ["--tier", "scenario"], ["--tier"],
    ["--case"], ["--case", "--full"], ["--json"], ["--from-plan", "saved.json"],
  ] {
    match gates.parse_request(arguments) {
      Ok(_) => test.fail(f"invalid gate request {arguments.join(" ")} was accepted")
      Err(error) => assert error.message == "PlanError.Invalid"
    }
  }
  let _ = gates.parse_request(["--full"])?
  let _ = gates.parse_request(["--plan", "--json", "--tier", "edit", "--case", "safe"])?
}

test test_catalogue_plans_tokei_without_showcase_fallback_and_marks_unknown_scope {
  let ctx = fixtures.linux_context(/repo)?
  let plan = gates.plan_paths(ctx, tests.Verification,
    ["showcase/tokei.xsh", "unknown/import.xsh"], [], gates.Change)?
  assert [entry.id for entry in plan.cases] == ["showcase/tokei/correctness"]
  assert plan.products == [tests.Xsh, tests.Xsht]
  assert plan.unresolved == ["unknown verification owner: unknown/import.xsh"]
  assert plan.cases[0].reasons == ["showcase/tokei.xsh output and classification contract"]
  match gates.plan_paths(ctx, tests.Verification, [], ["full"], gates.Change) {
    Ok(_) => test.fail("explicit case ID conferred manual authority")
    Err(error) => assert error.message == "PlanError.Invalid"
  }
}

test test_invalid_gate_dispatch_never_starts_docker_or_cargo { |ctx|
  test.timeout(ctx, 20s)
  let repository = fs.cwd()?
  let tools = test.temp_dir(ctx, name: "gate-forbidden-processes")?
  let marker = fp"{tools}/started"
  for name in ["cargo", "docker"] {
    fp"{tools}/{name}".write(f"""#!{ctx.xsh_bin}
p"{marker}".write("started")?
exit 99
""", mode: 0o755)
  }
  let inherited = env.get_or("PATH", "")?
  for arguments in [["--batch"], ["--full", "src/sema.rs"], ["--case", "full"], ["--tier", "full"], ["--from-plan", "plan.json"]] {
    let encoded = json.encode(arguments)?
    let _ = test.expect(ctx, f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  let ctx = fixtures.linux_context(p"{repository}")?
  let arguments = json.decode({json.encode(encoded)?})?.require(List[Str])?
  gates.execute(ctx, arguments)?
}}
main()?
""", status: 2, args: [], env: {{
      PATH: f"{tools}:{inherited}",
      XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
    }})?
    assert ! marker.exists()?
  }
}

test test_catalogue_core_case_keeps_product_dependencies_separate_from_ownership {
  let ctx = fixtures.linux_context(/repo)?
  let plan = gates.plan_paths(ctx, tests.Verification, ["core/lsmem.xsh"], [], gates.Edit)?
  assert plan.tier == gates.Edit
  assert [entry.id for entry in plan.cases] == ["core/lsmem/correctness"]
  assert plan.products == [tests.Xsh, tests.Xsht]
  assert plan.unresolved.is_empty()
  match plan.cases[0].execution {
    gates.RunCommand(spec) => assert spec.argv[1..] == ["test", "core/tests/test-lsmem.xsh"]
    gates.DocumentationCheck | gates.Alias | gates.ManualFull | gates.ManualPreparation(_) | gates.ManualCommand(_) | gates.ManualNative | gates.ManualDocumentation => test.fail("core correctness was not a native command")
  }
}

test test_catalogue_preserves_documentation_verification_and_products {
  let ctx = fixtures.linux_context(/repo)?
  let plan = gates.plan_paths(ctx, tests.Verification, ["docs/TESTING.md"], [], gates.Change)?
  assert [entry.id for entry in plan.cases] == ["check-docs"]
  assert plan.products == [tests.Xsh, tests.Xsht]
  assert plan.cases[0].execution == gates.DocumentationCheck
}

test test_catalogue_preserves_combined_route_witness_exclusions {
  let ctx = fixtures.linux_context(/repo)?
  let plan = gates.plan_paths(ctx, tests.Verification,
    ["src/runtime/eval/route_witness.rs", "crates/xsht/src/commands.rs", "tests/xsh/stdlib/env.xsh"], [], gates.Change)?
  let ids = [entry.id for entry in plan.cases]
  assert "test-route-witness-unit" in ids
  assert "test-route-witness-cli" not in ids
  assert "native-route-witness-env" not in ids
  assert "native-tests/xsh/stdlib/env.xsh" in ids
  let exact = gates.plan_paths(ctx, tests.Verification, [], ["test-route-witness-cli"], gates.Change)?
  assert [entry.id for entry in exact.cases] == ["test-route-witness-cli"]
}

test test_edit_rejects_change_only_cases_before_execution {
  let ctx = fixtures.linux_context(/repo)?
  for changed_path in ["src/sema/check/records.rs", "docs/TESTING.md"] {
    match gates.plan_paths(ctx, tests.Verification, [changed_path], [], gates.Edit) {
      Ok(_) => test.fail("Edit admitted a Change-only case")
      Err(error) => assert error.message == "PlanError.Invalid"
    }
  }
  assert gate_execution.limit_ms(30, 9000000)? == 9
  assert gate_execution.limit_ms(1, 30000000000)? == 1000
  match gate_execution.limit_ms(1, 0) {
    Ok(_) => test.fail("an exhausted runtime budget admitted a process")
    Err(error) => assert error.message == "LimitError.Expired"
  }
}

test test_bounded_gate_execution_preserves_finished_failed_and_cancelled_statuses { |ctx|
  test.timeout(ctx, 10s)
  let root = test.temp_dir(ctx, name: "gate-child-status")?
  for raw in [0, 7, 130] {
    let argv = ["sh", "-c", f"exit {raw}"]
    let spec = {stage: "child", target: "fixture", executable: "sh", argv: argv, cwd: root, environment: {}}
    match gate_execution.execute(spec, 1000)? {
      gate_execution.Finished(status) => assert status.shell_code()? == raw
      gate_execution.TimedOut(_) | gate_execution.Cancelled(_) | gate_execution.CouldNotExecute(_) => test.fail("completed child lost its raw status")
    }
  }
}

test test_bounded_gate_timeout_consumes_leader_and_descendant_cleanup { |ctx|
  test.timeout(ctx, 10s)
  let root = test.temp_dir(ctx, name: "gate-timeout-cleanup")?
  let leader_file = fp"{root}/leader"
  let child_file = fp"{root}/child"
  let source = f"echo $$ > '{leader_file}'; sleep 30 & echo $! > '{child_file}'; wait"
  let spec = {stage: "deadline", target: "fixture", executable: "sh", argv: ["sh", "-c", source], cwd: root, environment: {}}
  match gate_execution.execute(spec, 100)? {
    gate_execution.TimedOut(cause) => assert cause is ProcessError.Timeout {..}
    gate_execution.Finished(_) | gate_execution.Cancelled(_) | gate_execution.CouldNotExecute(_) => test.fail("deadline did not stop owned work")
  }
  for file in [leader_file, child_file] {
    let pid = file.read_text()?.trim() as Int
    match process.kill(pid, signal: "0") {
      Err(_) => {}
      Ok(_) => test.fail(f"owned process {pid} survived its consumed deadline")
    }
  }
}

test test_gate_prepares_each_cargo_harness_once_and_runs_filters_without_cargo { |ctx|
  test.timeout(ctx, 20s)
  let repository = fs.cwd()?
  let root = test.temp_dir(ctx, name: "gate-prepared-harness")?
  for name in ["Cargo.toml", "rust-toolchain.toml", "xsht-config.ini"] { fp"{root}/{name}".write("") }
  fp"{root}/dev".mkdir()
  fp"{root}/target".mkdir()
  fp"{root}/tools".mkdir()
  fp"{root}/dev/gate_case.xsh".write(fp"{repository}/dev/gate_case.xsh".read_bytes()?)
  let cargo_calls = fp"{root}/cargo-calls"
  let harness_calls = fp"{root}/harness-calls"
  let harness = fp"{root}/target/prepared-test"
  harness.write(f"""#!{ctx.xsh_bin}
let output = p"{harness_calls}"
let previous = if output.exists()? {{ output.read_text()? }} else {{ "" }}
output.write(previous + args.join(" ") + "\\n")?
""", mode: 0o755)
  fp"{root}/tools/cargo".write(f"""#!{ctx.xsh_bin}
let output = p"{cargo_calls}"
let previous = if output.exists()? {{ output.read_text()? }} else {{ "" }}
output.write(previous + args.join(" ") + "\\n")?
if args[0] == "test" {{
  if "--no-run" not in args {{ exit 99 }}
  print json.encode({{reason: "compiler-artifact", profile: {{test: true}}, executable: "{harness}"}})?
  print json.encode({{reason: "build-finished", success: true}})?
}}
""", mode: 0o755)
  let inherited = env.get_or("PATH", "")?
  let request_source = f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  let ctx = fixtures.linux_context(p"{root}")?
  gates.lane(ctx, ["--case", "test-route-witness-unit", "--case", "test-route-witness-inherited-env"])?
}}
main()?
"""
  let _ = test.expect(ctx, request_source, status: 0, args: [], env: {{PATH: f"{root}/tools:{inherited}",
    XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev", XSH_DEV_BUILD_PRELOAD: ""}})?
  let compile_calls = cargo_calls.read_lines()? |> where .starts_with("test ")
  assert compile_calls.len() == 1
  assert "--no-run" in compile_calls[0]
  assert "--message-format=json-render-diagnostics" in compile_calls[0]
  assert harness_calls.read_lines()? == ["runtime::eval::route_witness::tests", "runtime_env_route_witness_tests"]
  fp"{root}/dev/gate_case.xsh".write("exit 130\n")
  let _ = test.expect(ctx, request_source, status: 130, stdout: ["raw_status=130"], args: [], env: {{
    PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
    XSH_DEV_BUILD_PRELOAD: "",
  }})?
  fp"{root}/tools/cargo".write("#!/bin/sh\nexit 7\n", mode: 0o755)
  let _ = test.expect(ctx, request_source, status: 1, stdout: ["execution_error=exit 7"], args: [], env: {{
    PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
    XSH_DEV_BUILD_PRELOAD: "",
  }})?
  assert harness_calls.read_lines()?.len() == 2
}

test test_gate_cancel_consumes_owned_descendants_before_exit_130 { |ctx|
  test.timeout(ctx, 10s)
  let repository = fs.cwd()?
  let root = test.temp_dir(ctx, name: "gate-cancel-cleanup")?
  let child_file = fp"{root}/child"
  let ready = fp"{root}/ready"
  for name in ["Cargo.toml", "rust-toolchain.toml", "xsht-config.ini"] { fp"{root}/{name}".write("") }
  let script = fp"{repository}/dev/main.xsh"
  fp"{root}/tools".mkdir()
  fp"{root}/target".mkdir()
  fp"{root}/target/x86_64-unknown-linux-musl".mkdir()
  let products = fp"{root}/target/x86_64-unknown-linux-musl/verification"
  products.mkdir()
  fp"{root}/tools/cargo".write("#!/bin/sh\nexit 0\n", mode: 0o755)
  fp"{products}/xsht".write(f"#!/bin/sh\nsleep 30 & echo $! > '{child_file}'; touch '{ready}'; wait\n", mode: 0o755)
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let command = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--", "internal", "gate-lane", "--case", "showcase/tokei/correctness"],
    cwd: root, env: {XSH_MODULE_PATH: f"{repository}/dev:{repository}/dev/tests",
      PATH: f"{root}/tools:{env.get_or("PATH", "")?}", XSH_DEV_BUILD_PRELOAD: "", CARGO_TARGET_DIR: fp"{root}/target".display()}, stdout: stdout, stderr: stderr, timeout: 7s)
  let handle = spawn command?
  defer handle.cancel(signal: "KILL", kill_after: 0ms)
  let started = time.monotonic()?
  while ! ready.exists()? {
    test.fail(stderr.read_text()?) when time.monotonic()? - started > 2000000000
    time.sleep(5ms)
  }
  process.kill(handle.pid, signal: "TERM")
  let finished = process.wait_timeout([handle], 2s)?
  if let observed = finished {
    assert observed.status.shell_code()? == 130, stdout.read_text()? + stderr.read_text()?
  } else {
    handle.cancel(signal: "KILL", kill_after: 0ms)
    test.fail("cancelled gate driver did not finish")
  }
  let pid = child_file.read_text()?.trim() as Int
  match process.kill(pid, signal: "0") {
    Err(_) => {}
    Ok(_) => test.fail(f"cancelled gate left descendant {pid} alive")
  }
}

test test_gate_container_preserves_selected_failure_incomplete_and_cancel_statuses { |ctx|
  test.timeout(ctx, 15s)
  let repository = fs.cwd()?
  let tools = test.temp_dir(ctx, name: "gate-container-status")?
  let cargo_home = fp"{tools}/cargo"
  cargo_home.mkdir()
  fp"{cargo_home}/bin".mkdir()
  fp"{cargo_home}/bin/kache".write("#!/bin/sh\nexit 0\n", mode: 0o755)
  let requested_status = fp"{tools}/requested-status"
  fp"{tools}/docker".write(f"""#!{ctx.xsh_bin}
if args[0] == "image" {{ exit 0 }}
let requested = p"{requested_status}".read_text()?.trim() as Int
exit requested
""", mode: 0o755)
  let inherited = env.get_or("PATH", "")?
  for status in [1, 2, 130] {
    requested_status.write(f"{status}")
    let _ = test.expect(ctx, f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  gates.execute(fixtures.linux_context(p"{repository}")?, ["--case", "showcase/tokei/correctness"])?
}}
main()?
""", status: status, stdout: [f"raw_status={status}"], args: [], env: {{
      PATH: f"{tools}:{inherited}", CARGO_HOME: cargo_home,
      XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_TEST_IMAGE_BUILD: "0", XSH_TEST_IMAGE: "xsh-test:status-fixture",
    }})?
  }
}

test test_native_file_case_ids_replay_the_same_owned_case_and_products {
  let repository = fs.cwd()?
  let ctx = fixtures.linux_context(repository)?
  let path_plan = gates.plan_paths(ctx, tests.Verification, ["tests/xsh/stdlib/env.xsh"], [], gates.Change)?
  assert path_plan.cases.len() == 1
  let id = path_plan.cases[0].id
  let replay = gates.plan_paths(ctx, tests.Verification, [], [id], gates.Change)?
  assert [entry.id for entry in replay.cases] == [id]
  assert replay.cases[0].execution == path_plan.cases[0].execution
  assert replay.cases[0].tier == path_plan.cases[0].tier
  assert replay.products == path_plan.products
  for ids in [
    ["native-tests/xsh/not-a-real-test.xsh"], ["native-src/runner.xsh"], ["native-unknown/test.xsh"],
    ["native-/etc/passwd.xsh"], ["native-tests/xsh/../../outside.xsh"], [id, id],
  ] {
    match gates.plan_paths(ctx, tests.Verification, [], ids, gates.Change) {
      Ok(_) => test.fail("invalid native file selection was accepted")
      Err(error) => assert error.message == "PlanError.Invalid"
    }
  }
  match gates.parse_request(["--case", id, "--case", id]) {
    Ok(_) => test.fail("duplicate explicit case IDs were accepted")
    Err(error) => assert error.message == "PlanError.Invalid"
  }
}

test test_native_file_case_discovery_rejects_symlink_escapes { |ctx|
  let root = test.temp_dir(ctx, name: "native-case-owned-root")?
  let outside = test.temp_dir(ctx, name: "native-case-outside-root")?
  fp"{root}/tests".mkdir()
  fp"{root}/tests/xsh".mkdir()
  let target = fp"{outside}/external.xsh"
  target.write("test test_external {}")
  fs.symlink(target, fp"{root}/tests/xsh/escape.xsh")
  match gates.plan_paths(fixtures.linux_context(root)?, tests.Verification, [], ["native-tests/xsh/escape.xsh"], gates.Change) {
    Ok(_) => test.fail("native case discovery escaped its owned root")
    Err(error) => assert error.message == "PlanError.Invalid"
  }
}

test test_full_catalogue_validates_alias_dependency_and_product_closure {
  let ctx = fixtures.linux_context(/repo)?
  let command = {stage: "substitute", target: ctx.target.triple, executable: "never-start",
    argv: ["never-start"], cwd: ctx.root, environment: {}}
  let base = gates.CatalogCase(id: "prepare", tier: gates.Change, limit: gates.ManualOwner,
    products: [], prerequisite_ids: [], reasons: ["fixture"],
    execution: gates.ManualPreparation({products: [tests.Xsht], command: command}))
  let check = {...base, id: "check", products: [tests.Xsht], prerequisite_ids: ["prepare"], execution: gates.ManualCommand(command)}
  let alias = {...base, id: "alias", prerequisite_ids: ["check"], execution: gates.Alias}
  let full = {...base, id: "full", prerequisite_ids: ["alias"], execution: gates.ManualFull}
  let valid = gates.resolve_full([base, check, alias, full])?
  assert [entry.id for entry in valid.cases] == ["prepare", "check", "alias", "full"]
  assert valid.products == [tests.Xsht]
  let missing = {...check, prerequisite_ids: ["missing"]}
  let cycle = {...check, prerequisite_ids: ["alias"]}
  let unprepared = {...check, products: [tests.Xshi]}
  for catalogue in [
    [base, missing, alias, full], [base, cycle, alias, full],
    [base, unprepared, alias, full], [base, check, check, alias, full],
    [base, check, alias], [base, check, alias, full, full],
  ] {
    match gates.resolve_full(catalogue) {
      Ok(_) => test.fail("invalid manual closure was accepted")
      Err(error) => assert error.message == "PlanError.Invalid"
    }
  }
  match gates.resolve_plan([base, check, alias, full], ["alias"], gates.Change) {
    Ok(_) => test.fail("a manual child acquired focused authority through its alias")
    Err(error) => assert error.message == "PlanError.Invalid"
  }
}

test test_full_catalogue_preserves_the_existing_batch_owner_sequence {
  let ctx = fixtures.linux_context(/repo)?
  let rust = tests.rust_plan(ctx, tests.Verification, privileged: true)?
  let plan = gates.full_plan(ctx, tests.Verification)?
  let commands = collect {
    for entry in plan.cases {
      match entry.execution {
        gates.ManualPreparation(preparation) => yield preparation.command
        gates.ManualCommand(spec) => yield spec
        gates.RunCommand(_) | gates.DocumentationCheck | gates.Alias | gates.ManualFull | gates.ManualNative | gates.ManualDocumentation => {}
      }
    }
  }
  assert commands == [@rust, @gates.batch_checks(ctx, tests.Verification)]
  assert [entry.id for entry in plan.cases |> where .execution == gates.ManualNative] == ["full/native"]
  assert [entry.id for entry in plan.cases |> where .execution == gates.ManualDocumentation] == ["full/docs"]
  assert plan.products == tests.all_products()
}

test test_manual_dispatch_validates_every_closure_before_any_substitute_process { |ctx|
  test.timeout(ctx, 15s)
  let repository = fs.cwd()?
  let root = test.temp_dir(ctx, name: "full-dispatch-contract")?
  let marker = fp"{root}/started"
  let runner = fp"{root}/substitute.sh"
  runner.write("printf '%s\\n' \"$1\" >> \"$2\"\n")
  let source = f"""
use gates
use fixtures
use test_workflows as tests
proc main(...arguments: List[Str]) [fs, process, env, time, error, io] {{
  let ctx = fixtures.linux_context(p"{root}")?
  let prepare_command = {{stage: "fixture-prepare", target: ctx.target.triple, executable: "sh",
    argv: ["sh", "{runner}", "prepare", "{marker}"], cwd: ctx.root, environment: {{}}}}
  let check_command = {{...prepare_command, stage: "fixture-check", argv: ["sh", "{runner}", "check", "{marker}"]}}
  let preparation = gates.CatalogCase(id: "prepare", tier: gates.Change, limit: gates.ManualOwner,
    products: [], prerequisite_ids: [], reasons: ["fixture"],
    execution: gates.ManualPreparation({{products: [tests.Xsht], command: prepare_command}}))
  let check = {{...preparation, id: "check", products: [tests.Xsht], prerequisite_ids: ["prepare"], execution: gates.ManualCommand(check_command)}}
  let alias = {{...preparation, id: "alias", prerequisite_ids: ["check"], execution: gates.Alias}}
  let full = {{...preparation, id: "full", prerequisite_ids: ["alias"], execution: gates.ManualFull}}
  let variant = arguments[0]
  let entries = match variant {{
    "missing" => [preparation, {{...check, prerequisite_ids: ["missing"]}}, alias, full]
    "cycle" => [preparation, {{...check, prerequisite_ids: ["alias"]}}, alias, full]
    "duplicate" => [preparation, check, check, alias, full]
    "missing-root" => [preparation, check, alias]
    "wrong-root" => [preparation, check, alias, {{...full, execution: gates.RunCommand(check_command)}}]
    "empty-root" => [preparation, check, alias, {{...full, prerequisite_ids: []}}]
    "unprovided" => [{{...preparation, execution: gates.ManualPreparation({{products: [], command: prepare_command}})}}, check, alias, full]
    "manual-runtime-limit" => [{{...preparation, limit: gates.RuntimeSeconds(1)}}, check, alias, full]
    "automatic-manual-owner" => [preparation, {{...check, execution: gates.RunCommand(check_command)}}, alias, full]
    "zero-runtime" => [preparation, {{...check, limit: gates.RuntimeSeconds(0), execution: gates.RunCommand(check_command)}}, alias, full]
    "product-order" => [preparation, {{...check, prerequisite_ids: []}}, alias, {{...full, prerequisite_ids: ["check", "prepare"]}}]
    else => [preparation, check, alias, full]
  }}
  let request = if variant == "focused" {{ gates.parse_request(["--case", "alias"])? }} else {{ gates.parse_request(["--full"])? }}
  gates.dispatch_catalogue(ctx, tests.Verification, request, entries)?
}}
"""
  for variant in ["missing", "cycle", "duplicate", "missing-root", "wrong-root", "empty-root", "unprovided", "product-order", "manual-runtime-limit", "automatic-manual-owner", "zero-runtime", "focused"] {
    let _ = test.expect(ctx, source, status: 2, args: [variant],
      env: {XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev"})?
    assert ! marker.exists()?
  }
  let _ = test.expect(ctx, source, status: 0, args: ["valid"],
    env: {XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev"})?
  assert marker.read_lines()? == ["prepare", "check"]
}

test test_internal_full_rejects_automatic_operation_identity_before_effects {
  let ctx = fixtures.linux_context(/path-that-must-not-be-created)?
  for operation in ["", "gate-lane", "test-linux", "plan.json"] {
    match container_internal.gate_full(ctx, operation) {
      Ok(_) => test.fail("an automatic or serialized operation acquired Full authority")
      Err(error) => assert error.message == "PlanError.Invalid"
    }
  }
}
