test test_inferred_record_parameter_preserves_a_pattern_bound_argument { |ctx|
  let output = test.run_xsh(ctx, r"""type Options = {srcaddr: Str?, numeric: Bool}
proc lookup(host: Str, opts) -> Str {
  if opts.numeric { host } else { "off" }
}
proc selected(opts: Options) -> Str {
  if let given = opts.srcaddr { return lookup(given, opts) }
  "absent"
}
print $selected({srcaddr: "127.0.0.1", numeric: true})
print $selected({srcaddr: "127.0.0.1", numeric: false})
print $selected({srcaddr: null, numeric: true})
""")?
  assert output.success, output.stderr
  assert output.stdout == "127.0.0.1\noff\nabsent\n"
}

test test_inferred_address_preserves_record_union_arguments_in_both_branches { |ctx|
  let output = test.run_xsh(ctx, r"""type Options = {timeout: Int}
proc connect_bounded(fd: Int, target, opts: Options) [net, process, error] {
  if opts.timeout < 0 { linux.connect(fd, target) } else { linux.connect(fd, target) }
}
for timeout in [-1, 0] {
  let outcome = connect_bounded(-1, {family: "inet", address: "127.0.0.1", port: 80}, {timeout: timeout})
  assert match outcome { Err(_) => true; Ok(_) => false }
  print "rejected"
}
""")?
  assert output.success, output.stderr
  assert output.stdout == "rejected\nrejected\n"
}

test test_inferred_record_parameter_keeps_an_unguarded_optional_argument_rejected { |ctx|
  let output = test.run_xsh(ctx, r"""type Options = {srcaddr: Str?, numeric: Bool}
proc lookup(host: Str, opts) -> Bool { opts.numeric }
proc selected(opts: Options) { let _ = lookup(opts.srcaddr, opts) }
""")?
  assert ! output.success, output.stdout
  assert "[check.type-mismatch]" in output.stderr, output.stderr
}

test test_guarded_record_spread_preserves_the_source_field_view { |ctx|
  let output = test.run_xsh(ctx, r"""type Options = {srcaddr: Str?, numeric: Bool}
proc lookup(host: Str, opts) -> Str { if opts.numeric { host } else { "off" } }
proc selected(opts: Options) -> Str {
  if let given = opts.srcaddr { return lookup(given, {...opts, numeric: false}) }
  "absent"
}
print $selected({srcaddr: "127.0.0.1", numeric: true})
print $selected({srcaddr: null, numeric: true})
""")?
  assert output.success, output.stderr
  assert output.stdout == "off\nabsent\n"
}

test test_pipeline_callback_projections_authenticate_generic_arguments { |ctx|
  let output = test.run_xsh(ctx, r"""type File = {raw: Bytes}
pure width(raw) -> Int { raw.len() }
pure ordered(files: List[File]) -> List[File] {
  files |> sort-by { |file| width(file.raw) }
}
let files: List[File] = [{raw: b"wide"}, {raw: b"a"}, {raw: b"mid"}]
for file in ordered(files) { print width(file.raw) }
""")?
  assert output.success, output.stderr
  assert output.stdout == "1\n3\n4\n", output.stdout
}

test test_match_arm_preserves_an_optional_assignment_requirement { |ctx|
  let output = test.run_xsh(ctx, r"""pure mode(rest) -> Str? {
  match rest[0] {
    "a" => rest[0]
    else => null
  }
}
pure reversed(rest) -> Str? {
  match rest[0] {
    "a" => { let selected: Str? = rest[0]; selected }
    else => null
  }
}
print (mode(["a"]) ?? "none")
print (mode(["b"]) ?? "none")
print (reversed(["a"]) ?? "none")
""")?
  assert output.success, output.stderr
  assert output.stdout == "a\nnone\na\n", output.stdout
}

test test_empty_argument_uses_the_selected_method_argument_domain { |ctx|
  let output = test.run_xsh(ctx, r"""pure visit(n: Int, xs) -> Int {
  return 0 when n in xs
  let next = xs.push(n)
  visit(n, next) + 1
}
pure start() -> Int {
  let xs = []
  visit(1, xs)
}
pure forwarded(marker) -> Int {
  let _ = marker
  let xs = []
  visit(1, xs)
}
print start()
assert forwarded(1) == 1
assert forwarded("other") == 1
""")?
  assert output.success, output.stderr
  assert output.stdout == "1\n", output.stdout
  let rejected = test.run_xsh(ctx, "pure size(items) { items.len() }\nlet _ = size([])\n")?
  assert ! rejected.success, rejected.stdout
  assert "[check.local-inference]" in rejected.stderr, rejected.stderr
}

test test_registered_dynamic_results_preserve_their_json_boundary { |ctx|
  let output = test.run_xsh(ctx, r"""proc render(marker, text: Str) -> Result[Str] {
  let _ = marker
  let _ = json.decode(text)?
  "ok"
}
print render(1, "{}")?
print render("again", "[]")?
assert render(true, "{") is Err(_)
""")?
  assert output.success, output.stderr
  assert output.stdout == "ok\nok\n", output.stdout
  let rejected = test.run_xsh(ctx, r"""proc render(marker) -> Result[Str] {
  let _ = marker
  let decoded = json.decode("{}")?
  decoded.name
}
""")?
  assert ! rejected.success, rejected.stdout
  assert "[check.dynamic-boundary]" in rejected.stderr, rejected.stderr
}

test test_forwarded_nullable_field_keeps_its_physical_type { |ctx|
  let output = test.expect(ctx, r"""pure project(row) -> Str { if let text = row.text { text } else { "missing" } }
pure build(marker, value: Str?) -> Str { let _ = marker; project({bytes: b"a", text: value}) }
assert build(7, "ok") == "ok"
assert build(false, null) == "missing"
""", status: 0)?
  assert output.stderr == "", output.stderr
  test.expect(ctx, r"""pure project(row) -> Str { if let text = row.text { text } else { "missing" } }
pure build(marker, value: Int?) -> Str { let _ = marker; project({bytes: b"a", text: value}) }
let _ = build(7, 1)
""", status: 2, stderr: ["[check.type-mismatch]"])?
}

test test_inferred_field_unsigned_key_keeps_checked_conversion { |ctx|
  let program = r"""pure read(values: Map[UInt, Str], row) -> Result[Str] { values.get(row.key) }
proc main(...args: List[Str]) [io, error] {
  let values: Map[UInt, Str] = {[0]: "zero"}
  let unsigned: UInt = 0
  assert read(values, {key: unsigned})? == "zero"
  let key = if args.len() == 0 { 0 } else { -1 }
  print read(values, {key})?
}
"""
  let valid = test.expect(ctx, program, status: 0, args: [])?
  assert valid.stdout == "zero\n", valid.stdout
  let invalid = test.expect(ctx, program, status: 3, args: ["negative"])?
  assert invalid.stdout == "", invalid.stdout
  test.expect(ctx, r"""pure read(values: Map[UInt, Str], row) -> Result[Str] { values.get(row.key) }
let values: Map[UInt, Str] = {[0]: "zero"}
let _ = read(values, {key: "not an integer"})
""", status: 2, stderr: ["[check.index-type]"])?
}
