use docker
use fixtures
use gates
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
