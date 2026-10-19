proc copy_full_inference_metamorphic_module(root: Path, name: Str) [fs, error] -> Result[Unit] {
  let fixture = fp"tests/fixtures/modules/full-inference-metamorphic/{name}"
  fp"{root}/{name}".write(fixture.read_text()?)?
  Ok()
}

test test_full_inference_alpha_renaming_and_independent_declaration_order_preserve_bytes { |ctx|
  let cases = [
    {
      name: "original",
      source: r"""pure add_one(value) { value + 1 }
pure add_two(value) { value + 2 }
let first = add_one(40)
let second = add_two(40)
print $first
print $second
""",
    },
    {
      name: "renamed and reordered",
      source: r"""pure bump(item) { item + 2 }
pure next(number) { number + 1 }
let second = bump(40)
let first = next(40)
print $first
print $second
""",
    },
  ]
  var outputs: List[Str] = []
  for case in cases {
    let output = test.run_xsh(ctx, case.source)?
    assert output.success, f"{case.name}: {output.stderr}"
    outputs = outputs.push(output.stdout)
  }
  assert outputs == ["41\n42\n", "41\n42\n"]
}

test test_full_inference_transparent_generic_aliases_preserve_grounded_values { |ctx|
  let cases = [
    {
      name: "direct application",
      source: r"""type Box[T] = {value: T}
type Batch[T] = {items: List[Box[T]]}
let batch: Batch[List[Int]] = {items: [{value: [2, 3]}, {value: [5]}]}
print ${batch.items[0].value[1] + batch.items[1].value[0]}
""",
    },
    {
      name: "transparent alias",
      source: r"""type Box[T] = {value: T}
type Batch[T] = {items: List[Box[T]]}
type Packed[T] = Batch[List[T]]
type CountBatch = Packed[Int]
let batch: CountBatch = {items: [{value: [2, 3]}, {value: [5]}]}
print ${batch.items[0].value[1] + batch.items[1].value[0]}
""",
    },
  ]
  var outputs: List[Str] = []
  for case in cases {
    let output = test.run_xsh(ctx, case.source)?
    assert output.success, f"{case.name}: {output.stderr}"
    outputs = outputs.push(output.stdout)
  }
  assert outputs == ["8\n", "8\n"]

  for source in [
    r"""type Box[T] = {value: T}
type Batch[T] = {items: List[Box[T]]}
let wrong: Batch[List[Int]] = {items: [{value: ["wrong"]}]}
""",
    r"""type Box[T] = {value: T}
type Batch[T] = {items: List[Box[T]]}
type Packed[T] = Batch[List[T]]
type CountBatch = Packed[Int]
let wrong: CountBatch = {items: [{value: ["wrong"]}]}
""",
  ] {
    test.expect(ctx, source, status: 2, stderr: ["[check.type-mismatch]"])?
  }
}

test test_full_inference_inline_and_named_descriptors_ground_the_same_fields { |ctx|
  let cases = [
    {
      name: "inline descriptor",
      source: r"""let options = cli.parse(["--jobs", "6"], {jobs: {kind: "Int", default: 4}, verbose: "Bool"})?
let jobs: Int = options.jobs
let verbose: Bool = options.verbose
print $jobs $verbose
""",
    },
    {
      name: "named descriptor",
      source: r"""const descriptor = {jobs: {kind: "Int", default: 4}, verbose: "Bool"}
let options = cli.parse(["--jobs", "6"], descriptor)?
let jobs: Int = options.jobs
let verbose: Bool = options.verbose
print $jobs $verbose
""",
    },
  ]
  var outputs: List[Str] = []
  for case in cases {
    let output = test.run_xsh(ctx, case.source)?
    assert output.success, f"{case.name}: {output.stderr}"
    outputs = outputs.push(output.stdout)
  }
  assert outputs == ["6 false\n", "6 false\n"]

  for source in [
    r"""let options = cli.parse(["--jobs", "6"], {jobs: {kind: "Int", default: 4}, verbose: "Bool"})?
let wrong: Str = options.jobs
""",
    r"""const descriptor = {jobs: {kind: "Int", default: 4}, verbose: "Bool"}
let options = cli.parse(["--jobs", "6"], descriptor)?
let wrong: Str = options.jobs
""",
  ] {
    test.expect(ctx, source, status: 2, stderr: ["[check.type-mismatch]"])?
  }
}

