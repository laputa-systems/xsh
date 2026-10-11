use fixtures
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
  env DIST_PROFILE=verification { assert tests.execution_profile()? == tests.Verification }
  env DIST_PROFILE=release { assert tests.execution_profile()? == tests.Release }
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

test test_batch_automatic_checks_leave_paused_compatibility_inputs_alone {
  let ctx = fixtures.linux_context(/repo)?
  let checks = gates.batch_checks(ctx, tests.Verification)
  assert [spec.stage for spec in checks] == ["batch-check", "batch-lint", "batch-diff", "batch-ratchets"]
  let ratchets = checks[3]
  assert ratchets.argv[1] == "dev/consolidation/metrics.xsh"
  assert ["--root", ctx.root.display(), "--source-root", "src", "--xsht-source-root", "crates/xsht/src"] == ratchets.argv[3..9]
  assert ["--baseline", "dev/consolidation/baseline.json", "--envelope", "dev/consolidation/envelope.json", "--output", ".work/consolidation/current.json", "--audit", ".work/consolidation/rises.json"] == ratchets.argv[9..]
  for spec in checks {
    assert spec.executable != "python3"
    assert [arg for arg in spec.argv |> where .starts_with("dev/compat/")].is_empty()
  }
  let paused = gates.select(["dev/compat/policy.xsh"])?
  assert paused.areas.is_empty()
  assert paused.native_files.is_empty()
}
