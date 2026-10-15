pure route_self_count(value: Int) -> Int {
  if value == 0 { 0 } else { route_self_count(value - 1) + 1 }
}

pure route_checked(value: UInt) -> UInt { value }

enum RouteOracleTagMatch { Ready, Busy }

proc route_oracle_match_tag() -> RouteOracleTagMatch { return Ready }

test test_expr_comparison_chain { |ctx|
  assert 1 < 2 < 3
  test.expect(ctx, r"""let first = 1
let second = 2
let third = 3
let selected = first < second < third
assert selected == true
""", status: 0)?
}

test test_expr_binary { |ctx|
  test.expect(ctx, r"""assert "route " + "binary" == "route binary"
""", status: 0)?
}

test test_expr_if { |ctx|
  assert (if true { "selected" } else { "skipped" }) == "selected"
  test.expect(ctx, r"""let selected = if true { "selected" } else { "skipped" }
assert selected == "selected"
""", status: 0)?
}

test test_expr_if_non_in_place_selection { |ctx|
  let output = test.expect(
    ctx,
    """
pure route_if_condition() -> Bool { true }
assert (if route_if_condition() { "selected" } else { "skipped" }) == "selected"
""",
    status: 0,
  )?
  assert output.stdout == ""
}

test test_expr_pattern_if { |ctx|
  test.expect(
    ctx,
    r"""let result = Ok(7)
let selected = if let Ok(value) = result { value } else { 0 }
assert selected == 7
""",
    status: 0,
  )?
}

test test_expr_match { |ctx|
  let output = test.expect(
    ctx,
    """
proc route_match_source() -> Int { return 2 }
assert (match route_match_source() { 2 => 22, _ => 0 }) == 22
""",
    status: 0,
  )?
  assert output.stdout == ""
}

test test_expr_match_pure_selection { |ctx|
  let output = test.expect(
    ctx,
    """
assert (match 2 { 2 => 22, _ => 0 }) == 22
""",
    status: 0,
  )?
  assert output.stdout == ""
}

test test_expr_str_match { |ctx|
  test.expect(
    ctx,
    r"""let word = "green"
let selected = match word {
  "green" => "go"
  else => "stop"
}
assert selected == "go"
""",
    status: 0,
  )?
}

test test_expr_tag_match { |ctx|
  let tag = route_oracle_match_tag()
  let selected = match tag {
    Ready => "ready"
    Busy => "busy"
  }
  assert selected == "ready"
  test.expect(
    ctx,
    r"""enum RouteTag { Ready, Busy }
proc route_match_tag() -> RouteTag { return Ready }
let tag = route_match_tag()
let selected = match tag {
  Ready() => "ready"
  Busy() => "busy"
}
assert selected == "ready"
""",
    status: 0,
  )?
}

test test_expr_result_fallback { |ctx|
  test.expect(
    ctx,
    r"""error RouteOracleError = Failed
let missing: Result[Int, RouteOracleError] = Err(.Failed("missing"))
assert (missing ?? 9) == 9
""",
    status: 0,
  )?
}

test test_expr_fmt_string { |ctx|
  test.expect(
    ctx,
    r"""let word = "formatted"
assert f"route {word}" == "route formatted"
""",
    status: 0,
  )?
}

test test_expr_path_fmt_string { |ctx|
  test.expect(
    ctx,
    r"""let name = "entry"
assert fp"root/{name}" == p"root/entry"
""",
    status: 0,
  )?
}

test test_expr_record { |ctx|
  test.expect(
    ctx,
    r"""let entry = {name: "route", count: 2}
assert entry.name == "route"
assert entry.count == 2
""",
    status: 0,
  )?
}

test test_expr_map_literal { |ctx|
  test.expect(
    ctx,
    r"""let entries: Map[Int, Str] = {[1]: "one", [2]: "two"}
assert entries.get(2)? == "two"
""",
    status: 0,
  )?
}

test test_expr_record_update { |ctx|
  let original = {name: "before", count: 1}
  let changed = {...original, count: 2}
  assert changed.name == "before"
  assert changed.count == 2
  test.expect(
    ctx,
    r"""type RouteDetails = {count: Int}
type RouteRecord = {name: Str, details: RouteDetails}
proc route_original_record() -> RouteRecord { return {name: "before", details: {count: 1}} }
let original = route_original_record()
let changed = {...original, details.count: 2}
assert changed.name == "before"
assert changed.details.count == 2
""",
    status: 0,
  )?
}