test test_full_inference_named_argument_permutations_preserve_values_and_trace_effects { |ctx|
  let value_cases = [
    r"""pure combine(left: Int, right: Int) -> Int { left * 10 + right }
print ${combine(left: 8, right: 9)}
""",
    r"""pure combine(left: Int, right: Int) -> Int { left * 10 + right }
print ${combine(right: 9, left: 8)}
""",
  ]
  var value_outputs: List[Str] = []
  for source in value_cases {
    let output = test.run_xsh(ctx, source)?
    assert output.success, output.stderr
    value_outputs = value_outputs.push(output.stdout)
  }
  assert value_outputs == ["89\n", "89\n"]

  let trace_cases = [
    {
      source: r"""proc mark(label: Str, value: Int) [io] -> Int { print $label; value }
proc choose(left: Int = mark("left default", 1), right: Int = mark("right default", 2)) [io] -> Int {
  print "body"
  left * 10 + right
}
print ${choose(left: mark("left argument", 8), right: mark("right argument", 9))}
""",
      expected: "left argument\nright argument\nbody\n89\n",
    },
    {
      source: r"""proc mark(label: Str, value: Int) [io] -> Int { print $label; value }
proc choose(left: Int = mark("left default", 1), right: Int = mark("right default", 2)) [io] -> Int {
  print "body"
  left * 10 + right
}
print ${choose(right: mark("right argument", 9), left: mark("left argument", 8))}
""",
      expected: "right argument\nleft argument\nbody\n89\n",
    },
    {
      source: r"""proc mark(label: Str, value: Int) [io] -> Int { print $label; value }
proc choose(left: Int = mark("left default", 1), right: Int = mark("right default", 2)) [io] -> Int {
  print "body"
  left * 10 + right
}
print ${choose(right: mark("right supplied", 9))}
""",
      expected: "right supplied\nleft default\nbody\n19\n",
    },
  ]
  for case in trace_cases {
    let output = test.run_xsh(ctx, case.source)?
    assert output.success, output.stderr
    assert output.stdout == case.expected, output.stdout
  }
}

test test_full_inference_helper_extraction_preserves_immutable_capture_and_cleanup_order { |ctx|
  let cases = [
    r"""let prefix = "captured:"
proc render(value: Str) [io] -> Str {
  defer { print f"cleanup:{prefix}" }
  print f"body:{prefix}"
  f"{prefix}{value}"
}
print ${render("value")}
""",
    r"""let prefix = "captured:"
proc compose(value: Str) [io] -> Str {
  print f"body:{prefix}"
  f"{prefix}{value}"
}
proc render(value: Str) [io] -> Str {
  defer { print f"cleanup:{prefix}" }
  compose(value)
}
print ${render("value")}
""",
  ]
  var outputs: List[Str] = []
  for source in cases {
    let output = test.run_xsh(ctx, source)?
    assert output.success, output.stderr
    outputs = outputs.push(output.stdout)
  }
  assert outputs == [
    "body:captured:\ncleanup:captured:\ncaptured:value\n",
    "body:captured:\ncleanup:captured:\ncaptured:value\n",
  ]
}

test test_full_inference_exported_proc_and_stream_schemes_are_client_independent { |ctx|
  let modules = test.temp_dir(ctx, name: "full-inference-metamorphic-clients")?
  copy_full_inference_metamorphic_module(modules, "transport.xsh")?
  let clients = [
    {
      name: "integer client",
      source: r"""use transport as data
let preserved = data.preserve(42)?
let values = data.replay(preserved, 2) |> collect()
assert values == [42, 42]
print $values[0]
print $values[1]
""",
      expected: "42\n42\n",
    },
    {
      name: "text client",
      source: r"""use transport as data
let preserved = data.preserve("demo")?
let values = data.replay(preserved, 2) |> collect()
assert values == ["demo", "demo"]
print $values[0]
print $values[1]
""",
      expected: "demo\ndemo\n",
    },
  ]
  for client in clients {
    let output = test.run_xsh(ctx, client.source, env: {XSH_MODULE_PATH: modules})?
    assert output.success, f"{client.name}: {output.stderr}"
    assert output.stdout == client.expected, output.stdout
  }

  for source in [
    "use transport as data\nlet wrong: Str = data.preserve(42)?\n",
    "use transport as data\nlet wrong: List[Str] = data.replay(42, 2) |> collect()\n",
  ] {
    let output = test.run_xsh(ctx, source, env: {XSH_MODULE_PATH: modules})?
    assert !output.success, output.stderr
    assert "[check.type-mismatch]" in output.stderr, output.stderr
  }
}

test test_full_inference_reordering_independent_import_roots_preserves_client_bytes { |ctx|
  let transport_root = test.temp_dir(ctx, name: "full-inference-metamorphic-transport")?
  let numbers_root = test.temp_dir(ctx, name: "full-inference-metamorphic-numbers")?
  copy_full_inference_metamorphic_module(transport_root, "transport.xsh")?
  copy_full_inference_metamorphic_module(numbers_root, "numbers.xsh")?
  let first_roots = f"{transport_root.display()}:{numbers_root.display()}"
  let second_roots = f"{numbers_root.display()}:{transport_root.display()}"
  let cases = [
    {
      roots: first_roots,
      source: r"""use transport as data
use numbers as math
let value = math.increment(40)
let preserved = data.preserve(value)?
let rows = data.replay(preserved, 2) |> collect()
print $rows[0]
print $rows[1]
""",
    },
    {
      roots: second_roots,
      source: r"""use numbers as arithmetic
use transport as wire
let rows = wire.replay(wire.preserve(arithmetic.increment(40))?, 2) |> collect()
print $rows[0]
print $rows[1]
""",
    },
  ]
  var outputs: List[Str] = []
  for case in cases {
    let output = test.run_xsh(ctx, case.source, env: {XSH_MODULE_PATH: case.roots})?
    assert output.success, output.stderr
    outputs = outputs.push(output.stdout)
  }
  assert outputs == ["41\n41\n", "41\n41\n"]
}
