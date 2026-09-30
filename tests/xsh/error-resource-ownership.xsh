error ChildError = Owned(child: ProcessHandle)
error WrapperError = Failed(message: Str)

proc return_error_child() [process, error] -> Result[Unit] {
  let child = spawn run sh -c "sleep 10" ?
  Err(ChildError.Owned(child: child))
}

proc return_cause_child() [process, error] -> Result[Record] {
  let child = spawn run sh -c "sleep 10" ?
  let attached: Result[Unit, WrapperError] = Err(WrapperError.Failed(message: "outer"), cause: ChildError.Owned(child: child))
  {attached, pid: child.pid}
}

test test_error_payload_transfers_child_to_caller [process, error] {
  match return_error_child() {
    Err(ChildError.Owned {child}) => {
      defer child.cancel(signal: "TERM", kill_after: 0ms)
      test.ok(process.list()? |> any .pid == child.pid, "returned error payload must retain its owned child")?
    }
    Ok(_) => test.fail("expected child error")?
  }
}

test test_error_cause_transfers_child_to_caller [process, error] {
  let bundle = return_cause_child()?
  defer process.kill(bundle.pid, signal: "TERM")
  test.ok(process.list()? |> any .pid == bundle.pid, "child held only by a typed cause must survive callee cleanup")?
}

error CauseOwnerError = Failed(pid: Int)

test test_try_error_payload_transfers_child_before_nested_cleanup [process, error] {
  let captured: Result[Unit] = try {
    let child = spawn run sh -c "sleep 10" ?
    if true {
      Err(ChildError.Owned(child: child))?
    }
  }
  match captured {
    Err(ChildError.Owned {child}) => {
      defer child.cancel(signal: "TERM", kill_after: 0ms)
      test.ok(process.list()? |> any .pid == child.pid, "capture must retain a child owned by an outer discarded block")?
    }
    Err(error) => test.fail(error.message)?
    Ok(_) => test.fail("expected captured child error")?
  }
}

test test_try_error_cause_transfers_child_before_cleanup [process, error] {
  let captured: Result[Unit] = try {
    let child = spawn run sh -c "sleep 10" ?
    Err(CauseOwnerError.Failed(pid: child.pid), cause: ChildError.Owned(child: child))?
  }
  match captured {
    Err(CauseOwnerError.Failed {pid}) => {
      defer process.kill(pid, signal: "TERM")
      test.ok(process.list()? |> any .pid == pid, "capture must retain a child held only by a cause")?
    }
    Err(error) => test.fail(error.message)?
    Ok(_) => test.fail("expected captured cause")?
  }
}

test test_driver_try_error_payload_and_cause_transfer [error] { |ctx|
  let output = test.run_script(ctx, r"""error ChildError = Owned(child: ProcessHandle)
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
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "live\n")?
}

proc propagate_child_from_plain_callee() [process, error] -> Unit {
  let child = spawn run sh -c "sleep 10" ?
  if true { Err(ChildError.Owned(child: child))? }
}

test test_try_plain_callee_failure_transfers_child [process, error] {
  let captured: Result[Unit] = try { propagate_child_from_plain_callee() }
  match captured {
    Err(ChildError.Owned {child}) => {
      defer child.cancel(signal: "TERM", kill_after: 0ms)
      test.ok(process.list()? |> any .pid == child.pid, "checked runtime transport must retain the callee child")?
    }
    Err(error) => test.fail(error.message)?
    Ok(_) => test.fail("missing callee failure")?
  }
}

test test_scoped_propagated_resource_cannot_reach_outer_try [error] { |ctx|
  let output = test.run_script(ctx, r"""error ChildError = Owned(child: ProcessHandle)
let captured: Result[Unit] = try {
  env ({XSH_SCOPE_CHILD: "inner"}) {
    let child = spawn run sh -c "sleep 10" ?
    Err(ChildError.Owned(child: child))?
  }?
}
print "unreachable"
""")?
  test.ok(!output.success, "scoped resource propagation must be rejected")?
  test.contains(output.stderr, "cannot escape a restored context")?
}

test test_inner_try_retains_resource_inside_same_context [process, env, error] {
  let ignored = env ({XSH_SCOPE_CHILD: "inner"}) {
    let captured: Result[Unit] = try {
      let child = spawn run sh -c "sleep 10" ?
      Err(ChildError.Owned(child: child))?
    }
    match captured {
      Err(ChildError.Owned {child}) => {
        defer child.cancel(signal: "TERM", kill_after: 0ms)
        test.ok(process.list()? |> any .pid == child.pid, "inner capture may consume resource before restoration")?
      }
      Err(error) => test.fail(error.message)?
      Ok(_) => test.fail("missing inner capture")?
    }
    7
  }?
}

test test_heap_scoped_causes_and_callee_failures_cannot_escape [error] { |ctx|
  let caused = test.run_script(ctx, r"""error ChildError = Owned(child: ProcessHandle)
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
""")?
  test.ok(!caused.success, "heap scope must reject resource held only by cause")?
  test.contains(caused.stderr, "cannot escape a restored context")?
  let callee = test.run_script(ctx, r"""error ChildError = Owned(child: ProcessHandle)
proc failing_child() [process, error] -> Unit {
  let child = spawn run sh -c "sleep 10" ?
  Err(ChildError.Owned(child: child))?
}
proc scoped_failure() [process, env, error] -> Result[Unit] {
  try { env ({XSH_SCOPE_CHILD: "inner"}) { failing_child() }? }
}
scoped_failure()?
""")?
  test.ok(!callee.success, "checked runtime failure must not carry scoped child out")?
  test.contains(callee.stderr, "cannot escape a restored context")?
}

test test_retry_exhaustion_retains_resource_in_original_failure [process, time, error] {
  let exhausted: Result[Unit] = retry[0ms] {
    let child = spawn run sh -c "sleep 10" ?
    Err(ChildError.Owned(child: child))?
  }
  match exhausted {
    Err(ChildError.Owned {child}) => {
      defer child.cancel(signal: "TERM", kill_after: 0ms)
      test.ok(process.list()? |> any .pid == child.pid, "exhausted retry must retain the original failure's resource")?
    }
    Err(error) => test.fail(error.message)?
    Ok(_) => test.fail("retry unexpectedly succeeded")?
  }
}