test test_expr_list_comp { |ctx|
  test.expect(
    ctx,
    r"""let doubled = [value * 2 for value in [1, 2, 3]]
assert doubled == [2, 4, 6]
""",
    status: 0,
  )?
}

test test_expr_map_comp { |ctx|
  test.expect(
    ctx,
    r"""let entries = {[word]: word for word in ["a", "b"]}
assert entries.get("b")? == "b"
""",
    status: 0,
  )?
}

test test_expr_method { |ctx|
  test.expect(ctx, r"""let parsed = "17".parse_int()
assert parsed? == 17
""", status: 0)?
}

test test_expr_require { |ctx|
  test.expect(ctx, "let raw: Any = 17\nassert raw.require(Int)? == 17\n", status: 0)?
}

test test_expr_checked_value { |ctx|
  assert route_checked(17) == 17
  test.expect(
    ctx,
    "let raw: UInt = 17\nlet checked = raw\nassert checked == 17\n",
    status: 0,
  )?
}

test test_expr_run_capture { |ctx|
  test.expect(
    ctx,
    "let output = run.capture --text printf \"captured\"\nassert output.stdout == \"captured\"\n",
    status: 0,
  )?
}

test test_expr_run_pipeline { |ctx|
  test.expect(
    ctx,
    "let output = run.capture --text printf \"piped\" | run cat\nassert output.stdout == \"piped\"\n",
    status: 0,
  )?
}

test test_expr_spawn_run { |ctx|
  test.expect(ctx, "let child = spawn run true?\nassert (wait child?).exited_with(0)\n", status: 0)?
}

test test_expr_spawn_command { |ctx|
  test.expect(
    ctx,
    "let command = process.command_argv(\"true\", [\"true\"])\nlet child = spawn command?\nassert (wait child?).exited_with(0)\n",
    status: 0,
  )?
}

test test_expr_wait { |ctx|
  test.expect(
    ctx,
    "let child = spawn run true?\nlet status = wait child?\nassert status.exited_with(0)\n",
    status: 0,
  )?
}

test test_expr_capture { |ctx|
  test.expect(
    ctx,
    "let parsed = \"21\".parse_int()\nlet result = try { parsed? }\nassert result == Ok(21)\n",
    status: 0,
  )?
}

test test_expr_value_block { |ctx|
  test.expect(ctx, "let value = { 20 + 1 }\nassert value == 21\n", status: 0)?
}

test test_expr_error_context { |ctx|
  test.expect(ctx, "let value = ctx \"route oracle\" { 21 }\nassert value == 21\n", status: 0)?
}

test test_expr_context_scope { |ctx|
  test.expect(ctx, "assert within 1s { 21 } == Ok(21)\n", status: 0)?
}

test test_expr_loop { |ctx|
  test.expect(ctx, "let value = loop { break 21 }\nassert value == 21\n", status: 0)?
}

test test_expr_retry { |ctx|
  test.expect(ctx, "let value = retry [] { 21 }?\nassert value == 21\n", status: 0)?
}

test test_expr_module_call { |ctx|
  test.expect(
    ctx,
    "type RouteJson = {value: Int}\nassert json.decode(\"{\\\"value\\\":21}\")?.require(RouteJson)?.value == 21\n",
    status: 0,
  )?
}

test test_expr_process_command_argv { |ctx|
  test.expect(
    ctx,
    "let command = process.command_argv(\"true\", [\"true\"])\nassert process.run(command)?.exited_with(0)\n",
    status: 0,
  )?
}

test test_expr_process_command_builder { |ctx|
  test.expect(ctx, "let command = process.command { run true }\nassert process.run(command)?.exited_with(0)\n", status: 0)?
}

test test_expr_ok { |ctx|
  let result = Ok(21)
  assert result == Ok(21)
  test.expect(ctx, "let result = Ok(21)\nassert result is Ok(_)\n", status: 0)?
}

test test_expr_err { |ctx|
  test.expect(
    ctx,
    "error RouteOracleError = Failed\nlet result: Result[Int, RouteOracleError] = Err(.Failed(\"failed\"))\nassert result is Err(_)\n",
    status: 0,
  )?
}

test test_expr_try { |ctx|
  test.expect(ctx, "let parsed = \"21\".parse_int()\nassert parsed? == 21\n", status: 0)?
}

test test_expr_call { |ctx|
  test.expect(ctx, "proc route_proc_target(value: Int) -> Int { value + 1 }\nassert route_proc_target(20) == 21\n", status: 0)?
}

