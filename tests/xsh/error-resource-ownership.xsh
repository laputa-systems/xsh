error ChildError = Owned(child: ProcessHandle)

error WrapperError = Failed

type CauseChildBundle = {attached: Result[Unit, WrapperError], pid: Int}

proc return_error_child() [process, error] {
  let child = spawn run sh -c "sleep 10" ?
  Err(ChildError.Owned(child:))
}

proc return_cause_child() [process, error] -> Result[CauseChildBundle] {
  let child = spawn run sh -c "sleep 10" ?
  CauseChildBundle(
    Err(.Failed("outer"), cause: ChildError.Owned(child:)),
    child.pid,
  )
}

test test_error_payload_transfers_child_to_caller {
  match return_error_child() {
    Err(ChildError.Owned {child: child}) => {
      defer child.cancel(signal: "TERM", kill_after: 0ms)
      assert process.list()? |> any .pid == child.pid, "returned error payload must retain its owned child"
    }
    Ok(_) => test.fail("expected child error")
  }
}

test test_error_cause_transfers_child_to_caller {
  let bundle = return_cause_child()?
  defer process.kill(bundle.pid, signal: "TERM")
  assert process.list()? |> any .pid == bundle.pid, "child held only by a typed cause must survive callee cleanup"
}

error CauseOwnerError = Failed(pid: Int)

test test_try_error_payload_transfers_child_before_nested_cleanup {
  let captured: Result[Unit] = try {
    let child = spawn run sh -c "sleep 10" ?
    {
      Err(ChildError.Owned(child:))?
    }
  }
  match captured {
    Err(ChildError.Owned {child: child}) => {
      defer child.cancel(signal: "TERM", kill_after: 0ms)
      assert process.list()? |> any .pid == child.pid, "capture must retain a child owned by an outer discarded block"
    }
    Err(error) => test.fail(error.message)
    Ok(_) => test.fail("expected captured child error")
  }
}

test test_try_error_cause_transfers_child_before_cleanup {
  let captured: Result[Unit] = try {
    let child = spawn run sh -c "sleep 10" ?
    Err(CauseOwnerError.Failed(pid: child.pid), cause: ChildError.Owned(child:))?
  }
  match captured {
    Err(CauseOwnerError.Failed {pid: pid}) => {
      defer process.kill(pid, signal: "TERM")
      assert process.list()? |> any .pid == pid, "capture must retain a child held only by a cause"
    }
    Err(error) => test.fail(error.message)
    Ok(_) => test.fail("expected captured cause")
  }
}

test test_driver_try_error_payload_and_cause_transfer { |ctx|
  let output = test.expect(
    ctx,
    r"""error ChildError = Owned(child: ProcessHandle)
error CauseOwnerError = Failed(pid: Int)
let direct: Result[Unit] = try {
  let child = spawn run sh -c "sleep 10" ?
  if true { Err(ChildError.Owned(child: child))? }
}
match direct {
  Err(ChildError.Owned {child}) => {
    defer child.cancel(signal: "TERM", kill_after: 0ms)
    test.ok(process.list()? |> any .pid == child.pid, "driver payload child")?
  }
  Err(error) => test.fail(error.message)?
  Ok(_) => test.fail("missing payload failure")?
}
let caused: Result[Unit] = try {
  let child = spawn run sh -c "sleep 10" ?
  Err(CauseOwnerError.Failed(pid: child.pid), cause: ChildError.Owned(child: child))?
}
match caused {
  Err(CauseOwnerError.Failed {pid}) => {
    defer process.kill(pid, signal: "TERM")
    test.ok(process.list()? |> any .pid == pid, "driver cause child")?
  }
  Err(error) => test.fail(error.message)?
  Ok(_) => test.fail("missing caused failure")?
}
print "live"
""",
    status: 0,
  )?
  assert output.stdout == """live
"""
}

proc propagate_child_from_plain_callee() [process, error] -> Unit {
  let child = spawn run sh -c "sleep 10" ?
  {
    Err(ChildError.Owned(child:))?
  }
}

