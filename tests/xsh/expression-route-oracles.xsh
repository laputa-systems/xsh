type RouteScale = pure(value: Int) -> Int
type RouteJson = {value: Int}
type RouteRenderPlugin = module {export pure render(data: Any) -> Result[Str, Error]}
enum RouteTag { Ready, Busy }
error RouteOracleError = Failed

pure route_direct_scale(value: Int) -> Int { value * 3 }

pure route_self_count(value: Int) -> Int {
  if value == 0 { 0 } else { route_self_count(value - 1) + 1 }
}

pure route_checked(value: UInt) -> UInt { value }

pure route_double(value: Int) -> Int { value * 2 }

proc route_proc_target(value: Int) -> Int { value + 1 }

test test_expr_comparison_chain {
  assert 1 < 2 < 3
}

test test_expr_binary {
  assert "route " + "binary" == "route binary"
}

test test_expr_if {
  assert (if true { "selected" } else { "skipped" }) == "selected"
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

test test_expr_pattern_if {
  let result = Ok(7)
  let selected = if let Ok(value) = result { value } else { 0 }
  assert selected == 7
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

test test_expr_str_match {
  let word = "green"
  let selected = match word {
    "green" => "go"
    else => "stop"
  }
  assert selected == "go"
}

test test_expr_tag_match {
  let tag: RouteTag = Ready
  let selected = match tag {
    Ready => "ready"
    Busy => "busy"
  }
  assert selected == "ready"
}

test test_expr_result_fallback {
  let missing: Result[Int, RouteOracleError] = Err(.Failed("missing"))
  assert (missing ?? 9) == 9
}

test test_expr_fmt_string {
  let word = "formatted"
  assert f"route {word}" == "route formatted"
}

test test_expr_path_fmt_string {
  let name = "entry"
  assert fp"root/{name}" == p"root/entry"
}

test test_expr_record {
  let entry = {name: "route", count: 2}
  assert entry.name == "route"
  assert entry.count == 2
}

test test_expr_map_literal {
  let entries: Map[Int, Str] = {[1]: "one", [2]: "two"}
  assert entries.get(2)? == "two"
}

test test_expr_record_update {
  let original = {name: "before", count: 1}
  let changed = {...original, count: 2}
  assert changed.name == "before"
  assert changed.count == 2
}

test test_expr_list_comp {
  let doubled = [value * 2 for value in [1, 2, 3]]
  assert doubled == [2, 4, 6]
}

test test_expr_map_comp {
  let entries = {[word]: word for word in ["a", "b"]}
  assert entries.get("b")? == "b"
}

test test_expr_method {
  let parsed = "17".parse_int()
  assert parsed? == 17
}

test test_expr_require {
  let raw: Any = 17
  assert raw.require(Int)? == 17
}

test test_expr_checked_value {
  assert route_checked(17) == 17
}

test test_expr_run_capture {
  let output = run.capture --text printf "captured"
  assert output.stdout == "captured"
}

test test_expr_run_pipeline {
  let output = run.capture --text printf "piped" | run cat
  assert output.stdout == "piped"
}

test test_expr_spawn_run {
  let child = spawn run true?
  assert (wait child?).exited_with(0)
}

test test_expr_spawn_command {
  let command = process.command_argv("true", ["true"])
  let child = spawn command?
  assert (wait child?).exited_with(0)
}

test test_expr_wait {
  let child = spawn run true?
  let status = wait child?
  assert status.exited_with(0)
}

test test_expr_capture {
  let parsed = "21".parse_int()
  let result = try { parsed? }
  assert result == Ok(21)
}

test test_expr_value_block {
  let value = { 20 + 1 }
  assert value == 21
}

test test_expr_error_context {
  let value = ctx "route oracle" { 21 }
  assert value == 21
}

test test_expr_context_scope {
  assert within 1s { 21 } == Ok(21)
}

test test_expr_loop {
  let value = loop { break 21 }
  assert value == 21
}

test test_expr_retry {
  let value = retry [] { 21 }?
  assert value == 21
}

test test_expr_module_call {
  assert json.decode("{\"value\":21}")?.require(RouteJson)?.value == 21
}

test test_expr_process_command_argv {
  let command = process.command_argv("true", ["true"])
  assert process.run(command)?.exited_with(0)
}

test test_expr_process_command_builder {
  let command = process.command { run true }
  assert process.run(command)?.exited_with(0)
}

test test_expr_ok {
  let result = Ok(21)
  assert result == Ok(21)
}

test test_expr_err {
  let result: Result[Int, RouteOracleError] = Err(.Failed("failed"))
  assert result is Err(_)
}

test test_expr_try {
  let parsed = "21".parse_int()
  assert parsed? == 21
}

test test_expr_call {
  assert route_proc_target(20) == 21
}

test test_expr_direct_pure_call {
  assert route_direct_scale(7) == 21
}

test test_expr_external_call { |ctx|
  let root = test.temp_dir(ctx, name: "expression-external-call")?
  let plugin_file = fp"{root}/plugin.xsh"
  plugin_file.write("""##! Expression route plugin.
## Renders its input.
export pure render(data: Any) -> Result[Str, Error] {
  template.render("{{.value}}", data)
}
""")
  let plugin = module.load(plugin_file)?.require(RouteRenderPlugin)?
  assert plugin.render({value: "external"})? == "external"
}

test test_expr_dynamic_call {
  let scale: RouteScale = route_double
  let handle: Pure = scale
  assert handle.call(10).require(Int)? == 20
}

test test_expr_self_call {
  assert route_self_count(4) == 4
}

test test_expr_typed_call {
  let scale: RouteScale = route_direct_scale
  assert scale(7) == 21
}

test test_expr_proc_call_result {
  let handle: Proc = route_proc_target
  assert handle.call(20)?.require(Int)? == 21
}
