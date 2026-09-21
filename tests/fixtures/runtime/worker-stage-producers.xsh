# The declared `stream` contract through the worker stages.
#
# A producer handed to `par-map`, and to the fused
# `par-map | flat-map | reduce-by` path, is consumed with the same semantics as
# any other consumer: the body runs when the stage pulls it, the rows it yields
# are the rows the stage maps, and its `defer` runs exactly once — including
# when a row fails and the stage reports the failure the row declared.
#
# The stage's result is a `List`, so the stage does consume the whole producer;
# what this fixture pins is that the consumption goes through the producer
# machinery rather than around it. The program is driven by the Rust
# integration suite, which supplies a fresh directory in
# `XSH_WORKER_STAGE_DIR`, asks for the failing case with
# `XSH_WORKER_STAGE_EXPECT_FAILURE`, and removes the directory afterwards.

error RowError = Bad(detail: Str)

# The value of one row, or the failure a row that is not a number reports.
pure row_value(row: Str) -> Result[Int] {
  if row == "bad" {
    return Err(RowError.Bad(detail: "row is not a number"))
  }
  return Ok(row.byte_len())
}

# One marker per row, and one when the body ends.
stream counted(dir: Path, tag: Str) [fs, error] -> Stream[Int] {
  defer fp"${dir}/closed-${tag}".write("1")?
  for row in ["one", "two", "three"] {
    fp"${dir}/row-${tag}-${row}".write("1")?
    yield row.byte_len()
  }
}

# A producer whose second row fails, so the stage sees the failure the row
# declared rather than a row value.
stream checked(dir: Path, tag: Str) [fs, error] -> Stream[Int] {
  defer fp"${dir}/closed-${tag}".write("1")?
  for row in ["one", "bad", "three"] {
    fp"${dir}/row-${tag}-${row}".write("1")?
    yield row_value(row)?
  }
}

proc open_counted(dir: Path, tag: Str) [fs, error] -> Result[Stream[Int]] {
  return Ok(counted(dir, tag))
}

proc open_checked(dir: Path, tag: Str) [fs, error] -> Result[Stream[Int]] {
  return Ok(checked(dir, tag))
}

proc count_present(dir: Path, prefix: Str) [fs, error] -> Int {
  var found = 0
  for entry in fs.files(dir)? {
    if entry.name.starts_with(prefix) {
      found = found + 1
    }
  }
  return found
}

proc exists(target: Path) [fs] -> Bool {
  return target.exists() ?? false
}

proc main() [io, fs, env, error] {
  let root = Path(env.get("XSH_WORKER_STAGE_DIR")?)
  fs.mkdir(root)?

  # A worker stage over a producer: the mapped values are the producer's rows,
  # every row was produced exactly once, and the defer ran once.
  let mapped = open_counted(root, "map")? |> par-map --jobs=2 { |value| value * 2 } |> collect()
  print f"mapped=${mapped.len()} first=${mapped.get(0, -1)} rows=${count_present(root, "row-map-")} closed=${exists(fp"${root}/closed-map")}"

  # A bounded terminal after the stage: the stage's own result is a `List`, so
  # the producer is consumed there, and it is still stopped exactly once.
  let taken = open_counted(root, "take")? |> par-map --jobs=2 { |value| value * 2 } |> first()
  print f"taken=${taken ?? -1} rows=${count_present(root, "row-take-")} closed=${exists(fp"${root}/closed-take")}"

  # The fused worker path consumes the producer the same way.
  let fused = open_counted(root, "fuse")?
    |> par-map --jobs=2 { |value| [{bucket: "all", count: 1, total: value}] }
    |> flat-map { |rows| rows }
    |> reduce-by --sum { |row| {key: row.bucket, value: {count: row.count, total: row.total}} }
  print f"fused=${fused.get("all", {count: 0, total: 0}).total} rows=${count_present(root, "row-fuse-")} closed=${exists(fp"${root}/closed-fuse")}"

  # A producer that fails mid-stream fails the stage, and its defer still runs
  # exactly once; the run's exit status and the closed marker are what the Rust
  # test asserts for this case.
  let expect_failure = env.get_or("XSH_WORKER_STAGE_EXPECT_FAILURE", "")? == "1"
  if expect_failure {
    let accepted = open_checked(root, "check")? |> par-map --jobs=2 { |value| value * 2 } |> collect()
    print f"checked accepted=${accepted.len()}"
  }
}
