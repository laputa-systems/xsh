use tests.fixtures.runtime.resource_producers as resource_producers

const resource_response: NetResponse = {
  status: 200,
  reason: "OK",
  bytes: 2,
  headers: [],
  url: "https://example.test/resource",
  effective_url: "https://example.test/resource",
  redirect_count: 0,
  body: b"ok",
}

type ResourceRootHolder = {root: FsRoot?}

proc resource_lock_available(ctx: TestContext, lock_path: Path) [fs, process, error] -> Result[Bool] {
  let output = test.expect(ctx, "print (fs.lock(Path(args[0]), nonblocking: true) is Ok(_))", status: 0, args: [lock_path])?
  output.stdout.trim() == "true"
}

proc resource_return_root() [fs, error] -> Result[FsRoot] {
  fs.tempdir()?
}

proc resource_borrow_root(root: FsRoot) [fs, error] -> Result[Unit] {
  root.write(p"borrowed", "live")
}

test resource_lock_rejects_structural_and_json_forgery { |ctx|
  for source in [
    "let lock: FsLock = {id: 1, path: p\"nowhere\", shared: false}\n",
    "proc release(lock: FsLock) [fs] { fs.unlock(lock) }\nrelease({id: 1, path: p\"nowhere\", shared: false})\n",
  ] {
    let output = test.run_script(ctx, source)?
    assert output.status == 2, output.stderr
    assert "check.type-mismatch" in output.stderr, output.stderr
  }

  let decoded: Any = json.decode("{\"id\":1,\"path\":\"nowhere\",\"shared\":false}")?
  assert decoded.require(FsLock) is Err(_)
}

test resource_lock_keeps_metadata_and_hides_identity { |ctx|
  let dir = test.temp_dir(ctx, name: "resource-metadata")?
  let lock_path = fp"{dir}/lock"
  let lock = fs.lock(lock_path, shared: true)?
  defer fs.unlock(lock)
  assert lock.path == lock_path
  assert lock.shared
  let erased: Any = lock
  assert json.encode(erased) is Err(_)

  for statement in [
    "let _ = lock.id",
    "let _ = json.encode(lock)",
    "var alias = lock\nalias.path = p\"forged\"",
    "var alias = lock\nalias.shared = false",
  ] {
    let output = test.run_script(ctx, f"proc inspect(lock: FsLock) {{ {statement} }}\n")?
    assert output.status == 2, output.stderr
  }
}

test resource_temp_root_closes_after_its_defers {
  var usable_in_defer = false
  let root_path = {
    let root = fs.tempdir()?
    defer { usable_in_defer = root.exists(p".") ?? false }
    root.host_path()?
  }
  assert usable_in_defer
  assert ! root_path.exists()?, "the creating scope must remove its temporary directory"
}

test resource_lock_unlocks_after_its_defers { |ctx|
  let dir = test.temp_dir(ctx, name: "resource-lock-exit")?
  let lock_path = fp"{dir}/lock"
  var held_in_defer = false
  {
    let lock = fs.lock(lock_path)?
    defer { held_in_defer = ! resource_lock_available(ctx, lock_path)? }
    assert ! resource_lock_available(ctx, lock_path)?
  }
  assert held_in_defer
  assert resource_lock_available(ctx, lock_path)?, "the creating scope must release its lock"
}

test resource_middle_block_process_assignment_survives {
  var kept: ProcessHandle? = null
  {
    let child = spawn run sleep 30 ?
    { kept = child }
  }
  let child = kept.require(ProcessHandle)?
  defer child.cancel(kill_after: 0ms)
  assert process.list()? |> any .pid == child.pid, "an outward assignment retains a middle-block child"
}

test resource_middle_block_job_assignment_survives { |ctx|
  test.mock(ctx, "net.start", {url: resource_response.url}, Ok(resource_response))
  var kept: NetJob? = null
  {
    let job = net.start({method: "GET", url: resource_response.url})?
    { kept = job }
  }
  let job = kept.require(NetJob)?
  assert job.wait()?.body == b"ok"
}

test resource_stats_container_retains_its_child {
  let summary = {
    let child = spawn run sleep 30 ?
    {blanks: 0, blobs: {"child": child}, code: 0, comments: 0}
  }
  let child = summary.blobs.get("child")?
  defer child.cancel(kill_after: 0ms)
  assert process.list()? |> any .pid == child.pid, "a specialized record retains handles inside its map"
}

test resource_yield_transfers_children_and_roots {
  let children = resource_producers.children() |> collect()
  for child in children {
    defer child.cancel(kill_after: 0ms)
    assert process.list()? |> any .pid == child.pid, "a completed producer must retain its emitted child"
  }
  let roots = resource_producers.roots() |> take(1)
  assert roots.len() == 1
  for root in roots {
    defer root.close()
    assert root.exists(p".")?, "stopping a producer must retain its emitted root"
  }
}

