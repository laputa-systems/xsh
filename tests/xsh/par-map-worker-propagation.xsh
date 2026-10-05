# A `par-map` worker that binds a callee's `Result` owns that failure: the
# callee's `?` returns `Err` to the worker, the callee's defers run first, and
# the worker goes on to record the outcome. The proving step below has no
# declared return type, so its `Result[Unit]` is inferred, and it fails from
# inside an `env` scope behind a deferred cleanup, the shape that was once
# reported to unwind straight out of the worker and was worked around with
# `match prove() { Ok(_) => {} Err(e) => return Err(e) }`.

const NAMES = ["good-1", "bad-1", "skip", "run-bad-1", "good-2", "bad-2"]

error ProofError = Rejected(message: Str)

proc check_payload(name: Str) [error] -> Result[Unit, ProofError] {
  return Err(ProofError.Rejected(f"{name} rejected")) when name.starts_with("bad")
}

proc prove(root: Path, name: Str) [fs, process, env, error] {
  let cleaned = fp"{root}/{name}.proof-cleaned"
  defer cleaned.write("")?
  return when name == "skip"
  env ({PROOF_TARGET: name}) {
    check_payload(name)
    # A failed plain `run` is this proc's `Err` as well.
    if name.starts_with("run-bad") {
      run false
    }
  }
  fp"{root}/{name}.proved".write("")
}

proc build_with_propagation(root: Path, name: Str) [fs, process, env, error] -> Result[Str] {
  let cleaned = fp"{root}/{name}.build-cleaned"
  defer cleaned.write("")?
  prove(root, name)
  fp"{root}/{name}.committed".write("")
  f"{name} receipt"
}

proc assert_worker_outcomes(root: Path, names: List[Str], outcomes: List[Result[Str]]) [fs, error] {
  assert outcomes.len() == names.len()
  for index in range(names.len()) {
    let name = names[index]
    let failed = "bad" in name
    # Every worker reached its status marker, after both callees cleaned up.
    assert fp"{root}/{name}.status".read_text()? == (if failed { "failed" } else { "built" })
    assert fp"{root}/{name}.proof-cleaned".exists()?
    assert fp"{root}/{name}.build-cleaned".exists()?
    assert fp"{root}/{name}.committed".exists()? == ! failed
    if failed {
      if let Err(problem) = outcomes[index] {
        if name.starts_with("bad") {
          assert problem is ProofError.Rejected
          assert problem.message == f"{name} rejected"
        }
      } else {
        assert false, f"{name} must fail"
      }
    } else {
      assert outcomes[index]? == f"{name} receipt"
    }
  }
}

test test_par_map_worker_keeps_a_propagated_callee_failure_as_data { |ctx|
  for jobs in [1, 2, 6] {
    let root = test.temp_dir(ctx, name: f"par-map-worker-propagation-{jobs}")?
    let outcomes = NAMES |> par-map(jobs:) { |name|
      let outcome = build_with_propagation(root, name)
      fp"{root}/{name}.status".write(if outcome is Ok(_) { "built" } else { "failed" })
      outcome
    }
    assert_worker_outcomes(root, NAMES, outcomes)
  }
}

# The same boundary holds when the worker itself is a proc that captures the
# build with `try` and when the stage runs under an enclosing function that
# has cleanup of its own.
proc run_stage(root: Path, names: List[Str]) [fs, process, env, error] -> Result[List[Result[Str]]] {
  defer fp"{root}/stage-cleaned".write("")?
  let outcomes = names |> par-map(jobs: 2) { |name|
    let outcome = try { build_with_propagation(root, name)? }
    fp"{root}/{name}.status".write(if outcome is Ok(_) { "built" } else { "failed" })
    outcome
  }
  assert ! fp"{root}/stage-cleaned".exists()?
  outcomes
}

test test_par_map_worker_capture_runs_before_the_callers_cleanup { |ctx|
  let root = test.temp_dir(ctx, name: "par-map-worker-capture")?
  let outcomes = run_stage(root, NAMES)?
  assert fp"{root}/stage-cleaned".exists()?
  assert_worker_outcomes(root, NAMES, outcomes)
}
