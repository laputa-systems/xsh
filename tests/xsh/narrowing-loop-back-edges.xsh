let narrowed_then_overwritten = [
  {
    name: "for",
    call: "label(1)",
    diagnostic: "err[check.operator-type]: compound assignment requires Int operands",
    source: r"""
pure label(start: Int?) -> Int {
  var cur = start
  var total = 0
  if cur != null {
    for i in range(2) { total += cur; cur = null }
  }
  total
}
""",
  },
  {
    name: "while",
    call: "label(1)",
    diagnostic: "err[check.operator-type]: compound assignment requires Int operands",
    source: r"""
pure label(start: Int?) -> Int {
  var cur = start
  var total = 0
  var n = 0
  if cur != null {
    while n < 2 { total += cur; cur = null; n += 1 }
  }
  total
}
""",
  },
  {
    name: "loop",
    call: "label(1)",
    diagnostic: "err[check.operator-type]: compound assignment requires Int operands",
    source: r"""
pure label(start: Int?) -> Int {
  var cur = start
  var total = 0
  if cur != null {
    loop {
      total += cur
      cur = null
      if total > 0 { break }
    }
  }
  total
}
""",
  },
  {
    name: "while condition",
    call: "label(1)",
    diagnostic: "err[check.operator-type]: comparison requires Int, Float, Str, or Duration",
    source: r"""
pure label(start: Int?) -> Int {
  var cur = start
  var total = 0
  if cur != null {
    while cur > 0 { total += 1; cur = null }
  }
  total
}
""",
  },
  {
    name: "nested loop with the write in the outer body",
    call: "label(1)",
    diagnostic: "err[check.operator-type]: compound assignment requires Int operands",
    source: r"""
pure label(start: Int?) -> Int {
  var cur = start
  var total = 0
  if cur != null {
    for i in range(2) {
      for j in range(2) { total += cur }
      cur = null
    }
  }
  total
}
""",
  },
  {
    name: "nested loop with the guard in the outer body",
    call: "label(1)",
    diagnostic: "err[check.operator-type]: compound assignment requires Int operands",
    source: r"""
pure label(start: Int?) -> Int {
  var cur = start
  var total = 0
  for i in range(2) {
    if cur != null {
      for j in range(2) { total += cur; cur = null }
    }
  }
  total
}
""",
  },
  {
    name: "continue after the write",
    call: "label(1)",
    diagnostic: "err[check.operator-type]: compound assignment requires Int operands",
    source: r"""
pure label(start: Int?) -> Int {
  var cur = start
  var total = 0
  if cur != null {
    for i in range(3) {
      total += cur
      if i == 0 { cur = null; continue }
    }
  }
  total
}
""",
  },
  {
    name: "break after the write, read after the loop",
    call: "label(1)",
    diagnostic: "err[check.operator-type]: compound assignment requires Int operands",
    source: r"""
pure label(start: Int?) -> Int {
  var cur = start
  var total = 0
  if cur != null {
    for i in range(3) {
      if i == 1 { cur = null; break }
    }
    total += cur
  }
  total
}
""",
  },
  {
    name: "field write",
    call: "label(1, null)",
    diagnostic: "err[check.operator-type]: compound assignment requires Int operands",
    source: r"""
type Entry = {file: Int?, other: Int}
pure label(found: Int, none: Int?) -> Int {
  var entry: Entry = {file: found, other: 0}
  var total = 0
  if entry.file != null {
    for i in range(2) { total += entry.file; entry.file = none }
  }
  total
}
""",
  },
  {
    name: "whole record write under a field guard",
    call: "label(1, null)",
    diagnostic: "err[check.operator-type]: compound assignment requires Int operands",
    source: r"""
type Entry = {file: Int?}
pure label(found: Int, none: Int?) -> Int {
  var entry: Entry = {file: found}
  var total = 0
  if entry.file != null {
    for i in range(2) { total += entry.file; entry = {file: none} }
  }
  total
}
""",
  },
  {
    name: "if let beside a guarded field read",
    call: "label(1, null)",
    diagnostic: "err[check.operator-type]: compound assignment requires Int operands",
    source: r"""
type Entry = {file: Int?}
pure label(found: Int, none: Int?) -> Int {
  var entry: Entry = {file: found}
  var total = 0
  if let file = entry.file {
    for i in range(2) { total += file; total += entry.file; entry = {file: none} }
  }
  total
}
""",
  },
  {
    name: "guard let beside a guarded field read",
    call: "label(1, null)",
    diagnostic: "err[check.operator-type]: compound assignment requires Int operands",
    source: r"""
type Entry = {file: Int?}
pure label(found: Int, none: Int?) -> Int {
  var entry: Entry = {file: found}
  var total = 0
  guard let file = entry.file else { return 0 }
  for i in range(2) { total += entry.file; entry = {file: none} }
  total + file
}
""",
  },
  {
    name: "proc call that clears a global",
    call: "label()",
    diagnostic: "err[check.operator-type]: compound assignment requires Int operands",
    source: r"""
var shared: Int? = 1
proc clear() [] { shared = null }
proc label() [error] -> Int {
  var total = 0
  if shared != null {
    for i in range(2) { total += shared; clear() }
  }
  total
}
""",
  },
  {
    name: "while condition that calls a proc clearing a global",
    call: "label()",
    diagnostic: "err[check.operator-type]: comparison requires Int, Float, Str, or Duration",
    source: r"""
var shared: Int? = 1
proc tick() [] -> Bool { shared = null; true }
proc label() [error] -> Int {
  var total = 0
  if shared != null {
    while shared > 0 and tick() { total += 1 }
  }
  total
}
""",
  },
]

