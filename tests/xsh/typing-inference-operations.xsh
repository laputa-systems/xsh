test generic_sealed_add_two_domains [error] { |ctx|
  let output = test.run_script(ctx, r"""pure add(left, right) { left + right }
let integer: Int = add(7, 11)
let decimal: Float = add(1.25, 2.5)
print ${integer} ${decimal}
""")?
  assert output.success, output.stderr
  output.status == 0
  output.stdout == "18 3.75\n"
}

test generic_sealed_add_forwarded_requirements [error] { |ctx|
  let output = test.run_script(ctx, r"""pure add(left, right) { left + right }
pure forwarded(left, right) { add(left, right) }
let integer: Int = forwarded(19, 23)
let decimal: Float = forwarded(0.5, 0.75)
print ${integer} ${decimal}
""")?
  assert output.success, output.stderr
  output.status == 0
  output.stdout == "42 1.25\n"
}

test generic_sealed_add_unsupported_domain_rejected [error] { |ctx|
  let output = test.run_script(ctx, r"""pure add(left, right) { left + right }
let _ = add(true, false)
""")?
  output.status == 2
  output.stdout == ""
  assert "check." in output.stderr, output.stderr
  assert "Bool" in output.stderr, output.stderr
  assert "parse." not in output.stderr, output.stderr
}

test generic_sealed_add_mixed_domains_rejected [error] { |ctx|
  let output = test.run_script(ctx, r"""pure add(left, right) { left + right }
let _ = add(1, 2.5)
""")?
  output.status == 2
  output.stdout == ""
  assert "check." in output.stderr, output.stderr
  assert "Int" in output.stderr, output.stderr
  assert "Float" in output.stderr, output.stderr
  assert "parse." not in output.stderr, output.stderr
}

test generic_computed_call_scheme_scope [error] { |ctx|
  let output = test.run_script(ctx, r"""pure identity(value) { value }
pure invoke(callback, value) { callback(value) }
let alias = identity
print ${invoke(alias, 7)} ${invoke(alias, "word")} ${invoke(alias, false)}
""")?
  assert output.success, output.stderr
  output.status == 0
  output.stdout == "7 word false\n"
}

test generic_computed_call_rank_one_callback_rejected [error] { |ctx|
  let output = test.run_script(ctx, r"""pure identity(value) { value }
pure twice(callback) {
  let number: Int = callback(1)
  let text: Str = callback("word")
  number
}
let _ = twice(identity)
""")?
  output.status == 2
  output.stdout == ""
  assert "check." in output.stderr, output.stderr
  assert "Int" in output.stderr, output.stderr
  assert "Str" in output.stderr, output.stderr
  assert "parse." not in output.stderr, output.stderr
}

type OperationSourceCheck = {status: Status, stdout: Str, stderr: Str}

test operation_source_command_argv_preserves_native_input_domains [fs, process, error] { |ctx|
  let prefix = r"""pure recipe(target, argv) { process.command_argv(target: target, argv: argv) }
pure forwarded(target, argv) { recipe(target, argv) }
"""
  let accepted = check_operation_source(ctx, prefix + r"""let text = forwarded("child", ["child", "word"])
let paths = forwarded(Path("child"), [Path("child"), Path("item")])
pure mixed(target: Path) -> Command { process.command_argv(target, ["child", Path("item")]) }
pure dynamic(target: Any, argv: Any) -> Command { process.command_argv(argv: argv, target: target) }
""")?
  let rejected = check_operation_source(ctx, prefix + "let wrong = forwarded(\"child\", [7])\n")?
  let empty = check_operation_source(ctx, "let wrong = process.command_argv(\"child\", [])\n")?
  assert accepted.status.exited_with(0), accepted.stderr
  assert_operation_source_rejection(rejected)
  assert_operation_source_rejection(empty)
  assert "check.process-argv-empty" in empty.stderr, empty.stderr
}