test test_expr_direct_pure_call { |ctx|
  test.expect(ctx, "pure route_direct_scale(value: Int) -> Int { value * 3 }\nassert route_direct_scale(7) == 21\n", status: 0)?
}

test test_expr_external_call { |ctx|
  test.expect(
    ctx,
    r"""type RouteRenderPlugin = module {export pure render(data: Any) -> Result[Str, Error]}
let root = fs.tempdir()?
defer root.close()?
let plugin_file = fp"{root.host_path()?}/plugin.xsh"
plugin_file.write("##! Expression route plugin.\n## Renders its input.\nexport pure render(data: Any) -> Result[Str, Error] { template.render(\"{{.value}}\", data) }\n")
let plugin = module.load(plugin_file)?.require(RouteRenderPlugin)?
assert plugin.render({value: "external"})? == "external"
""",
    status: 0,
  )?
}

test test_expr_dynamic_call { |ctx|
  test.expect(
    ctx,
    "type RouteScale = pure(value: Int) -> Int\npure route_double(value: Int) -> Int { value * 2 }\nlet scale: RouteScale = route_double\nlet handle: Pure = scale\nassert handle.call(10).require(Int)? == 20\n",
    status: 0,
  )?
}

test test_expr_self_call { |ctx|
  assert route_self_count(4) == 4
  test.expect(
    ctx,
    "pure route_self_count(value: Int) -> Int { if value == 0 { 0 } else { route_self_count(value - 1) + 1 } }\nassert route_self_count(1) == 1\n",
    status: 0,
  )?
}

test test_expr_typed_call { |ctx|
  test.expect(
    ctx,
    "type RouteScale = pure(value: Int) -> Int\npure route_direct_scale(value: Int) -> Int { value * 3 }\nlet scale: RouteScale = route_direct_scale\nassert scale(7) == 21\n",
    status: 0,
  )?
}

test test_expr_proc_call_result { |ctx|
  test.expect(
    ctx,
    "proc route_proc_target(value: Int) -> Int { value + 1 }\nlet handle: Proc = route_proc_target\nassert handle.call(20)?.require(Int)? == 21\n",
    status: 0,
  )?
}

test test_expr_function_ref { |ctx|
  test.expect(
    ctx,
    "pure increment(value: Int) -> Int { value + 1 }\nlet alias = increment\nassert alias(41) == 42\n",
    status: 0,
  )?
}

test test_expr_field { |ctx|
  test.expect(
    ctx,
    "type Item = {value: Int}\npure read_item(item: Item) -> Int { item.value }\nassert read_item({value: 41}) == 41\n",
    status: 0,
  )?
}

test test_expr_abort { |ctx|
  test.expect(
    ctx,
    "proc terminate() { exit 23 }\nterminate()\n",
    status: 23,
  )?
}

test test_expr_archive_tar_create { |ctx|
  let root = test.temp_dir(ctx, name: "route-archive-create")?
  fp"{root}/member".write("payload")
  let archive_file = fp"{root}/payload.tar"
  test.expect(
    ctx,
    "let root = Path(args[0])\narchive.tar_create(fp\"{root}/payload.tar\", root, [p\"member\"])\nassert fp\"{root}/payload.tar\".exists()?\n",
    status: 0,
    args: [root],
  )?
  assert archive_file.exists()?
}

test test_expr_error { |ctx|
  test.expect(
    ctx,
    "error RouteError = Failed(message: Str)\nlet result: Result[Int, RouteError] = Err(RouteError.Failed(message: \"failed\"))\nassert result is Err(_)\n",
    status: 0,
  )?
}

test test_expr_fail { |ctx|
  test.expect(
    ctx,
    "let result = error.fail(\"expected failure\")\nmatch result { Err(problem) => assert problem.message == \"expected failure\"; Ok(_) => assert false }\n",
    status: 0,
  )?
}

test test_expr_fs_files { |ctx|
  let root = test.temp_dir(ctx, name: "route-fs-files")?
  fp"{root}/visible.txt".write("text")
  fp"{root}/ignored.bin".write("bytes")
  test.expect(
    ctx,
    "let root = Path(args[0])\nlet files = fs.files(root, gitignore: false, stat: false, hidden: false, exts: [\"txt\"])?.collect()\nassert files.len() == 1 and files[0].name == \"visible.txt\"\n",
    status: 0,
    args: [root],
  )?
}

