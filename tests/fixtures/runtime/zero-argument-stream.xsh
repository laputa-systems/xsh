# A zero-argument `stream` producer, reached from every position that resolves
# a call.
#
# The explicit-frame engine resolved a call with no arguments without asking
# whether the callee is a producer: it pushed an ordinary frame, the body's
# `yield`s had nowhere to report, and the call failed as "lowered function did
# not return". The same producer with one parameter worked, which is why the
# defect survived — every producer in the tree took one.
#
# The program lives under `tests/fixtures/` because it is executed by the Rust
# integration suite, and it is a plain script: no internal names, no switches.

stream items() [] -> Stream[Int] {
  yield 1
  yield 2
}

# A producer reached from inside another producer's body.
stream doubled() [] -> Stream[Int] {
  for item in items() {
    yield item * 2
  }
}

# A producer reached from inside a loop in an ordinary proc.
proc total() [error] -> Int {
  var sum = 0
  for item in items() {
    sum = sum + item
  }
  return sum
}

# A producer whose `defer` must still run when a bounded terminal stops early.
stream guarded(log: Path) [fs, error] -> Stream[Int] {
  defer log.write("closed")?
  for item in items() {
    yield item
  }
}

proc main() [io, env, error] {
  # Direct `for`, a binding, a pipeline stage, a bounded terminal, and a call
  # from inside another producer.
  var direct = 0
  for item in items() {
    direct = direct + item
  }
  print f"direct=${direct}"

  let bound = items()
  var collected = bound.collect()
  print f"bound=${collected.len()}"

  print f"total=${total()}"
  print f"doubled=${doubled().collect().len()}"

  var mapped = items() |> map { |item| item + 1 } |> collect()
  print f"mapped=${mapped.len()}"

  var taken = items() |> take(1) |> collect()
  print f"taken=${taken.len()}"

  let first_taken = items() |> first()
  print f"first=${first_taken ?? -1}"

  # The log path comes from the environment so the fixture needs no argument
  # position: `main` must use the spread form to receive script arguments, and
  # the producer it drives is what this fixture is about.
  let log = Path(env.get("XSH_ZERO_ARGUMENT_STREAM_LOG")?)
  let stopped = guarded(log) |> first()
  print f"stopped=${stopped ?? -1}"
  print log.read_text()?
}
