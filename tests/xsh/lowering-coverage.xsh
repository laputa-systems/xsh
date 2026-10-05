test test_registered_calls_lower_named_entries_through_checked_bindings { |ctx|
  let root = test.temp_dir(ctx, name: "lowering-registered")?
  let output = test.run_script(
    ctx,
    r"""let root = fp"{args[0]}"
let parsed = Path.parse_bytes(bytes: b"/tmp/parsed")?
print parsed.display()
fp"{root}/a.txt".write(data: b"a")?
fp"{root}/b.txt".write_atomic(data: b"b")?
fp"{root}/sub".mkdir(false)?
fp"{root}/sub/c.txt".write(data: b"c")?
let files = fs.files(path: root, hidden: false)?.collect()
let walked = fs.walk(hidden: false, path: root)?.collect()
print f"{files.len()} {walked.len()}"
archive.tar_create(root: root, path: fp"{root}/out.tar", entries: [p"a.txt"], overwrite: true)?
print f"{archive.tar_list(fp"{root}/out.tar")?.collect().len()}"
let found = mime.lookup_ext(ext: "txt")
print ${found != null}
let command = process.command_argv(argv: ["true"], target: "true")
print process.run(command)?.exited_with(0)
proc nested(base: Path) {
  print Path.parse_bytes(bytes: b"/tmp/nested")?.display()
  fp"{base}/d.txt".write(data: b"d")?
  print f"{fs.files(path: base)?.collect().len()}"
}
nested(root)
""",
    [root.display()],
  )?
  assert output.success, output.stderr
  assert output.stdout == """/tmp/parsed
3 5
1
true
true
/tmp/nested
5
"""
}

