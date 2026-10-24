error ScopeCompletionFailure = Rejected(message: Str)
var scope_calls = 0
var scope_cleaned = 0
var scope_events: List[Str] = []

pure scope_failure() -> Error { ScopeCompletionFailure.Rejected(message: "retained") }

proc scope_stage(failed: Bool) [env, error] -> Result[Unit, Error] {
  scope_calls += 1
  defer {
    scope_cleaned += 1
    scope_events += [env.get_or("XSH_COMPLETION_SCOPE", "outside")?]
  }
  if failed { Err(scope_failure()) } else { Ok() }
}

proc written_scope(failed: Bool, scoped: Bool) [env, error] -> Result[Unit, Error] {
  if scoped {
    env ({XSH_COMPLETION_SCOPE: "inner"}) { scope_stage(failed) }
  } else {
    scope_stage(failed)
  }
}

proc inferred_scope(failed, scoped) {
  if scoped {
    env ({XSH_COMPLETION_SCOPE: "inner"}) { scope_stage(failed) }
  } else {
    scope_stage(failed)
  }
}

proc inferred_row_scope(step) {
  let preload = step.preload
  if preload == null {
    scope_stage(step.failed)
  } else {
    env ({XSH_COMPLETION_SCOPE: preload}) { scope_stage(step.failed) }
  }
}

proc written_nested_scope(failed: Bool) [env, error] -> Result[Result[Unit, Error], Error] {
  env ({XSH_COMPLETION_SCOPE: "written-value"}) { scope_stage(failed) }
}

proc verify_statement_scope_completion() [env, error] -> Result[Unit, Error] {
  let before = env.get_or("XSH_COMPLETION_SCOPE", "outside")?
  env ({XSH_COMPLETION_SCOPE: "outer"}) {
    for failed in [false, true] {
      for scoped in [false, true] {
        let written = try { written_scope(failed, scoped)? }
        let inferred = try { inferred_scope(failed, scoped)? }
        assert written == inferred
        if failed {
          match inferred {
            Err(ScopeCompletionFailure.Rejected {message}) => assert message == "retained"
            else => assert false
          }
        } else {
          assert inferred == Ok()
        }
        assert env.get_or("XSH_COMPLETION_SCOPE", "outside")? == "outer"
      }
      let direct = try { inferred_row_scope({failed, preload: null})? }
      let scoped = try { inferred_row_scope({failed, preload: "inner"})? }
      assert direct == scoped
      if failed { assert direct is Err(_) } else { assert direct == Ok() }
    }
  }?
  let success = env ({XSH_COMPLETION_SCOPE: "value-success"}) { scope_stage(false) }
  let failure = env ({XSH_COMPLETION_SCOPE: "value-failure"}) { scope_stage(true) }
  assert success is Ok(Ok(_))
  assert failure is Ok(Err(_))
  let retained = written_nested_scope(true)?
  assert retained == Err(scope_failure())
  let opaque: Result[Unit, Str] = Err("opaque")
  let nested = env ({XSH_COMPLETION_SCOPE: "opaque"}) { opaque }
  assert nested == Ok(opaque)
  assert scope_calls == 15
  assert scope_cleaned == 15
  assert scope_events == ["outer", "outer", "inner", "inner", "outer", "inner", "outer", "outer", "inner", "inner", "outer", "inner", "value-success", "value-failure", "written-value"]
  assert env.get_or("XSH_COMPLETION_SCOPE", "outside")? == before
  Ok()
}