test test_expr_fs_list { |ctx|
  let root = test.temp_dir(ctx, name: "route-fs-list")?
  fp"{root}/entry".write("payload")
  test.expect(
    ctx,
    "let root = Path(args[0])\nlet children = fs.children(root, ordered: true)?.collect()\nassert children.len() == 1 and children[0].name == \"entry\"\n",
    status: 0,
    args: [root],
  )?
}

test test_expr_fs_mkdir { |ctx|
  let root = test.temp_dir(ctx, name: "route-fs-mkdir")?
  let target = fp"{root}/scoped"
  test.expect(
    ctx,
    "let directory = Path(args[0])\ntempdir scratch at directory { assert scratch.exists()? }\nassert !directory.exists()?\n",
    status: 0,
    args: [target],
  )?
  assert ! target.exists()?
}

test test_expr_fs_remove { |ctx|
  let root = test.temp_dir(ctx, name: "route-fs-remove")?
  let file = fp"{root}/blocker"
  file.write("not a directory")
  let target = fp"{file}/child"
  test.expect(
    ctx,
    "let target = Path(args[0])\nlet result = tempdir scratch at target { assert false }\ntest.error_kind(result, \"fs-remove\")\n",
    status: 0,
    args: [target],
  )?
  assert file.read_text()? == "not a directory"
}

test test_expr_fs_temp_dir { |ctx|
  test.expect(
    ctx,
    "let root = fs.tempdir()?\ndefer root.close()?\nlet host_path = root.host_path()?\nassert host_path.exists()?\n",
    status: 0,
  )?
}

test test_expr_fs_walk { |ctx|
  let root = test.temp_dir(ctx, name: "route-fs-walk")?
  fp"{root}/entry".write("payload")
  test.expect(
    ctx,
    "let root = Path(args[0])\nlet walked = fs.walk(root, gitignore: false, stat: false, hidden: false)?.collect()\nassert walked.len() == 2\nassert walked |> any .name == \"entry\"\n",
    status: 0,
    args: [root],
  )?
}

test test_expr_glob { |ctx|
  let root = test.temp_dir(ctx, name: "route-glob")?
  fp"{root}/match.txt".write("payload")
  test.expect(
    ctx,
    "let root = Path(args[0])\ncd root { let matches = g\"*.txt\"; assert matches == [p\"match.txt\"] }\n",
    status: 0,
    args: [root],
  )?
}

test test_expr_index { |ctx|
  test.expect(
    ctx,
    "proc selected_index() [io] -> Int { return 1 }\nlet values = [10, 20]\nassert values[selected_index()] == 20\n",
    status: 0,
  )?
}

test test_expr_index_from_end { |ctx|
  test.expect(
    ctx,
    "proc last(values: List[Str]) -> Str { return values[-1] }\nassert last([\"first\", \"last\"]) == \"last\"\n",
    status: 0,
  )?
}

test test_expr_last_status { |ctx|
  test.expect(
    ctx,
    "run.status false\nlet status = $?\nassert status.exited_with(1)\n",
    status: 0,
  )?
}

test test_expr_list { |ctx|
  test.expect(
    ctx,
    "let values = [10, 20]\nassert values.len() == 2 and values[0] == 10 and values[1] == 20\n",
    status: 0,
  )?
}

test test_expr_list_build { |ctx|
  test.expect(
    ctx,
    "proc middle() -> List[Int] { return [2, 3] }\nlet values = [1, @middle(), 4]\nassert values.len() == 4 and values[1] == 2 and values[2] == 3\n",
    status: 0,
  )?
}

test test_expr_path_du { |ctx|
  let root = test.temp_dir(ctx, name: "route-path-du")?
  fp"{root}/entry".write("payload")
  test.expect(
    ctx,
    "let target = Path(args[0])\nassert target.du()? >= 0\n",
    status: 0,
    args: [root],
  )?
}

test test_expr_path_executable { |ctx|
  test.expect(
    ctx,
    "let executable = Path(args[0])\nassert executable.executable()?\n",
    status: 0,
    args: [ctx.xsh_bin],
  )?
}

test test_expr_path_exists { |ctx|
  let root = test.temp_dir(ctx, name: "route-path-exists")?
  let file = fp"{root}/entry"
  file.write("payload")
  test.expect(
    ctx,
    "let target = Path(args[0])\nassert target.exists()?\n",
    status: 0,
    args: [file],
  )?
}

test test_expr_path_from { |ctx|
  test.expect(
    ctx,
    "let candidate = Path(\"folder/file\")\nassert candidate == p\"folder/file\"\n",
    status: 0,
  )?
}