proc check_operation_source(ctx: TestContext, source: Str) [fs, process, error] -> Result[OperationSourceCheck] {
  let file = test.temp_file(ctx, name: "operation-contract.xsh", contents: bytes.from_text(source))?
  let checked = run.capture --text "xsht" check $file ?
  let {status, stdout, stderr, ..} = checked
  {status, stdout, stderr}
}

proc assert_operation_source_rejection(checked: OperationSourceCheck) [error] -> Unit {
  assert checked.status.exited_with(2), checked.stderr
  assert "check." in checked.stderr, checked.stderr
  assert "parse." not in checked.stderr, checked.stderr
  assert "compact.indexed-build" not in checked.stderr, checked.stderr
}

test operation_source_record_get_keeps_the_selected_producer_permissions [fs, process, error] { |ctx|
  let prefix = r"""stream clocked() [time] {
  let value = time.now()
  yield 1
}
"""
  let accepted = check_operation_source(ctx, prefix + r"""proc consumed() [time, error] -> Result[List[Int]] {
  let record = {rows: clocked(), ignored: ["local"]}
  record.get("rows")?.collect()
}
""")?
  let rejected = check_operation_source(ctx, prefix + r"""proc consumed() [error] -> Result[List[Int]] {
  let record = {rows: clocked(), ignored: ["local"]}
  record.get("rows")?.collect()
}
""")?
  assert accepted.status.exited_with(0), accepted.stderr
  assert_operation_source_rejection(rejected)
}

test operation_source_named_native_module_forwarding [fs, process, error] { |ctx|
  let prefix = r"""pure added(items, item, marker) {
  let _ = marker
  set.add(item: item, set: items)
}
pure forwarded(items, item, marker) { added(marker: marker, item: item, items: items) }
let seed: Map[Bool] = {one: true}
"""
  let accepted = check_operation_source(ctx, prefix + r"""let first: Map[Bool] = forwarded(marker: 1, item: "two", items: seed)
let second: Map[Bool] = forwarded(items: seed, marker: false, item: "three")
""")?
  let rejected = check_operation_source(ctx, prefix + "let wrong = forwarded(items: seed, item: 7, marker: false)\n")?
  assert accepted.status.exited_with(0), accepted.stderr
  assert_operation_source_rejection(rejected)
}

test operation_source_fresh_map_module_results_keep_key_and_value_constraints [fs, process, error] { |ctx|
  let prefix = r"""pure empty(marker) { let _ = marker; map.empty() }
pure forwarded(marker) { empty(marker: marker) }
"""
  let accepted = check_operation_source(ctx, prefix + r"""let labels: Map[Int, Str] = forwarded(marker: 1)
let counts: Map[Bool, Int] = forwarded(marker: false)
""")?
  let rejected = check_operation_source(ctx, prefix + "let wrong: Map[Float, Int] = forwarded(marker: false)\n")?
  assert accepted.status.exited_with(0), accepted.stderr
  assert_operation_source_rejection(rejected)
}

test operation_source_standard_method_candidates_ignore_caller_order [fs, process, error] { |ctx|
  let prefix = r"""pure appended(values, item) { values.push(item: item) }
pure forwarded(values, item) { appended(item: item, values: values) }
"""
  for calls in [
    "let numbers: List[Int] = forwarded(values: [1], item: 2)\nlet words: List[Str] = forwarded(values: [\"one\"], item: \"two\")\n",
    "let words: List[Str] = forwarded(values: [\"one\"], item: \"two\")\nlet numbers: List[Int] = forwarded(values: [1], item: 2)\n",
  ] {
    let accepted = check_operation_source(ctx, prefix + calls)?
    assert accepted.status.exited_with(0), accepted.stderr
  }
  let rejected = check_operation_source(ctx, prefix + "let wrong = forwarded(values: [1], item: \"two\")\n")?
  assert_operation_source_rejection(rejected)
}

