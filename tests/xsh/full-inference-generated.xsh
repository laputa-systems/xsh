test test_generated_inference_equivalences_and_type_mutants { |ctx|
  let modules = test.temp_dir(ctx, name: "generated-inference-modules")?
  for name in ["numbers.xsh", "transport.xsh"] {
    let fixture = fp"tests/fixtures/modules/full-inference-metamorphic/{name}"
    fp"{modules}/{name}".write(fixture.read_text()?)?
  }
  for seed in [2, 5, 9] {
    for variant in [0, 1] {
      let imports = if variant == 0 { "use numbers as math\nuse transport as wire\n" } else { "use transport as wire\nuse numbers as math\n" }
      let declarations = if variant == 0 {
        "pure increase(value: Int) -> Int { math.increment(value) }\npure twice(value: Int) -> Int { value * 2 }\n"
      } else {
        "pure twice(item) { item * 2 }\npure increase(item) { math.increment(item) }\n"
      }
      let aliases = if variant == 0 { "type Count = Int\n" } else { "type Base = Int\ntype Count = Base\n" }
      let descriptor = if variant == 0 {
        "let options = cli.parse([], {jobs: {kind: \"Int\", default: 4}})?\n"
      } else {
        "const descriptor = {jobs: {kind: \"Int\", default: 4}}\nlet options = cli.parse([], descriptor)?\n"
      }
      let source = imports + declarations + aliases + descriptor + f"let value: Count = twice(increase({seed})) + options.jobs\n" + r"""let rows = wire.replay(wire.preserve(value)?, 2) |> collect()
print ${rows[0]}
print ${rows[1]}
"""
      let output = test.run_xsh(ctx, source, env: {XSH_MODULE_PATH: modules})?
      assert output.success, f"seed {seed}, variant {variant}: {output.stderr}"
      let expected = (seed + 1) * 2 + 4
      assert output.stdout == f"{expected}\n{expected}\n", output.stdout
      let mutant = source + "let wrong: Str = rows[0]\n"
      let rejected = test.run_xsh(ctx, mutant, env: {XSH_MODULE_PATH: modules})?
      assert rejected.status == 2, rejected.stderr
      assert "[check.type-mismatch]" in rejected.stderr, rejected.stderr
    }
  }
}

test test_generated_defaults_extraction_and_cleanup_traces { |ctx|
  for seed in [3, 7, 11] {
    for variant in [0, 1] {
      let helper = if variant == 0 { "" } else { "proc compute(item: Int) [io] -> Int { print \"body\"; item * 10 + 2 }\n" }
      let body = if variant == 0 { "print \"body\"; value * 10 + 2" } else { "compute(value)" }
      let signature = if variant == 0 { "proc render(value: Int = fallback()) [io] -> Result[Int]" } else { "proc render(value = fallback())" }
      let source = "proc fallback() [io] -> Int { print \"default\"; 1 }\n" + helper + signature + " { defer { print \"cleanup\" }; " + body + " }\n" + r"""print ${render(value: """ + f"{seed}" + r""" )?}
print ${render()?}
"""
      let output = test.run_xsh(ctx, source)?
      assert output.success, f"seed {seed}, variant {variant}: {output.stderr}"
      let expected = seed * 10 + 2
      assert output.stdout == f"body\ncleanup\n{expected}\ndefault\nbody\ncleanup\n12\n", output.stdout
      let mutant = source + "let wrong: Str = render(value: 4)?\n"
      test.expect(ctx, mutant, status: 2, stderr: ["[check.type-mismatch]"])?
    }
  }
}

test test_generated_named_argument_order_and_failure_cleanup { |ctx|
  for seed in [3, 7, 11] {
    for reverse in [false, true] {
      let call = if reverse {
        r"""right: mark("right", 2), left: mark("left", """ + f"{seed}" + ")"
      } else {
        r"""left: mark("left", """ + f"{seed}" + r"""), right: mark("right", 2)"""
      }
      let source = r"""proc mark(label: Str, value: Int) [io] -> Int { print $label; value }
proc combine(left: Int, right: Int) [io] -> Result[Int] {
  defer { print "cleanup" }
  print "body"
  fail "bad input" unless left > 0
  left * 10 + right
}
""" + r"""print ${combine(""" + call + ")?}\n"
      let output = test.run_xsh(ctx, source)?
      assert output.success, output.stderr
      let order = if reverse { "right\nleft\n" } else { "left\nright\n" }
      let value = seed * 10 + 2
      assert output.stdout == order + f"body\ncleanup\n{value}\n", output.stdout

    }
  }
  for signature in ["proc combine(left: Int, right: Int) [io] -> Result[Int]", "proc combine(left, right)"] {
    let failing = signature + r""" {
  defer { print "cleanup" }
  print "body"
  fail "bad input" unless left > 0
  left * 10 + right
}
match combine(left: 0, right: 2) {
  Ok(value) => print $value
  Err(problem) => print $problem.message
}
"""
    let failed = test.run_xsh(ctx, failing)?
    assert failed.success, failed.stderr
    assert failed.stdout == "body\ncleanup\nbad input\n", failed.stdout
  }
}
