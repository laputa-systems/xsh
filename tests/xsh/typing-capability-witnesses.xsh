proc typing_module_fixture(ctx: TestContext, name: Str) [fs, error] -> Result[Str] {
  fp"{ctx.file.parent()}/../fixtures/modules/typing-capability/{name}".read_text()
}

test test_typing_identity_and_open_record_projection { |ctx|
  let output = test.run_script(
    ctx,
    r"""pure identity(value) {
  value
}

pure name_of(value) {
  value.name
}

let integer: Int = identity(42)
let text: Str = identity("demo")
let bool: Bool = identity(false)
let sizes: List[UInt] = identity([1KiB, 2KiB])
assert integer == 42
assert text == "demo"
assert bool == false
assert sizes == [1KiB, 2KiB]
assert !bool
let package_name: Str = name_of({name: "package", version: "1.0"})
let numeric_name: Int = name_of({name: 42, enabled: true})
let boolean_name: Bool = name_of({name: false, enabled: true})
assert package_name == "package"
assert numeric_name == 42
assert boolean_name == false
assert !boolean_name
""",
    [],
    {},
    b"",
    "typing-relationships.xsh",
  )?
  assert output.success, output.stderr
  assert output.stdout == ""
}

test test_typing_identity_rejects_a_wrong_context { |ctx|
  test.expect(
    ctx,
    r"""pure identity(value) {
  value
}

let wrong: Str = identity(42)
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}

test test_typing_open_record_projection_requires_the_projected_field { |ctx|
  test.expect(
    ctx,
    r"""pure name_of(value) {
  value.name
}

let missing: Str = name_of({version: "1.0"})
""",
    status: 2,
    stderr: ["[check.unknown-field]"],
  )?
}

test test_typing_annotated_projection_missing_field_remains_rejected { |ctx|
  test.expect(
    ctx,
    r"""type MissingName = {version: Str}

pure name_of(value: MissingName) -> Str {
  value.name
}

let missing = name_of({version: "1.0"})
""",
    status: 2,
    stderr: ["[check.unknown-field]"],
  )?
}

test test_typing_dynamic_values_do_not_gain_a_concrete_type { |ctx|
  test.expect(
    ctx,
    r"""pure identity(value: Any) -> Any {
  value
}

let raw: Any = 42
let concrete: Int = identity(raw)
""",
    status: 2,
    stderr: ["[check.dynamic-boundary]"],
  )?
}

test test_typing_generic_module_alias_preserves_independent_instances { |ctx|
  let module_dir = test.temp_dir(ctx, name: "typing-generic-pure-value")?
  fp"{module_dir}/model.xsh".write("##! Generic module callable.\n## Returns its input.\nexport pure identity(value) { value }\n")?
  let result = test.run_xsh(
    ctx,
    "use model\nlet identity = model.identity\nlet number: Int = identity(42)\nlet text: Str = identity(\"demo\")\nlet boolean: Bool = identity(false)\nassert !boolean\nprint $number\nprint $text\nprint $boolean\n",
    env: {XSH_MODULE_PATH: module_dir},
  )?
  assert result.success, result.stderr
  assert result.stdout == "42\ndemo\nfalse\n"

  let null_safe = test.run_xsh(
    ctx,
    "use model\nlet wrapped = Ok(model)\nlet identity = wrapped?.identity\n",
    env: {XSH_MODULE_PATH: module_dir},
  )?
  assert null_safe.status == 2, null_safe.stderr
  assert "[check.module-member]" in null_safe.stderr, null_safe.stderr

  let projected = test.run_xsh(
    ctx,
    "use model\nlet identity = model.get(\"identity\")?\n",
    env: {XSH_MODULE_PATH: module_dir},
  )?
  assert projected.status == 2, projected.stderr
  assert "[check.module-member]" in projected.stderr, projected.stderr

  let indexed = test.run_xsh(
    ctx,
    "use model\nlet identity = model[\"identity\"]\n",
    env: {XSH_MODULE_PATH: module_dir},
  )?
  assert indexed.status == 2, indexed.stderr
  assert "[check.module-member]" in indexed.stderr, indexed.stderr
}

test test_typing_concrete_module_index_beside_generic_export { |ctx|
  let module_dir = test.temp_dir(ctx, name: "typing-concrete-module-index")?
  fp"{module_dir}/model.xsh".write("##! Mixed module exports.\n## Returns its input.\nexport pure identity(value) { value }\n## The concrete answer.\nexport const answer = 42\n")?
  let result = test.run_xsh(
    ctx,
    "use model\nassert model[\"answer\"] == 42\nassert model.get(\"answer\")? == 42\n",
    env: {XSH_MODULE_PATH: module_dir},
  )?
  assert result.status == 0, result.stderr

  let dynamic = test.run_xsh(
    ctx,
    "use model\npure lookup(key: Str) -> Any { model[key] }\n",
    env: {XSH_MODULE_PATH: module_dir},
  )?
  assert dynamic.status == 2, dynamic.stderr
  assert "[check.module-member]" in dynamic.stderr, dynamic.stderr
}

test test_typing_recursive_parameter_conflicts_in_both_orders { |ctx|
  for source in [
    "pure first(value) -> Int { second(\"wrong\") + value }\npure second(value) -> Int { first(value) }\nlet answer = first(1)\n",
    "pure second(value) -> Int { first(value) }\npure first(value) -> Int { second(\"wrong\") + value }\nlet answer = first(1)\n",
  ] {
    test.expect(ctx, source, status: 2, stderr: ["[check.type-mismatch]"])?
  }
}

test test_typing_recursive_parameters_need_declaration_evidence { |ctx|
  for source in [
    "pure first(value) -> Int { second(value) }\npure second(value) -> Int { first(value) }\nlet answer = first(1)\n",
    "let answer = first(1)\npure first(value) -> Int { second(value) }\npure second(value) -> Int { first(value) }\n",
  ] {
    test.expect(ctx, source, status: 2, stderr: ["[check.local-inference]"])?
  }
}

test test_typing_recursive_parameters_infer_one_concrete_monotype { |ctx|
  for source in [
    "pure first(value) -> Int { if value == 0 { 0 } else { second(value - 1) + 1 } }\npure second(value) -> Int { first(value) }\nassert first(2) == 2\n",
    "pure second(value) -> Int { first(value) }\npure first(value) -> Int { if value == 0 { 0 } else { second(value - 1) + 1 } }\nassert first(2) == 2\n",
  ] {
    test.expect(ctx, source, status: 0)?
  }
}

test test_typing_exported_scheme_is_independent_of_other_importers { |ctx|
  let module_dir = test.temp_dir(ctx, name: "typing-importers")?
  let identity = typing_module_fixture(ctx, "typing-capability-identity.xsh")?
  let reordered_identity = typing_module_fixture(ctx, "typing-capability-identity-reordered.xsh")?
  let integer_importer = typing_module_fixture(ctx, "typing-capability-import-int.xsh")?
  let text_importer = typing_module_fixture(ctx, "typing-capability-import-text.xsh")?
  let integer_root = typing_module_fixture(ctx, "typing-capability-root-int.xsh")?
  let text_root = typing_module_fixture(ctx, "typing-capability-root-text.xsh")?
  let joint_root = typing_module_fixture(ctx, "typing-capability-root-joint.xsh")?
  let reversed_joint_root = typing_module_fixture(ctx, "typing-capability-root-joint-reversed.xsh")?
  let bool_root = typing_module_fixture(ctx, "typing-capability-root-bool.xsh")?

  fp"{module_dir}/typing-capability-identity.xsh".write(identity)?
  fp"{module_dir}/typing-capability-import-int.xsh".write(integer_importer)?
  let only_integer = test.run_script(
    ctx,
    integer_root,
    [],
    {XSH_MODULE_PATH: module_dir},
    b"",
    "typing-root-int.xsh",
  )?

  fp"{module_dir}/typing-capability-import-text.xsh".write(text_importer)?
  let only_text = test.run_script(
    ctx,
    text_root,
    [],
    {XSH_MODULE_PATH: module_dir},
    b"",
    "typing-root-text.xsh",
  )?
  let both_importers = test.run_script(
    ctx,
    joint_root,
    [],
    {XSH_MODULE_PATH: module_dir},
    b"",
    "typing-root-joint.xsh",
  )?
  let reversed_importers = test.run_script(
    ctx,
    reversed_joint_root,
    [],
    {XSH_MODULE_PATH: module_dir},
    b"",
    "typing-root-joint-reversed.xsh",
  )?

  fp"{module_dir}/typing-capability-import-text.xsh".remove(missing_ok: false)?
  let after_text_removal = test.run_script(
    ctx,
    integer_root,
    [],
    {XSH_MODULE_PATH: module_dir},
    b"",
    "typing-root-int-after-removal.xsh",
  )?
  fp"{module_dir}/typing-capability-import-text.xsh".write(text_importer)?
  fp"{module_dir}/typing-capability-import-int.xsh".remove(missing_ok: false)?
  let after_integer_removal = test.run_script(
    ctx,
    text_root,
    [],
    {XSH_MODULE_PATH: module_dir},
    b"",
    "typing-root-text-after-removal.xsh",
  )?

  fp"{module_dir}/typing-capability-identity.xsh".write(reordered_identity)?
  let reordered_bool = test.run_script(
    ctx,
    bool_root,
    [],
    {XSH_MODULE_PATH: module_dir},
    b"",
    "typing-root-bool-after-reorder.xsh",
  )?

  let all_succeeded = only_integer.success and only_text.success and both_importers.success and reversed_importers.success and after_text_removal.success and after_integer_removal.success and reordered_bool.success
  assert all_succeeded, f"integer-only: {only_integer.stderr}; text-only: {only_text.stderr}; A-to-B: {both_importers.stderr}; B-to-A: {reversed_importers.stderr}; after text removal: {after_text_removal.stderr}; after integer removal: {after_integer_removal.stderr}; reordered bool: {reordered_bool.stderr}"
  assert only_integer.stdout == "42\n7\n17\n"
  assert only_text.stdout == "demo\ndemo\n17\n"
  assert both_importers.stdout == "42\n7\ndemo\ndemo\n17\n17\n"
  assert reversed_importers.stdout == "demo\ndemo\n42\n7\n17\n17\n"
  assert after_text_removal.stdout == only_integer.stdout
  assert after_integer_removal.stdout == only_text.stdout
  assert reordered_bool.stdout == ""
}

test test_typing_host_result_anchor_and_nested_loops { |ctx|
  let annotated = r"""pure anchor(value: Str) -> Bool { value == "x" }
pure count_words(text: Str) -> Int {
  let flat = text.trim()
  let anchored = anchor(flat)
  var count = 0
  for line in flat.split("\n") {
    for word in line.split(" ") { count += 1 }
  }
  count
}
assert count_words(" a b\nc ") == 3
"""
  for source in [annotated, annotated.replace("text: Str", with: "text")] {
    let output = test.run_script(ctx, source, [], {}, b"", "typing-host-result-anchor.xsh")?
    assert output.success, output.stderr
    assert output.stdout == ""
  }
}

test test_typing_host_constraints_follow_chains_and_body_order { |ctx|
  for body in [
    r"""let flat = text.replace("\t", with: " ")
  let first = text.trim()
  let clean = first.trim()
  let anchored = anchor(clean)
  flat""",
    r"""let first = text.trim()
  let clean = first.trim()
  let anchored = anchor(clean)
  let flat = text.replace("\t", with: " ")
  flat""",
  ] {
    let source = "pure replace(text) -> Str {\n" + body + r"""
}
pure anchor(value: Str) -> Bool { value == "x" }
assert replace("a\tb") == "a b"
"""
    let output = test.run_script(ctx, source, [], {}, b"", "typing-host-constraints.xsh")?
    assert output.success, output.stderr
    assert output.stdout == ""
  }
}

test test_typing_host_replacement_result_keeps_receiver_ambiguity { |ctx|
  for caller in ["\"a\\tb\"", "rx\"a\""] {
    let source = r"""pure anchor(value: Str) -> Bool { value == "x" }
pure ambiguous(text) -> Str {
  let flat = text.replace("\t", with: " ")
  let anchored = anchor(flat)
  flat
}
let output = ambiguous(""" + caller + ")\n"
    test.expect(ctx, source, status: 2, stderr: ["[check.ambiguous-overload]"])?
  }
}

test test_typing_host_byte_receiver_keeps_ambiguity_after_bytes_caller { |ctx|
  test.expect(
    ctx,
    r"""pure byte(raw) -> Int { raw.byte_at(0) ?? 0 }
let output = byte(b"A")
""",
    status: 2,
    stderr: ["[check.ambiguous-overload]"],
  )?
}

test test_typing_host_inference_preserves_argument_and_effect_errors { |ctx|
  test.expect(
    ctx,
    r"""pure invalid(text) -> Str { text.replace(1, with: " ") }
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
  test.expect(
    ctx,
    r"""pure invalid(file) -> Str { file.read_text() ?? "" }
""",
    status: 2,
    stderr: ["[check.pure-effect]"],
  )?
}

test test_typing_host_untemplated_receiver_domains_remain_candidates { |ctx|
  test.expect(
    ctx,
    r"""pure keys(value) -> List[Str] { value.keys() }
let output = keys({value: 1})
""",
    status: 2,
    stderr: ["[check.ambiguous-overload]"],
  )?
  test.expect(
    ctx,
    r"""pure number(value) -> Float { value.float() }
let output = number(1)
""",
    status: 2,
    stderr: ["[check.local-inference]"],
  )?
  let output = test.run_script(
    ctx,
    r"""type Row = {value: Int}
pure record_keys(value: Row) -> List[Str] { value.keys() }
pure map_keys(value: Map[Str, Int]) -> List[Str] { value.keys() }
pure signed(value: Int) -> Float { value.float() }
pure unsigned(value: UInt) -> Float { value.float() }
assert record_keys({value: 1}) == ["value"]
assert map_keys(map.empty()).len() == 0
assert signed(2) == 2.0
assert unsigned(2) == 2.0
""",
    [],
    {},
    b"",
    "typing-host-domain-controls.xsh",
  )?
  assert output.success, output.stderr
  assert output.stdout == ""
}
