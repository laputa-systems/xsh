# Typed integer and boolean assignments at a script's top level stay visible
# to every later reader: pipeline blocks, comprehensions, commands, formatted
# strings, and top-level `defer` blocks on both normal and failing exits.

test test_typed_top_level_assignments_reach_every_reader { |ctx|
  let output = test.run_script(ctx, r"""
var n = 0
var flag = false
defer { print f"defer n={n} flag={flag}" }
n += 5
flag = true
let shifted = [1, 2] |> map { . + n }
n += 1
print f"block={shifted[1]} n={n}"
run echo $n
n *= 3
let text = f"{n}"
print $text
var m = 0
let before = [x + m for x in [1, 2]]
m += 4
let after = [x + m for x in [1, 2]]
print f"before={before[0]} after={after[0]}"
""")?
  assert output.status == 0, f"{output.stdout}{output.stderr}"
  assert output.stdout == [
    "block=7 n=6",
    "6",
    "18",
    "before=1 after=5",
    "defer n=18 flag=true",
    "",
  ].join("\n"), output.stdout
}

test test_typed_top_level_assignment_reaches_defer_on_failure { |ctx|
  let output = test.run_script(ctx, r"""
var n = 0
defer { print f"defer n={n}" }
n += 7
error.fail("boom")
""")?
  assert output.status != 0, output.stdout
  assert output.stdout == "defer n=7\n", output.stdout
}
