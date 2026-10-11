##! Lazy producers that emit live handles or a scalar row for ownership tests.

## Emit two children whose lifetimes belong to their consumer after each yield.
export stream children() [process, error] -> Stream[ProcessHandle] {
  yield spawn run sleep 30 ?
  yield spawn run sleep 30 ?
}

## Emit temporary roots so a bounded consumer can retain one after cancellation.
export stream roots() [fs, error] -> Stream[FsRoot] {
  yield fs.tempdir()?
  yield fs.tempdir()?
}

## Emit one scalar row through a live producer.
export stream numbers() [] -> Stream[Int] {
  yield 7
}

## Delegate children so each item crosses both suspended producer scopes.
export stream delegated_children() [process, error] -> Stream[ProcessHandle] {
  yield @children()
}

## Delegate a materialized handle list without moving its unconsumed rows.
export stream listed_children() [process, error] -> Stream[ProcessHandle] {
  yield @[spawn run sleep 30 ?, spawn run sleep 30 ?]
}

## Delegate roots through another suspended producer before bounded cancellation.
export stream delegated_roots() [fs, error] -> Stream[FsRoot] {
  yield @roots()
}

## Emit locks whose scopes may end while a consumer still holds them.
export stream locks(lock_paths: List[Path]) [fs, error] -> Stream[FsLock] {
  for lock_path in lock_paths { yield fs.lock(lock_path)? }
}

## Emit mocked asynchronous jobs for consumer-side completion.
export stream jobs(url: Str) [net, error] -> Stream[NetJob] {
  yield net.start({method: "GET", url: url})?
  yield net.start({method: "GET", url: url})?
}

## Error payloads retain the activities handed to their consumer.
export error ChildError = Owned(child: ProcessHandle)

# Fail with an activity held by a typed payload instead of an emitted row.
stream error_children() [process, error] -> Stream[Int] {
  let child = spawn run sleep 30 ?
  Err(ChildError.Owned(child: child))?
}

## Propagate a child's resource-bearing failure through a suspended parent.
export stream delegated_errors() [process, error] -> Stream[Int] {
  yield @error_children()
}
