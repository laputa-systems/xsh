var resource_events: List[Str] = []

proc return_root(root: FsRoot) [fs] -> Int {
  let ignored = with held = root {
    defer { resource_events += ["root defer"] }
    resource_events += ["root body"]
    return 7
  }
}

proc return_lock(lock: FsLock) [fs] -> Int {
  let ignored = with held = lock {
    defer { resource_events += ["lock defer"] }
    resource_events += ["lock body"]
    return 11
  }
}

proc break_lock(lock: FsLock, early: Bool) [fs] -> Int {
  loop {
    let ignored = with held = lock {
      defer { resource_events += ["break defer"] }
      if early { break }
      0
    }
    break
  }
  return 13
}

proc continue_lock(lock: FsLock, early: Bool) [fs] -> Int {
  var visits = 0
  for item in [1, 2] {
    let ignored = with held = lock {
      defer { resource_events += ["continue defer"] }
      visits += item
      if early { continue }
      0
    }
  }
  return visits
}

proc ordinary_defer_failure() [error] -> Int {
  defer { error.fail("ordinary defer stays primary")? }
  return 17
}

proc foreign_lock(lock_path: Path) [fs, error] -> Result[FsLock] {
  let locks = [lock_path, fp"{lock_path}.other"] |> par-map(jobs: 2) { |worker| fs.lock(worker)? }
  locks[0]
}

proc normal_release(lock: FsLock) [fs] -> Result[Int] {
  with held = lock { 3 }
}

proc primary_failure(lock: FsLock) [fs, error] -> Result[Unit] {
  with held = lock { fail "body stays primary" }
  return Ok()
}

proc verify_resource_scope_return() [fs, error] -> Result[Unit] {
  tempdir directory {
    let root = fs.open_root(directory)?
    assert return_root(root) == 7
    assert root.exists(p".") is Err(_)
    let local = fs.lock(fp"{directory}/local")?
    assert return_lock(local) == 11
    assert fs.unlock(local) is Err(_)
    let foreign = foreign_lock(fp"{directory}/lock")?
    assert return_lock(foreign) == 11
    assert break_lock(foreign, true) == 13
    assert continue_lock(foreign, true) == 3
    match try { ordinary_defer_failure() } {
      Err(problem) => assert problem.message == "ordinary defer stays primary"
      else => assert false
    }
    match normal_release(foreign) {
      Err(problem) => assert problem.message == "lock handle is not active"
      else => assert false
    }
    match primary_failure(foreign) {
      Err(problem) => assert problem.message == "body stays primary"
      else => assert false
    }
    assert resource_events == ["root body", "root defer", "lock body", "lock defer", "lock body", "lock defer", "break defer", "continue defer", "continue defer"]
  }?
}
