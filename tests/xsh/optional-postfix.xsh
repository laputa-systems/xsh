error OptionalPostfixError = Failed : InvalidData

type OptionalPostfixServer = {host: Str?}

type OptionalPostfixConfig = {server: OptionalPostfixServer?}

type OptionalPostfixObservation = {target: Path?, state: Str}

proc optional_postfix_observation(present: Bool) [] -> OptionalPostfixObservation {
  return {target: /dev/example, state: "observed"} when present

  {target: null, state: "observed"}
}

test test_optional_record_return_field_alias_preserves_receiver_type {
  for present in [true, false] {
    let source = optional_postfix_observation(present)
    let target = source.target
    let displayed = target?.display() ?? ""
    assert displayed == (if present { "/dev/example" } else { "" })
  }
}

test test_optional_method_skips_arguments_and_preserves_fallback {
  let absent: Str? = null
  assert (absent?.trim() ?? "default") == "default"
  assert (absent?.replace("x", with: "y") ?? "default") == "default"
  let present: Str? = "  label  "
  assert (present?.trim() ?? "default") == "label"
}

test test_optional_index_and_slice {
  let absent: List[Int]? = null
  assert (absent?[0] ?? -1) == -1
  assert (absent?[0..2] ?? []) == []
  let present: List[Int]? = [1, 2, 3]
  assert (present?[1] ?? -1) == 2
  assert (present?[1..] ?? []) == [2, 3]
  let text: Str? = "αβγ"
  assert (text?[1..2] ?? "") == "β"
}

