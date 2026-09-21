# The declared `stream` contract, exercised at the boundaries that decide it:
# a producer call does not run the body, a pull runs the body up to the next
# `yield`, a consumer that stops early does not reach later rows, and the
# producer's `defer` runs exactly once on every way a producer can end.
#
# The program is driven by the Rust integration suite, which supplies a fresh
# directory in `XSH_LAZY_STREAM_DIR` and removes it afterwards; the markers each
# step leaves behind are what the assertions read.

error RowError = Malformed(detail: Str)

# The value of one row, or the failure a row that is not a number reports.
pure row_value(row: Str) -> Result[Int] {
  if row == "bad" {
    return Err(RowError.Malformed(detail: "row is not a number"))
  }
  return Ok(row.byte_len())
}

# One marker per row, and one when the body ends.
stream counted(text: Str, dir: Path, tag: Str) [fs, error] -> Stream[Int] {
  defer fp"${dir}/closed-${tag}".write("1")?
  for row in text.lines() {
    fp"${dir}/row-${tag}-${row}".write("1")?
    yield row.byte_len()
  }
}

# A wrapper in the shape of the ported Linux entries: the text is read when the
# call is made, and the rows are interpreted when the stream is consumed.
proc open_rows(dir: Path, tag: Str) [fs, error] -> Result[Stream[Int]] {
  let text = fs.read_text(fp"${dir}/input.txt")?
  return Ok(counted(text, dir, tag))
}

# A producer whose row fails: `?` in a `yield` position propagates the failure
# to the consumer instead of yielding an `Err` element.
stream checked(text: Str, dir: Path, tag: Str) [fs, error] -> Stream[Int] {
  defer fp"${dir}/closed-${tag}".write("1")?
  for row in text.lines() {
    fp"${dir}/row-${tag}-${row}".write("1")?
    yield row_value(row)?
  }
}

proc open_checked(dir: Path, tag: Str) [fs, error] -> Result[Stream[Int]] {
  let text = fs.read_text(fp"${dir}/input.txt")?
  return Ok(checked(text, dir, tag))
}

# Leaves a producer mid-body by returning out of the loop that was consuming
# it; the producer is stopped where it stopped, not drained.
proc stop_by_return(dir: Path, tag: Str) [fs, error] -> Result[Int] {
  for row in open_rows(dir, tag)? {
    return Ok(row)
  }
  return Ok(-1)
}

proc exists(target: Path) [fs] -> Bool {
  return target.exists() ?? false
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

# Consumes a producer that fails on its second row, so the failure is reported
# where the consumer pulls it rather than inside `main`.
proc consume_checked(dir: Path, tag: Str) [fs, error] -> Result[Int] {
  let rows = open_checked(dir, tag)?
  let items = rows.collect()
  return Ok(items.len())
}

proc main() [io, fs, env, error] {
  let root = Path(env.get("XSH_LAZY_STREAM_DIR")?)
  fs.mkdir(root)?

  # 1. The call reads the text but does not run the producer body.
  fs.write(fp"${root}/input.txt", "one\ntwo\nthree")?
  var numbers = open_rows(root, "call")?
  print f"body started at call=${exists(fp"${root}/row-call-one")}"
  # The rows are interpreted from the text the call retained, so removing the
  # file cannot change the stream.
  fs.remove(fp"${root}/input.txt")?
  let collected = numbers.collect()
  print f"rows=${collected.len()} values=${collected.get(0, -1)}"

  # 2. An early stop does not reach later rows, and an unconsumed producer is
  #    still stopped once the program can no longer reach it.
  fs.write(fp"${root}/input.txt", "one\ntwo\nthree")?
  let first = open_rows(root, "stop")? |> first()
  print f"first=${first ?? -1} rows=${count_present(root, "row-stop-")} closed=${exists(fp"${root}/closed-stop")}"
  # A producer whose body never started has nothing to clean up: dropping it
  # without a pull must not run the defers its body would have registered.
  let dropped = open_rows(root, "dropped")?
  let _ = dropped
  let other = open_rows(root, "other")?
  print f"never started closed=${exists(fp"${root}/closed-dropped")} other=${other.collect().len()}"

  # A producer abandoned from inside a loop runs its defers where it was left.
  let stopped = stop_by_return(root, "return")?
  print f"return stopped=${stopped} closed=${exists(fp"${root}/closed-return")}"

  # 3. `take` keeps its items and stops there.
  numbers = open_rows(root, "take")?
  let taken = numbers |> take(2) |> collect()
  print f"taken=${taken.len()} rows=${count_present(root, "row-take-")} closed=${exists(fp"${root}/closed-take")}"

  # 4. A `for` that breaks stops the producer.
  for item in open_rows(root, "break")? {
    print f"break item=${item}"
    break
  }
  print f"break rows=${count_present(root, "row-break-")} closed=${exists(fp"${root}/closed-break")}"

  # 5. A malformed later row is not reached by an early stop, and consuming
  #    past it reports the failure the row declared.
  fs.write(fp"${root}/input.txt", "one\nbad\nthree")?
  let head = open_checked(root, "checked")? |> first()
  print f"checked first=${head ?? -1} rows=${count_present(root, "row-checked-")}"
  let expect_failure = env.get_or("XSH_LAZY_STREAM_EXPECT_FAILURE", "")? == "1"
  if expect_failure {
    # Consuming past the malformed row aborts the consumer with the failure the
    # row declared; the run's exit status and the closed marker are what the
    # Rust test asserts.
    fs.write(fp"${root}/input.txt", "one\nbad\nthree")?
    let failing = open_checked(root, "failing")?
    let all = failing.collect()
    print f"malformed row accepted=${all.len()}"
  }

  # 6. Unreadable text fails the call, before any row is interpreted.
  fs.remove(fp"${root}/input.txt")?
  match open_rows(root, "missing") {
    Ok(_) => { print "missing input accepted" }
    Err(_) => { print "missing input rejected" }
  }
}