test resource_root_return_block_break_and_borrow_survive {
  let returned = resource_return_root()?
  defer returned.close()
  resource_borrow_root(returned)
  assert returned.read_text(p"borrowed")? == "live"

  let block = { fs.tempdir()? }
  defer block.close()
  assert block.exists(p".")?

  let broken: FsRoot = loop {
    let root = fs.tempdir()?
    break root
  }
  defer broken.close()
  assert broken.exists(p".")?

  var holder: ResourceRootHolder = {root: null}
  {
    let root = fs.tempdir()?
    { holder.root = root }
  }
  let assigned = holder.root.require(FsRoot)?
  defer assigned.close()
  assert assigned.exists(p".")?
}

test resource_worker_rejects_foreign_process_before_selecting_its_child {
  let parent = spawn run sleep 30 ?
  defer parent.cancel(kill_after: 0ms)
  let results = [1, 2] |> par-map(jobs: 2) { |_|
    let local = spawn run true ?
    defer local.cancel(kill_after: 0ms)
    let rejected_wait = wait parent is Err(_)
    let rejected_cancel = parent.cancel(kill_after: 0ms) is Err(_)
    let rejected_list = wait [parent] is Err(_)
    let rejected_any = process.wait_any([parent]) is Err(_)
    let rejected_ready = process.wait_ready([parent]) is Err(_)
    let rejected_timeout = process.wait_timeout([parent], 0ms) is Err(_)
    let local_status = wait local?
    rejected_wait and rejected_cancel and rejected_list and rejected_any and rejected_ready and rejected_timeout and local_status.ok
  }
  assert results == [true, true]
  assert process.list()? |> any .pid == parent.pid
}

test resource_worker_rejects_foreign_job_before_selecting_its_job { |ctx|
  test.mock(ctx, "net.start", {url: resource_response.url}, Ok(resource_response))
  let parent = net.start({method: "GET", url: resource_response.url})?
  let results = [1, 2] |> par-map(jobs: 2) { |_|
    test.mock(ctx, "net.start", {url: resource_response.url}, Ok(resource_response))
    let local = net.start({method: "GET", url: resource_response.url})?
    let rejected_wait = parent.wait() is Err(_)
    let rejected_cancel = parent.cancel() is Err(_)
    let response = local.wait()?
    rejected_wait and rejected_cancel and response.body == b"ok"
  }
  assert results == [true, true]
  assert parent.wait()?.body == b"ok"
}

test resource_worker_rejects_foreign_capabilities_before_table_access { |ctx|
  let dir = test.temp_dir(ctx, name: "resource-worker-capability")?
  let parent_lock = fs.lock(fp"{dir}/parent")?
  defer fs.unlock(parent_lock)
  let parent_root = fs.tempdir()?
  defer parent_root.close()
  let results = [1, 2] |> par-map(jobs: 2) { |item|
    let local_lock = fs.lock(fp"{dir}/worker-{item}")?
    defer fs.unlock(local_lock)
    let local_root = fs.tempdir()?
    defer local_root.close()
    let rejected_unlock = fs.unlock(parent_lock) is Err(_)
    let rejected_close = parent_root.close() is Err(_)
    let still_held = ! resource_lock_available(ctx, local_lock.path)?
    rejected_unlock and rejected_close and still_held and local_root.exists(p".")?
  }
  assert results == [true, true]
  assert ! resource_lock_available(ctx, parent_lock.path)?
  assert parent_root.exists(p".")?
}

test resource_worker_rejects_foreign_stream_and_closes_worker_root {
  let parent = resource_producers.numbers()
  let results = [1, 2] |> par-map(jobs: 2) { |_|
    let root = fs.tempdir()?
    let local = resource_producers.numbers()
    let rejected = try { parent |> collect() }
    assert rejected is Err(_)
    assert (local |> collect()) == [7]
    root
  }
  for root in results {
    assert root.exists(p".") is Err(_), "a returned worker handle is not live in the caller"
  }
  assert (parent |> collect()) == [7]
}

test resource_use_after_release_is_checked_for_exact_locals { |ctx|
  for source in [
    "let root = fs.tempdir()?\nroot.close()\n{ let _ = root.exists(p\".\") }\n",
    "let lock = fs.lock(p\"resource-affine.lock\")?\nfs.unlock(lock)\nfs.unlock(lock)\n",
    "let child = spawn run true ?\nlet _ = wait child?\nlet _ = wait child\n",
    "let job = net.start({method: \"GET\", url: \"https://example.test/resource\"})?\njob.cancel()\nlet _ = job.wait()\n",
  ] {
    let output = test.run_script(ctx, source)?
    assert output.status == 2, output.stderr
    assert "check.use-after-release" in output.stderr, output.stderr
  }
}

test resource_untracked_release_reassignment_and_cancel_exception { |ctx|
  let output = test.expect(ctx, r"""
proc close_borrowed(root: FsRoot) [fs] { root.close() }
proc root_released(root: FsRoot) [fs] -> Bool { root.exists(p".") is Err(_) }
let borrowed = fs.tempdir()?
close_borrowed(borrowed)
assert root_released(borrowed)
var replaced = fs.tempdir()?
replaced.close()
replaced = fs.tempdir()?
assert replaced.exists(p".")?
let conditional = fs.tempdir()?
if true { conditional.close() }
assert root_released(conditional)
let child = spawn run true ?
let _ = wait child?
assert child.cancel(kill_after: 0ms) == Ok()
print "retained"
""", status: 0)?
  assert output.stdout == "retained\n", output.stderr
}
