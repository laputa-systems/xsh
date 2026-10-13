use fixtures
use docker
use gates
use test_workflows as tests

# Combining owners preserves their required gates without scheduling a second
# copy for every changed file in the same source family.
test test_changed_paths_select_and_deduplicate_their_verification_owners {
  let selected = gates.select(["src/sema/check/records.rs", "src/sema/check/results.rs", "src/runtime/eval/indexed/full.rs", "tests/xsh/retry.xsh", "tests/xsh/retry.xsh"])?
  assert selected.areas == [gates.Checker, gates.Lowering]
  assert selected.native_files == ["tests/xsh/retry.xsh"]
  assert gates.areas_for("src/syntax/grammar.rs")? == [gates.Syntax, gates.Grammar, gates.Docs]
  assert gates.areas_for("docs/snippets/api/fs.xsh")? == [gates.Docs, gates.Api]
  match gates.select(["unowned-family/behavior.rs"]) {
    Ok(_) => test.fail("a changed path without a gate owner was accepted")
    Err(error) => assert error.message == "GateError.UnmappedPath"
  }
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
      let product = tests.product_path(ctx, profile, "xsht")
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

test test_catalog_and_sibling_filters_preserve_the_selected_verification_scope {
  let ctx = fixtures.linux_context(/repo)?
  let selected = gates.select(["src/stdlib.rs", "crates/xsht/src/lint.rs", "tests/syntax.rs"])?
  let plan = gates.lane_plan(ctx, tests.Verification, selected)?
  let catalog = [spec for spec in plan |> where .stage == "test-stdlib-catalog"]
  assert catalog.len() == 1
  assert "--lib" in catalog[0].argv
  assert "--profile" not in catalog[0].argv
  assert "stdlib::tests::every_catalog_module_parses_checks_and_lowers" in catalog[0].argv
  for spec in plan {
    if spec.stage == "test-tooling" {
      assert "lint_format_invariance::formatting_preserves_lints_on_laputa_corpus" in spec.argv
      assert "--skip" in spec.argv
    }
    if spec.stage == "test-syntax" {
      assert "syntax::formatter_is_idempotent_on_laputa_corpus" in spec.argv
      assert "--skip" in spec.argv
    }
  }
}

test test_batch_automatic_checks_use_retained_repository_gates {
  let ctx = fixtures.linux_context(/repo)?
  let checks = gates.batch_checks(ctx, tests.Verification)
  assert [spec.stage for spec in checks] == ["batch-check", "batch-lint", "batch-diff", "batch-ratchets"]
  let ratchets = checks[3]
  assert ratchets.argv[1] == "dev/consolidation/metrics.xsh"
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

test test_docker_gate_captures_repository_root_before_inspecting_image { |ctx|
  let repository = fs.cwd()?
  let tools = test.temp_dir(ctx, name: "docker-gate-tools")?
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
      XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_TEST_IMAGE_BUILD: "0",
      XSH_TEST_IMAGE: "xsh-test:cwd-regression",
    },
  )?
  assert "docker-image-inspect" in result.stdout
  assert "docker-gate-lane" in result.stdout
  assert log.read_lines()?.len() == 2
}