test test_optional_postfix_evaluation_order { |ctx|
  let output = test.run_script(
    ctx,
    """
proc absent() [io] -> Str? { print "receiver"; return null }
proc present() [io] -> Str? { print "present"; return "x" }
proc argument() [io] -> Str { print "argument"; return "x" }
proc bound(value: Int) [io] -> Int { print $value; return value }
proc values() [io] -> List[Int]? { print "list"; return [1, 2, 3] }
print (absent()?.replace(from: argument(), with: argument()) ?? "absent")
print (present()?.replace(from: argument(), with: argument()) ?? "absent")
let empty: List[Int]? = null
print (empty?[bound(0)] ?? -1)
let skipped = (empty?[bound(0)..bound(2)] ?? []).len()
print $skipped
let reached = (values()?[bound(1)..bound(3)] ?? []).len()
print $reached
""",
    [],
    {},
    b"",
    "optional-order.xsh",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """receiver
absent
present
argument
argument
x
-1
0
list
1
3
2
"""
}

test test_optional_result_layers_and_outer_propagation {
  let present: Str? = "42"
  let absent: Str? = null
  assert (present?.parse_int() ?? Ok(0))? == 42
  assert (absent?.parse_int() ?? Ok(0))? == 0
  let bad: Str? = "bad"
  test.error_kind(bad?.parse_int() ?? Ok(0), "parse-int")
  let wrapped = Ok([3, 4, 5])
  assert wrapped?[1] == 4
  assert wrapped?[1..] == [4, 5]
  let nested: Result[Str?] = Ok(" label ")
  assert ((nested?)?.trim() ?? "default") == "label"
}

test test_optional_non_null_errors_and_explicit_hops { |ctx|
  let empty = test.run_script(
    ctx,
    """let values: List[Int]? = []
let item = values?[0] ?? 0
print $item
""",
    [],
    {},
    b"",
    "optional-empty.xsh",
  )?
  assert ! empty.success
  {
    let assertion_condition = "index" in empty.stderr
    let assertion_message = empty.stderr
    assert assertion_condition, assertion_message
  }
  let any = test.run_script(
    ctx,
    """let value: Any = null
print value?.trim()
""",
    [],
    {},
    b"",
    "optional-any.xsh",
  )?
  assert ! any.success
  {
    let assertion_condition = "check.null-safe-field" in any.stderr
    let assertion_message = any.stderr
    assert assertion_condition, assertion_message
  }
  let mixed = test.run_script(
    ctx,
    """let value: Str? = null
let wrapped = value?.parse_int()
print wrapped?
""",
    [],
    {},
    b"",
    "optional-mixed.xsh",
  )?
  assert ! mixed.success
  {
    let assertion_condition = "check.try-result" in mixed.stderr
    let assertion_message = mixed.stderr
    assert assertion_condition, assertion_message
  }
}

test test_optional_postfix_requires_explicit_hops_error_effects_and_bool_values { |ctx|
  let ordinary_hop = test.run_script(
    ctx,
    r"""type Server = {host: Str?}
type Config = {server: Server?}
let config: Config? = {server: {host: "label"}}
print (config?.server.host ?? "default")
""",
  )?
  assert ! ordinary_hop.success, ordinary_hop.stderr
  assert "check.field-access" in ordinary_hop.stderr
  let restricted = test.run_script(
    ctx,
    r"""proc first(values: Result[List[Int]]) [] -> Int {
  return values?[0]
}
""",
  )?
  assert ! restricted.success, restricted.stderr
  assert "check.effect-violation" in restricted.stderr
  let statement = test.expect(
    ctx,
    r"""let name: Str? = "abc"
name?.starts_with("x")
print "after"
""",
    status: 2,
  )?
  assert statement.stdout == ""
  assert "check.ignored-result" in statement.stderr, statement.stderr
  assert "check.bool-statement" not in statement.stderr, statement.stderr
}

test test_result_postfix_propagation_skips_index_on_failure { |ctx|
  let output = test.run_script(
    ctx,
    """
error InputError = Failed(message: Str) : InvalidData
proc argument() [io] -> Int { print "index"; return 0 }
proc read() [io, error] -> Result[Int] {
  let input: Result[List[Int]] = Err(InputError.Failed("failed"))
  return input?[argument()]
}
match read() {
  Ok(_) => print "unexpected"
  Err(_) => print "failed"
}
""",
    [],
    {},
    b"",
    "result-index-failure.xsh",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """failed
"""
}

test test_optional_fields_guard_each_hop_and_flatten_null_layers {
  let absent: OptionalPostfixConfig? = null
  let missing_server: OptionalPostfixConfig? = {server: null}
  let missing_host: OptionalPostfixConfig? = {server: {host: null}}
  let present: OptionalPostfixConfig? = {server: {host: " label "}}
  assert (absent?.server?.host?.trim() ?? "default") == "default"
  assert (missing_server?.server?.host?.trim() ?? "default") == "default"
  assert (missing_host?.server?.host?.trim() ?? "default") == "default"
  assert (present?.server?.host?.trim() ?? "default") == "label"
  let name: Str? = null
  let selected: Bool? = name?.starts_with("x")
  assert (selected ?? false) == false
}

test test_optional_postfix_null_branch_differential_witness { |ctx|
  let before = test.run_script(
    ctx,
    """
proc fallback() [io] -> Str { print "fallback"; return "default" }
proc argument() [io] -> Str { print "argument"; return "x" }
proc label(name: Str?) [io] -> Str {
  return if name == null { fallback() } else { name.replace(argument(), with: "y") }
}
print label(null)
print label("x")
""",
    [],
    {},
    b"",
    "optional-before.xsh",
  )?
  let after = test.run_script(
    ctx,
    """
proc fallback() [io] -> Str { print "fallback"; return "default" }
proc argument() [io] -> Str { print "argument"; return "x" }
proc label(name: Str?) [io] -> Str {
  return name?.replace(argument(), with: "y") ?? fallback()
}
print label(null)
print label("x")
""",
    [],
    {},
    b"",
    "optional-after.xsh",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = before
    assert assertion_condition, assertion_message
  }
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = after
    assert assertion_condition, assertion_message
  }
  assert after.stdout == before.stdout
  assert after.stdout == """fallback
default
argument
y
"""
}

test test_optional_runtime_record_fields_preserve_result_layers {
  let failure: Error? = OptionalPostfixError.Failed("42")
  assert (failure?.message?.parse_int() ?? Ok(0))? == 42
  let absent: Error? = null
  assert (absent?.message?.parse_int() ?? Ok(0))? == 0
  let handle: ProcessHandle? = null
  assert (handle?.command?.parse_int() ?? Ok(0))? == 0
}

test test_optional_method_retains_validated_local_receiver_type {
  let rows = json.decode("""[{"sched":"  noop  "},{"sched":null}]""")?
  var labels = []
  for row in rows.require(List[Record])? {
    let scheduler = json.get(row, ["sched"])?.require(Str?)?
    labels += [scheduler?.trim() ?? "absent"]
  }

  assert labels == ["noop", "absent"]
}