test test_try_plain_callee_failure_transfers_child {
  let captured = try {
    propagate_child_from_plain_callee()
  }
  match captured {
    Err(ChildError.Owned {child: child}) => {
      defer child.cancel(signal: "TERM", kill_after: 0ms)
      assert process.list()? |> any .pid == child.pid, "checked runtime transport must retain the callee child"
    }
    Err(error) => test.fail(error.message)
    Ok(_) => test.fail("missing callee failure")
  }
}

test test_scoped_propagated_resource_cannot_reach_outer_try { |ctx|
  let output = test.run_script(
    ctx,
    r"""error ChildError = Owned(child: ProcessHandle)
let captured: Result[Unit] = try {
  env ({XSH_SCOPE_CHILD: "inner"}) {
    let child = spawn run sh -c "sleep 10" ?
    Err(ChildError.Owned(child: child))?
  }?
}
print "unreachable"
""",
  )?
  assert ! output.success, "scoped resource propagation must be rejected"
  assert "cannot escape a restored context" in output.stderr
}

test test_inner_try_retains_resource_inside_same_context {
  let _ = env ({XSH_SCOPE_CHILD: "inner"}) {
    let captured: Result[Unit] = try {
      let child = spawn run sh -c "sleep 10" ?
      Err(ChildError.Owned(child:))?
    }
    match captured {
      Err(ChildError.Owned {child: child}) => {
        defer child.cancel(signal: "TERM", kill_after: 0ms)
        assert process.list()? |> any .pid == child.pid, "inner capture may consume resource before restoration"
      }
      Err(error) => test.fail(error.message)
      Ok(_) => test.fail("missing inner capture")
    }

    7
  }?
}

test test_heap_scoped_causes_and_callee_failures_cannot_escape { |ctx|
  let caused = test.run_script(
    ctx,
    r"""error ChildError = Owned(child: ProcessHandle)
error OuterError = Failed(message: Str)
proc scope_cause() [process, env, error] -> Result[Unit] {
  try {
    env ({XSH_SCOPE_CHILD: "inner"}) {
      let child = spawn run sh -c "sleep 10" ?
      Err(OuterError.Failed(message: "outer"), cause: ChildError.Owned(child: child))?
    }?
  }
}
scope_cause()?
""",
  )?
  assert ! caused.success, "heap scope must reject resource held only by cause"
  assert "cannot escape a restored context" in caused.stderr
  let callee = test.run_script(
    ctx,
    r"""error ChildError = Owned(child: ProcessHandle)
proc failing_child() [process, error] -> Unit {
  let child = spawn run sh -c "sleep 10" ?
  Err(ChildError.Owned(child: child))?
}
proc scoped_failure() [process, env, error] -> Result[Unit] {
  try { env ({XSH_SCOPE_CHILD: "inner"}) { failing_child() }? }
}
scoped_failure()?
""",
  )?
  assert ! callee.success, "checked runtime failure must not carry scoped child out"
  assert "cannot escape a restored context" in callee.stderr
}

test test_retry_exhaustion_retains_resource_in_original_failure {
  let exhausted = retry [0ms] {
    let child = spawn run sh -c "sleep 10" ?
    Err(ChildError.Owned(child:))?
  }
  match exhausted {
    Err(ChildError.Owned {child: child}) => {
      defer child.cancel(signal: "TERM", kill_after: 0ms)
      assert process.list()? |> any .pid == child.pid, "exhausted retry must retain the original failure's resource"
    }
    Err(error) => test.fail(error.message)
    Ok(_) => test.fail("retry unexpectedly succeeded")
  }
}

proc cleanup_error_child() [process, error] {
  let child = spawn run sh -c "sleep 10" ?
  Err(ChildError.Owned(child:))
}

proc cleanup_cause_child() [process, error] {
  let child = spawn run sh -c "sleep 10" ?
  Err(CauseOwnerError.Failed(pid: child.pid), cause: ChildError.Owned(child:))
}

test test_primary_defer_error_retains_child_after_exhausted_block {
  let captured = try {
    defer cleanup_error_child()
  }
  match captured {
    Err(ChildError.Owned {child: child}) => {
      defer child.cancel(signal: "TERM", kill_after: 0ms)
      assert process.list()? |> any .pid == child.pid, "primary defer failure must retain its child"
    }
    Err(error) => test.fail(error.message)
    Ok(_) => test.fail("missing defer failure")
  }
}