test test_environment_path_views_lower_named_entries_and_bound_receivers { |ctx|
  let output = test.run_script(
    ctx,
    r"""env.PATH.append(path: p"/opt/probe-a")?
let entries = env.PATH
entries.prepend(path: p"/opt/probe-b")?
print env.PATH.pop()?.display()
print ${p"/opt/probe-b" in env.PATH}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """/opt/probe-a
true
"""
}

test test_user_module_calls_lower_named_and_defaulted_entries { |ctx|
  let root = test.temp_dir(ctx, name: "lowering-module")?
  fp"{root}/helper.xsh".write_atomic(r"""##! Helper module for named argument lowering.

## Multiplies a value.
export pure scale(value: Int, by: Int = 2) -> Int { value * by }
## Counts a table.
export pure count(table: Map[Int] = {a: 1, b: 2}, extra: Int = 0) -> Int { table.len() + extra }
""")
  let output = test.run_script(
    ctx,
    r"""use helper
print f"{helper.scale(by: 5, value: 3)} {helper.scale(3, by: 4)} {helper.scale(value: 3)}"
print f"{helper.count(extra: 1)} {helper.count(table: {z: 1})}"
proc nested() {
  let options = {value: 2}
  print f"{helper.scale(...options)}"
}
nested()
""",
    [],
    {XSH_MODULE_PATH: root},
  )?
  assert output.success, output.stderr
  assert output.stdout == """15 12 6
3 1
4
"""
}

test test_top_level_guard_bindings_publish_success_values { |ctx|
  let output = test.run_script(
    ctx,
    r"""type Build = {jobs: Int, target: Str}
type Config = {root: Str, build: Build}
proc config(ok: Bool) [error] -> Result[Config] {
  if !ok { error.fail("missing")? }
  Ok({root: "src", build: {jobs: 3, target: "native"}})
}
guard let {root, build: {jobs, target: target_name, ..}, ..} = config(true) else {
  abort(3)
}
print f"{root} {jobs} {target_name}"
guard let missing = config(false) else { |failure|
  print f"fallback {failure.message}"
  abort(4)
}
print "unreachable ${missing.root}"
""",
  )?
  assert output.status == 4, output.stderr
  assert output.stdout == """src 3 native
fallback missing
"""
}

# Top-level slots take the checker's binding type: a stream run binding is a
# Stream, so a signal hook's root snapshot can read it.
test test_signal_hooks_read_top_level_stream_run_bindings { |ctx|
  let output = test.run_script(
    ctx,
    r"""let rows = run.stream --text printf "a\nb\n" ?
on USR1 [] {
  print rows.collect().len()
  abort(0)
}
run sh -c r"kill -USR1 $PPID; sleep 1" ?
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "2\n"
}

test test_map_parameter_defaults_encode_map_keys { |ctx|
  let output = test.run_script(
    ctx,
    r"""pure size(table: Map[Int] = {["first"]: 1, second: 2}) -> Int { table.len() }
print ${size()}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """2
"""
}

test test_process_command_requires_builder_block { |ctx|
  let output = test.run_script(
    ctx,
    """let command = process.command()
print process.run(command)?.exited()
""",
  )?
  assert ! output.success
  assert "check.builder-call" in output.stderr, output.stderr
  assert "compact.indexed-build" not in output.stderr, output.stderr
}

test test_members_without_a_checked_type_are_checker_errors { |ctx|
  # Each of these used to check as Unknown or Any, then failed preparation
  # with an internal encode error or ran into a runtime type error.
  for case in [
    {
      source: "let flag = false\nlet value = flag.upper()",
      code: "check.unknown-method",
    },
    {
      source: "enum Level { Low, High }\nlet level = Low\nlet value = level.upper()",
      code: "check.unknown-method",
    },
    {
      source: "let value = null.upper()",
      code: "check.unknown-method",
    },
    {
      source: "pure pick(flag: Bool) -> Str? {\n  if flag { \"x\" } else { null }\n}\nlet value = pick(false).upper()",
      code: "check.optional-method",
    },
    {
      source: "let value = p\"a/b\".missing",
      code: "check.unknown-field",
    },
    {
      source: "let value = rx\"a\".missing",
      code: "check.unknown-field",
    },
    {
      source: "error Failure = Bad(message: Str)\nlet failure = Failure.Bad(message: \"m\")\nlet value = failure.missing",
      code: "check.unknown-field",
    },
    {
      source: "let maybe: Int? = 3\nlet value = maybe?.missing",
      code: "check.field-access",
    },
    {
      source: "let value = fs.cwd",
      code: "check.module-member",
    },
    {
      source: "let value = fs.no_such_function",
      code: "check.module-member",
    },
    {
      source: "let value = system",
      code: "check.module-member",
    },
    {
      source: "let value = 99999999999999999999",
      code: "check.int-literal",
    },
    {
      source: "run echo @missing_words",
      code: "check.unresolved-name",
    },
    {
      source: "let raw = json.decode(\"1.25\")?.require(Any)?\nlet value = raw.format(precision: 2)",
      code: "check.dynamic-boundary",
    },
    {
      source: "pure helper(x: Int) -> Int { x }\nproc body() -> Int {\n  var total = 0\n  for helper in [{[1]: \"a\"}] {\n    total = helper(2)\n  }\n  total\n}\nlet value = body()",
      code: "check.call-target",
    },
    {
      source: "let value = 18446744073709551616ms",
      code: "check.duration-literal",
    },
    {
      source: "pure pick(flag: Bool) -> Int {\n  if flag {\n    1\n  } else {\n    let unused = 2\n  }\n}\nlet value = pick(false)",
      code: "check.type-mismatch",
    },
    {
      source: "pure pick(flag: Bool) -> Int {\n  match flag {\n    true => { 1 }\n    false => {}\n  }\n}\nlet value = pick(false)",
      code: "check.type-mismatch",
    },
  ] {
    let output = test.run_script(ctx, case.source + "\nprint ran\n")?
    assert ! output.success, case.source
    assert case.code in output.stderr, output.stderr
    assert "compact.indexed-build" not in output.stderr, output.stderr
    assert output.stdout == "", case.source
  }
}

test test_nested_declarations_are_checker_errors { |ctx|
  # Declarations inside a body or block bind nothing callers can resolve;
  # the checker used to accept them and preparation then failed with an
  # internal indexed-IR encode error.
  for declaration in [
    "use json",
    "export let shared = 3",
    "error Failure = Bad(message: Str)",
    "stream items() -> Stream[Int] { yield 1 }",
    "type Point = {x: Int}",
    "enum Level { Low, High }",
    "pure helper() -> Int { 1 }",
    "proc step() -> Int { 1 }",
  ] {
    for lines in [
      ["proc body() -> Int {", "  " + declaration, "  7", "}", r"print ${body()}"],
      ["if true {", "  " + declaration, "}", "print done"],
    ] {
      let source = lines.join("\n") + "\n"
      let output = test.run_script(ctx, source)?
      assert ! output.success, source
      assert "check.nested-declaration" in output.stderr, output.stderr
      assert "compact.indexed-build" not in output.stderr, output.stderr
    }
  }

  let local = test.run_script(
    ctx,
    r"""proc body() -> Int {
  const limit = 7
  limit
}
print ${body()}
""",
  )?
  assert local.success, local.stderr
  assert local.stdout == "7\n"
}
