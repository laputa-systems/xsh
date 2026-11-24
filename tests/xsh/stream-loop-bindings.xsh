# A `for` loop over a stream at a script's top level keeps the script's
# bindings current for every reader: the loop body, a script producer pulled
# inside the loop, a function called from the body, and code after the loop,
# including after `break` and `continue`.

test test_top_level_stream_loops_keep_bindings_current { |ctx|
  let output = test.run_script(ctx, r"""
stream doubled(limit: Int) -> Stream[Int] {
  for i in range(limit) {
    yield i * 2
  }
}

pure plus(left: Int, right: Int) -> Int { left + right }

var total = 0
var trail: List[Int] = []
for i in range(5) {
  total += i
  trail = trail + [total]
}
print f"range total={total} trail={trail.len()} last={trail[4]}"

var outer = 10
for _ in range(3) {
  outer += 1
  for value in doubled(2) {
    outer = plus(outer, value)
  }
}
print f"nested outer={outer}"

var kept = 0
for i in range(10) {
  continue when i % 2 == 0
  break when i > 6
  kept += i
}
print f"control kept={kept}"

var length = 0
for line in "a\nbb\nccc".lines() {
  length += line.byte_len()
}
print f"lines length={length}"
""")?
  assert output.status == 0, f"{output.stdout}{output.stderr}"
  assert output.stdout == [
    "range total=10 trail=5 last=10",
    "nested outer=19",
    "control kept=9",
    "lines length=6",
    "",
  ].join("\n"), output.stdout
}
