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