test test_primary_defer_cause_retains_child {
  let captured = try {
    {
      defer cleanup_cause_child()
    }
  }
  match captured {
    Err(CauseOwnerError.Failed {pid: pid}) => {
      defer process.kill(pid, signal: "TERM")
      assert process.list()? |> any .pid == pid, "primary defer cause must retain its child across nested scope exits"
    }
    Err(error) => test.fail(error.message)
    Ok(_) => test.fail("missing defer cause")
  }
}

proc cleanup_marked_error_child(marker: Path) [fs, process, error] {
  let child = spawn run sh -c "sleep 10" ?
  marker.write(f"{child.pid}")
  Err(ChildError.Owned(child:))
}

test test_secondary_defer_failure_releases_its_child { |ctx|
  let marker = test.temp_path(ctx, name: "secondary-child-pid")
  let captured: Result[Unit] = try {
    defer cleanup_marked_error_child(marker)
    Err(WrapperError.Failed("primary"))?
  }
  match captured {
    Err(WrapperError.Failed {message: message}) => assert message == "primary"
    Err(error) => test.fail(error.message)
    Ok(_) => test.fail("missing primary failure")
  }

  let pid = marker.read_text()? as Int
  assert ! (process.list()? |> any .pid == pid), "secondary cleanup resource must close locally"
}

test test_scoped_primary_defer_resources_are_rejected_before_restore { |ctx|
  let normal = test.run_script(
    ctx,
    r"""error ChildError = Owned(child: ProcessHandle)
error OuterError = Failed(message: Str)
proc cleanup_cause() [process, error] -> Result[Unit] {
  let child = spawn run sh -c "sleep 10" ?
  Err(OuterError.Failed(message: "cleanup"), cause: ChildError.Owned(child: child))
}
proc scoped_cleanup() [process, env, error] -> Result[Unit] {
  try { let ignored = env ({X: "inner"}) { defer cleanup_cause()?; 7 }? }
}
scoped_cleanup()?
""",
  )?
  assert ! normal.success, "primary scoped cleanup resources must be rejected"
  assert "cannot escape a restored context" in normal.stderr
  let returning = test.run_script(
    ctx,
    r"""error ChildError = Owned(child: ProcessHandle)
proc cleanup_child() [process, error] -> Result[Unit] {
  let child = spawn run sh -c "sleep 10" ?
  Err(ChildError.Owned(child: child))
}
proc returning_scope() [process, env, error] -> Int {
  env ({X: "inner"}) { defer cleanup_child()?; return 7 }?
}
let captured: Result[Int] = try { returning_scope() }
print "unreachable"
""",
  )?
  assert ! returning.success, "return cleanup resources must not cross restoration"
  assert "cannot escape a restored context" in returning.stderr
}

test test_inner_try_primary_defer_can_retain_resource_inside_context {
  let _ = env ({X: "inner"}) {
    let captured = try {
      defer cleanup_error_child()
    }
    match captured {
      Err(ChildError.Owned {child: child}) => {
        defer child.cancel(signal: "TERM", kill_after: 0ms)
        assert process.list()? |> any .pid == child.pid, "inner primary defer failure may retain its resource"
      }
      Err(error) => test.fail(error.message)
      Ok(_) => test.fail("missing inner defer failure")
    }

    7
  }?
}

test test_scoped_stream_cancel_failure_rejects_resource_before_restore { |ctx|
  let output = test.run_script(
    ctx,
    r"""error ChildError = Owned(child: ProcessHandle)
proc cleanup_child() [process, error] -> Result[Unit] {
  let child = spawn run sh -c "sleep 10" ?
  Err(ChildError.Owned(child: child))
}
stream rows() [process, error] -> Stream[Int] {
  defer cleanup_child()?
  yield 1
  yield 2
}
proc returning_scope() [process, env, error] -> Int {
  env ({X: "inner"}) {
    for row in rows() { return 7 }
    0
  }?
}
let captured: Result[Int] = try { returning_scope() }
print "unreachable"
""",
  )?
  assert ! output.success, "stream cancellation failure must not carry scoped child across restoration"
  assert "cannot escape a restored context" in output.stderr
}