test test_expr_path_metadata { |ctx|
  let root = test.temp_dir(ctx, name: "route-path-metadata")?
  let file = fp"{root}/entry"
  file.write("payload")
  test.expect(
    ctx,
    "let target = Path(args[0])\nassert target.metadata()?.size == 7\n",
    status: 0,
    args: [file],
  )?
}

test test_expr_path_mkdir { |ctx|
  let root = test.temp_dir(ctx, name: "route-path-mkdir")?
  let target = fp"{root}/created"
  test.expect(
    ctx,
    "let target = Path(args[0])\ntarget.mkdir()?\nassert target.exists()?\n",
    status: 0,
    args: [target],
  )?
  assert target.exists()?
}

test test_expr_path_read_bytes { |ctx|
  let root = test.temp_dir(ctx, name: "route-path-read-bytes")?
  let file = fp"{root}/entry"
  file.write(b"raw\0bytes")
  test.expect(
    ctx,
    "let target = Path(args[0])\nassert target.read_bytes()? == b\"raw\\0bytes\"\n",
    status: 0,
    args: [file],
  )?
}

test test_expr_path_read_text { |ctx|
  let root = test.temp_dir(ctx, name: "route-path-read-text")?
  let file = fp"{root}/entry"
  file.write("payload")
  test.expect(
    ctx,
    "let target = Path(args[0])\nassert target.read_text()? == \"payload\"\n",
    status: 0,
    args: [file],
  )?
}

test test_expr_path_readlink { |ctx|
  let root = test.temp_dir(ctx, name: "route-path-readlink")?
  fp"{root}/target".write("payload")
  fp"{root}/link".symlink(to: p"target")
  test.expect(
    ctx,
    "let link = Path(args[0])\nassert link.readlink()? == p\"target\"\n",
    status: 0,
    args: [fp"{root}/link"],
  )?
}

test test_expr_path_remove { |ctx|
  let root = test.temp_dir(ctx, name: "route-path-remove")?
  let file = fp"{root}/entry"
  file.write("payload")
  test.expect(
    ctx,
    "let target = Path(args[0])\ntarget.remove(missing_ok: false)?\nassert !target.exists()?\n",
    status: 0,
    args: [file],
  )?
  assert ! file.exists()?
}

test test_expr_path_resolve { |ctx|
  let root = test.temp_dir(ctx, name: "route-path-resolve")?
  let file = fp"{root}/entry"
  file.write("payload")
  test.expect(
    ctx,
    "let target = Path(args[0])\nassert target.resolve()?.name() == \"entry\"\n",
    status: 0,
    args: [file],
  )?
}

test test_expr_path_write { |ctx|
  let root = test.temp_dir(ctx, name: "route-path-write")?
  let file = fp"{root}/entry"
  test.expect(
    ctx,
    "let target = Path(args[0])\ntarget.write(\"payload\")?\nassert target.read_text()? == \"payload\"\n",
    status: 0,
    args: [file],
  )?
  assert file.read_text()? == "payload"
}

test test_expr_pipeline { |ctx|
  test.expect(
    ctx,
    "let values = [1, 2, 3] |> map { |value| value * 2 }\nassert values.len() == 3 and values[1] == 4\n",
    status: 0,
  )?
}

test test_expr_range { |ctx|
  test.expect(
    ctx,
    "let values = range(2, 5).collect()\nassert values == [2, 3, 4]\n",
    status: 0,
  )?
}

test test_expr_slice { |ctx|
  test.expect(
    ctx,
    "let values = [10, 20, 30, 40]\nlet selected = values[1..3]\nassert selected.len() == 2 and selected[0] == 20 and selected[1] == 30\n",
    status: 0,
  )?
}

test test_expr_str_byte_at { |ctx|
  test.expect(
    ctx,
    "assert \"é\".byte_at(1) == 169\n",
    status: 0,
  )?
}

test test_expr_str_byte_len { |ctx|
  test.expect(
    ctx,
    "assert \"é\".byte_len() == 2\n",
    status: 0,
  )?
}

test test_expr_str_predicate { |ctx|
  test.expect(
    ctx,
    "assert \"alpha\".starts_with(\"al\")\n",
    status: 0,
  )?
}

test test_expr_tag { |ctx|
  test.expect(
    ctx,
    "enum Choice { Selected(Int) }\nlet selected: Choice = Selected(42)\nmatch selected { Selected(value) => assert value == 42 }\n",
    status: 0,
  )?
}
