var failure_tail_events: List[Str] = []

proc observed_failure(lock: FsLock) [fs, error] -> Result[Unit] {
  with held = lock {
    defer { failure_tail_events += ["defer"] }
    failure_tail_events += ["body"]
    fail "observed body stays primary"
  }
}

proc verify_resource_failure_tail() [fs, error] -> Result[Unit] {
  tempdir directory {
    let first = fs.lock(fp"{directory}/first")?
    match primary_failure(first) {
      Err(problem) => assert problem.message == "body stays primary"
      else => assert false
    }
    assert fs.unlock(first) is Err(_)
    let observed = fs.lock(fp"{directory}/observed")?
    failure_tail_events += ["before"]
    match observed_failure(observed) {
      Err(problem) => assert problem.message == "observed body stays primary"
      else => assert false
    }
    failure_tail_events += ["after"]
    assert fs.unlock(observed) is Err(_)
    assert failure_tail_events == ["before", "body", "defer", "after"]
  }?
}