test operation_source_materialized_line_consumers_keep_receiver_identity [fs, process, error] { |ctx|
  let prefix = r"""pure collected(value) { value.lines().collect() }
pure forwarded(value) { collected(value: value) }
"""
  let accepted = check_operation_source(ctx, prefix + r"""let text: List[Str] = forwarded(value: "one\ntwo\n")
let binary: List[Bytes] = forwarded(value: b"one\n")
""")?
  let rejected = check_operation_source(ctx, prefix + "let wrong: List[Int] = forwarded(value: \"one\")\n")?
  assert accepted.status.exited_with(0), accepted.stderr
  assert_operation_source_rejection(rejected)
}

test operation_source_stream_collect_retains_pull_and_close_permissions [fs, process, error] { |ctx|
  let prefix = r"""stream delayed() [time, env] -> Stream[Int] {
  defer { let _ = env.get("UNREAD_SETTING") }
  let _ = time.now()
  yield 1
}
let rows = delayed()
"""
  let accepted = check_operation_source(ctx, prefix + "proc collected() [time, env] -> List[Int] { rows.collect() }\n")?
  let rejected = check_operation_source(ctx, prefix + "proc collected() [time] -> List[Int] { rows.collect() }\n")?
  assert accepted.status.exited_with(0), accepted.stderr
  assert_operation_source_rejection(rejected)
  assert "check.effect-violation" in rejected.stderr, rejected.stderr
  assert "env" in rejected.stderr, rejected.stderr
}

test operation_source_generalized_stage_projection_keeps_item_relationships [fs, process, error] { |ctx|
  let prefix = r"""pure names(values) { values |> map { |item| item.name } }
pure forwarded(values) { names(values: values) }
"""
  let accepted = check_operation_source(ctx, prefix + r"""let numeric: List[Int] = forwarded(values: [{name: 7}])
let flags: List[Bool] = forwarded(values: [{name: false, extra: "wide"}])
""")?
  let rejected = check_operation_source(ctx, prefix + "let wrong = forwarded(values: [{other: 7}])\n")?
  assert accepted.status.exited_with(0), accepted.stderr
  assert_operation_source_rejection(rejected)
}

test operation_source_named_stage_callback_requires_its_effects [fs, process, error] { |ctx|
  let prefix = r"""proc clocked(item: Int) [time] -> Int { let _ = time.now(); item }
"""
  let accepted = check_operation_source(ctx, prefix + "proc mapped(values: List[Int]) [time] -> List[Int] { values |> map(clocked) }\n")?
  let rejected = check_operation_source(ctx, prefix + "proc mapped(values: List[Int]) [] -> List[Int] { values |> map(clocked) }\n")?
  assert accepted.status.exited_with(0), accepted.stderr
  assert_operation_source_rejection(rejected)
  assert "check.effect-violation" in rejected.stderr, rejected.stderr
  assert "time" in rejected.stderr, rejected.stderr
}

test operation_source_result_flat_map_requires_propagation_effect [fs, process, error] { |ctx|
  let prefix = "pure expanded(item: Str) -> Result[List[Str]] { Ok([item]) }\n"
  let accepted = check_operation_source(ctx, prefix + "proc flattened(values: List[Str]) [error] -> List[Str] { values |> flat-map(expanded) }\n")?
  let rejected = check_operation_source(ctx, prefix + "proc flattened(values: List[Str]) [] -> List[Str] { values |> flat-map(expanded) }\n")?
  assert accepted.status.exited_with(0), accepted.stderr
  assert_operation_source_rejection(rejected)
  assert "check.effect-violation" in rejected.stderr, rejected.stderr
  assert "error" in rejected.stderr, rejected.stderr
}