test test_a_loop_ends_a_narrowing_that_its_body_overwrites { |ctx|
  for {name, call, diagnostic, source} in narrowed_then_overwritten {
    let output = test.run_xsh(ctx, source + f"\nprint ({call})\n")?
    assert output.status == 2, name
    assert diagnostic in output.stderr, f"{name}: {output.stderr}"
    assert !("runtime." in output.stderr), f"{name}: {output.stderr}"
    assert !("compact." in output.stderr), f"{name}: {output.stderr}"
  }
}

test test_a_loop_ends_a_generic_field_guard_that_its_body_overwrites { |ctx|
  let inferred = test.run_xsh(ctx, r"""
pure label(opts, none: Int?) -> Int {
  var entry = {file: opts.mnt}
  var total = 0
  if entry.file != null {
    for i in range(2) { total += entry.file; entry = {file: none} }
  }
  total
}
print (label({mnt: 1}, null))
""")?
  let unguarded = test.run_xsh(ctx, r"""
pure label(opts, none: Int?) -> Int {
  var entry = {file: opts.mnt}
  var total = 0
  for i in range(2) { total += entry.file; entry = {file: none} }
  total
}
print (label({mnt: 1}, null))
""")?
  for output in [inferred, unguarded] {
    assert output.status == 2, output.stderr
    assert "err[check.type-mismatch]: type mismatch" in output.stderr, output.stderr
    assert "err[check.type-mismatch]: inferred types disagree" in output.stderr, output.stderr
    assert "expected Int, found Int?" in output.stderr, output.stderr
    assert !("compact." in output.stderr), output.stderr
  }
}

test test_a_loop_keeps_a_narrowing_that_its_body_cannot_overwrite { |ctx|
  let output = test.run_xsh(ctx, r"""
type Entry = {file: Int?, other: Int}
pure sibling(found: Int) -> Int {
  var entry: Entry = {file: found, other: 0}
  var total = 0
  if entry.file != null {
    for i in range(2) { total += entry.file; entry.other = i }
  }
  total
}
pure unrelated(start: Int?) -> Int {
  var cur = start
  var other = start
  var total = 0
  if cur != null {
    for i in range(2) { total += cur; other = null }
  }
  total
}
pure reguarded(start: Int?) -> Int {
  var cur = start
  var total = 0
  for i in range(3) {
    cur = if i == 1 { null } else { i }
    if cur != null { total += cur }
  }
  total
}
pure read_only(start: Int?) -> Int {
  var cur = start
  var total = 0
  var n = 0
  if cur != null {
    while n < 3 { total += cur; n += 1 }
  }
  total
}
pure generic_read_only(opts) -> Int {
  var entry = {file: opts.mnt}
  var total = 0
  if entry.file != null {
    for i in range(2) { total += entry.file }
  }
  total
}
print (sibling(4))
print (unrelated(3))
print (reguarded(null))
print (read_only(2))
print (generic_read_only({mnt: 5}))
""")?
  assert output.success, output.stderr
  assert output.stdout == "8\n6\n2\n6\n10\n"
}

test test_a_retry_with_delays_ends_a_narrowing_that_its_body_overwrites { |ctx|
  let output = test.run_xsh(ctx, r"""
error Again = again(message: Str)
pure attempt(count: Int) -> Result[Int, Again] {
  if count < 2 { return Err(Again.again(message: "x")) }
  Ok(count)
}
proc label(start: Int?) [error, time] -> Int {
  var cur = start
  var total = 0
  var n = 0
  if cur != null {
    let r = retry [0ms] { total += cur; cur = null; n += 1; attempt(n)? }?
  }
  total
}
print (label(1))
""")?
  assert output.status == 2, output.stderr
  assert "err[check.operator-type]" in output.stderr, output.stderr
  assert !("runtime." in output.stderr), output.stderr
}
