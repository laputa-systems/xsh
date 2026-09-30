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