test operation_source_explicit_registry_schema_validation_keeps_dynamic_boundary [fs, process, error] { |ctx|
  let prefix = "pure identity(value) { value }\nlet raw: Any = {name: \"demo\", value: \"text\"}\n"
  let accepted = check_operation_source(ctx, prefix + "let validated: Result[EnvEntry] = identity(raw).require(EnvEntry)\n")?
  let rejected = check_operation_source(ctx, prefix + "let unchecked: EnvEntry = identity(raw)\n")?
  assert accepted.status.exited_with(0), accepted.stderr
  assert_operation_source_rejection(rejected)
  assert "check.dynamic-boundary" in rejected.stderr, rejected.stderr
}

test operation_source_json_eligibility_survives_generic_forwarding [fs, process, error] { |ctx|
  let prefix = r"""pure encoded(value) { json.encode(value: value) }
pure forwarded(value) { encoded(value: value) }
"""
  let accepted = check_operation_source(ctx, prefix + "let text: Result[Str] = forwarded(value: {name: \"demo\", count: 7})\n")?
  let rejected = check_operation_source(ctx, prefix + "let wrong = forwarded(value: p\"native\")\n")?
  assert accepted.status.exited_with(0), accepted.stderr
  assert_operation_source_rejection(rejected)
}

test operation_source_builtin_nominal_payload_keeps_declaring_family [fs, process, error] { |ctx|
  let prefix = "pure identity(value) { value }\n"
  let accepted = check_operation_source(ctx, prefix + "let failure: AssertionError = identity(AssertionError.Failed(message: \"failed\"))\n")?
  let rejected = check_operation_source(ctx, prefix + "let wrong = identity(AssertionError.Failed(message: 7))\n")?
  assert accepted.status.exited_with(0), accepted.stderr
  assert_operation_source_rejection(rejected)
}

test operation_source_nonadd_arithmetic_and_membership_keep_finite_domains [fs, process, error] { |ctx|
  let prefix = r"""pure scaled(value, factor) { value * factor }
pure member(item, values) { item in values }
"""
  let accepted = check_operation_source(ctx, prefix + r"""let integer: Int = scaled(7, 2)
let decimal: Float = scaled(1.5, 2.0)
let listed: Bool = member(7, [7, 8])
let keyed: Bool = member("name", {name: 7})
""")?
  let rejected = check_operation_source(ctx, prefix + "let wrong = scaled(true, false)\n")?
  assert accepted.status.exited_with(0), accepted.stderr
  assert_operation_source_rejection(rejected)
}

test operation_source_path_literal_coercion_preserves_the_path_boundary [fs, process, error] { |ctx|
  let accepted = check_operation_source(ctx, r"""pure relative(value: Path) -> Result[Path] { value.strip_prefix(prefix: "base") }
let relative_path: Result[Path] = relative(p"base/item")
""")?
  let rejected = check_operation_source(ctx, r"""pure relative(value: Path, prefix: Str) -> Result[Path] { value.strip_prefix(prefix: prefix) }
""")?
  assert accepted.status.exited_with(0), accepted.stderr
  assert_operation_source_rejection(rejected)
}

test operation_source_result_context_keeps_registered_arity_and_result_data [fs, process, error] { |ctx|
  let accepted = check_operation_source(ctx, r"""let original: Result[Int] = Ok(7)
let short: Result[Int] = original.context("kind")
let named: Result[Int] = original.context(message: "message", kind: "kind")
""")?
  let rejected = check_operation_source(ctx, r"""let original: Result[Int] = Ok(7)
let wrong = original.context("kind", "message", 1)
""")?
  assert accepted.status.exited_with(0), accepted.stderr
  assert_operation_source_rejection(rejected)
  assert "check.arity" in rejected.stderr, rejected.stderr
}

