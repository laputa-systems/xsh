error ContextFailure = Failed(kind: Str, message: Str)
error ContextCause = Failed(message: Str)

pure contextual_tail(cause_message: Str, stage: Str) -> Result[Int, Error] {
  Err(ContextFailure.Failed(kind: "compiler-failed", message: "outer"), cause: ContextCause.Failed(cause_message)).context(stage, "requested preparation")
}

pure contextual_failure(cause_message: Str, stage: Str) -> Result[Unit, Error] {
  let original: Result[Unit, Error] = Err(
    ContextFailure.Failed(kind: "compiler-failed", message: "outer"),
    cause: ContextCause.Failed(cause_message),
  ).context(stage, "requested preparation")
  original
}

pure contextual_default() -> Result[Unit, Error] {
  let original: Result[Unit, Error] = Err(ContextFailure.Failed(kind: "default", message: "outer")).context("default-stage")
  original
}

var context_events: List[Str] = []

proc ordered_context_error() -> ContextFailure {
  defer { context_events += ["error-close"] }
  context_events += ["error"]
  ContextFailure.Failed(kind: "ordered", message: "outer")
}

proc ordered_context_cause() -> ContextCause {
  defer { context_events += ["cause-close"] }
  context_events += ["cause"]
  ContextCause.Failed("cause")
}

proc ordered_context_kind() -> Str { context_events += ["kind"]; "stage" }
proc ordered_context_message() -> Str { context_events += ["message"]; "detail" }

proc fresh_result_context() -> Result[Unit] {
  let ordered: Result[Unit, Error] = Err(ordered_context_error(), cause: ordered_context_cause()).context(ordered_context_kind(), ordered_context_message())
  assert context_events == ["error", "error-close", "cause", "cause-close", "kind", "message"]
  match ordered {
    Err(ContextFailure.Failed {kind, message}) => { assert kind == "ordered"; assert message == "outer" }
    else => assert false
  }
  let success: Result[Int] = Ok(7).context("success")
  assert success == Ok(7)
  let original = contextual_failure("input unavailable", "kernel-scenario")
  let tail = contextual_tail("input unavailable", "kernel-scenario")
  assert tail == contextual_tail("input unavailable", "kernel-scenario")
  assert tail != contextual_tail("different input", "kernel-scenario")
  assert tail != contextual_tail("input unavailable", "different scenario")
  assert original == contextual_failure("input unavailable", "kernel-scenario")
  assert original != contextual_failure("different input", "kernel-scenario")
  assert original != contextual_failure("input unavailable", "different scenario")
  match original {
    Err(ContextFailure.Failed {kind, message}) => {
      assert kind == "compiler-failed"
      assert message == "outer"
    }
    else => assert false
  }
  match contextual_default() {
    Err(ContextFailure.Failed {kind, message}) => {
      assert kind == "default"
      assert message == "outer"
    }
    else => assert false
  }
}