test operation_source_cli_descriptors_keep_static_plans_and_dynamic_boundaries [fs, process, error] { |ctx|
  let accepted = check_operation_source(ctx, r"""const schema = {count: {kind: "Int", default: 7}}
const commands = {build: {positionals: ["root"], types: {root: "Path"}}}
const fallback = {positionals: ["root"], types: {root: "Path"}}
let parsed = cli.parse(argv: [], schema: schema)?
let full = cli.parse_full(schema: schema, argv: [])?
let applet_values = cli.applet([], schema)?
let command = cli.commands(commands: commands, argv: ["build", "workspace"])?
let rootless = cli.commands(fallback_command: fallback, commands: commands, rootless_default: "build", argv: ["workspace"])?
let count: Int = parsed.count
let full_count: Int = full.values.count
let applet_count: Int = applet_values.count
let root: Path = command.root
let fallback_root: Path = rootless.root
proc dynamic(commands: Record) [error] -> Result[Record] { cli.commands([], commands) }
""")?
  let rejected = check_operation_source(ctx, r"""const schema = {count: {kind: "UnknownKind"}}
let output = cli.parse([], schema)
""")?
  assert accepted.status.exited_with(0), accepted.stderr
  assert_operation_source_rejection(rejected)
  assert "check.cli-descriptor" in rejected.stderr, rejected.stderr
}

test operation_source_hash_aliases_keep_selector_authority_and_fs_permission [fs, process, error] { |ctx|
  let accepted = check_operation_source(ctx, r"""proc md5(file: Path) [fs] { hash.verify_file(file, md5: "checksum") }
proc sha1(file: Path) [fs] { hash.verify_file(file, sha1: "checksum") }
proc sha256(file: Path) [fs] { hash.verify_file(file, sha256: "checksum") }
proc sha512(file: Path) [fs] { hash.verify_file(file, sha512: "checksum") }
""")?
  let rejected = check_operation_source(ctx, r"""proc unchecked(file: Path) [] { hash.verify_file(file, sha256: "checksum") }
""")?
  assert accepted.status.exited_with(0), accepted.stderr
  assert_operation_source_rejection(rejected)
}

test operation_source_run_observations_keep_all_result_envelopes [fs, process, error] { |ctx|
  let checked = check_operation_source(ctx, r"""type TextObservation = {status: Status, stdout: Str, stderr: Str}
type BytesObservation = {status: Status, stdout: Bytes, stderr: Bytes}
proc observations() [process] -> Unit {
  let plain: Status = run true
  let status: Status = run.status true
  let text: Result[Str, ProcessError] = run.text true
  let byte_output: Result[Bytes, ProcessError] = run.bytes true
  let text_record: Result[TextObservation, ProcessError] = run.capture --text true
  let bytes_record: Result[BytesObservation, ProcessError] = run.capture --bytes true
  let text_rows: Result[Stream[Str], ProcessError] = run.stream --text true
  let bytes_rows: Result[Stream[Bytes], ProcessError] = run.stream --bytes true
  let _ = [plain, status]
  let _ = text
  let _ = byte_output
  let _ = text_record
  let _ = bytes_record
  let _ = text_rows
  let _ = bytes_rows
}
""")?
  assert checked.status.exited_with(0), checked.stderr
}

test operation_source_spawn_preserves_permissions_and_single_child_shape [fs, process, error] { |ctx|
  let accepted = check_operation_source(ctx, r"""proc owned(command: Command) [process] -> Unit {
  let direct: Result[ProcessHandle, ProcessError] = spawn run true
  let status: Result[ProcessHandle, ProcessError] = spawn run.status true
  let planned: Result[ProcessHandle, ProcessError] = spawn command
  let _ = direct
  let _ = status
  let _ = planned
}
""")?
  let permission = check_operation_source(ctx, "proc omitted() [] { spawn run true }\n")?
  let capture = check_operation_source(ctx, "proc invalid() [process] { spawn run.text true }\n")?
  let pipeline = check_operation_source(ctx, "proc invalid() [process] { spawn run true | run cat }\n")?
  assert accepted.status.exited_with(0), accepted.stderr
  assert_operation_source_rejection(permission)
  assert_operation_source_rejection(capture)
  assert_operation_source_rejection(pipeline)
  assert "check.spawn-run-kind" in capture.stderr, capture.stderr
  assert "check.spawn-run-shape" in pipeline.stderr, pipeline.stderr
}
