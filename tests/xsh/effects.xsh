type CheckResult = {ok: Bool, out: Str}

proc run_check(src: Path) [fs, process, error] -> Result[CheckResult] {
  let err = fp"{src}.err"
  let status: Status = run.status "xsht" check $src 2> $err
  let out = err.read_text()?
  {ok: status.exited_with(0), out}
}

proc run_lint(src: Path) [fs, process] -> Result[Str] {
  let err = fp"{src}.err"
  run.status "xsht" lint $src 2> $err
  err.read_text()
}

test test_module_call_blocked_by_annotation { |ctx|
  let src = test.temp_file(ctx, name: "t.xsh", contents: b"proc bad() [fs] {\n  dns.lookup(\"g.com\")\n}\n")?
  let result = run_check(src)?
  assert ! result.ok, "expected check failure"
  assert "check.effect-violation" in result.out
  assert "net" in result.out
}

test test_correct_annotation_passes { |ctx|
  let src = test.temp_file(
    ctx,
    name: "t.xsh",
    contents: b"proc good() [net, error] {\n  let _ = dns.lookup(\"g.com\")?\n}\n",
  )?

  let result = run_check(src)?
  assert result.ok, "expected clean check"
}

test test_print_requires_no_effect { |ctx|
  let src = test.temp_file(
    ctx,
    name: "t.xsh",
    contents: b"proc good() [fs] { print \"ok\"; eprint \"warn\" }\n",
  )?
  let result = run_check(src)?
  assert result.ok, "print and eprint should require no effect"
}

test test_io_covers_net { |ctx|
  let src = test.temp_file(
    ctx,
    name: "t.xsh",
    contents: b"proc good() [io, error] {\n  let _ = dns.lookup(\"g.com\")?\n}\n",
  )?

  let result = run_check(src)?
  assert result.ok, "io should cover net"
}

test test_io_does_not_cover_time { |ctx|
  let src = test.temp_file(ctx, name: "t.xsh", contents: b"proc bad() [io] {\n  let _ = time.now()\n}\n")?
  let result = run_check(src)?
  assert ! result.ok, "expected check failure"
  assert "check.effect-violation" in result.out
  assert "time" in result.out
}

test test_question_mark_requires_error_effect { |ctx|
  let src = test.temp_file(
    ctx,
    name: "t.xsh",
    contents: b"proc bad() [fs] -> Result[Str] {\n  return p\"x\".read_text()?\n}\n",
  )?

  let result = run_check(src)?
  assert ! result.ok, "expected check failure"
  assert "check.effect-violation" in result.out
  assert "error" in result.out
}

test test_run_form_requires_process_effect { |ctx|
  let src = test.temp_file(ctx, name: "t.xsh", contents: b"proc bad() [fs] {\n  run echo hello\n}\n")?
  let result = run_check(src)?
  assert ! result.ok, "expected check failure"
  assert "check.effect-violation" in result.out
  assert "process" in result.out
}

test test_unrestricted_proc_unchecked { |ctx|
  let src = test.temp_file(
    ctx,
    name: "t.xsh",
    contents: b"proc legacy() {\n  let _ = dns.lookup(\"g.com\")\n  run echo hi\n}\n",
  )?

  let result = run_check(src)?
  assert result.ok, "private proc effects should be inferred"
}

test test_restricted_caller_sees_inferred_export_effects { |ctx|
  let src = test.temp_file(
    ctx,
    name: "t.xsh",
    contents: b"##! Public effect boundary.\n## Runs a process, which its inferred effects publish.\nexport proc legacy() {\n  run true\n}\nproc restricted() [fs] {\n  legacy()\n}\n",
  )?

  let result = run_check(src)?
  assert ! result.ok, "expected check failure"
  assert "check.effect-violation" in result.out
  assert "process" in result.out
}

test test_proc_to_proc_subset_passes { |ctx|
  let src = test.temp_file(
    ctx,
    name: "t.xsh",
    contents: b"proc reader() [fs, error] -> Result[Str] {\n  return p\"x\".read_text()?\n}\nproc caller() [fs, error] -> Result[Str] {\n  return reader()?\n}\n",
  )?

  let result = run_check(src)?
  assert result.ok, "superset caller should pass"
}

test test_annotated_proc_not_flagged_by_linter { |ctx|
  let src = test.temp_file(
    ctx,
    name: "t.xsh",
    contents: b"proc main() [fs, error] {\n  let _ = p\"x\".read_text()?\n}\n",
  )?

  let out = run_lint(src)?
  let unannotated_effects = rx"unannotated-effects"
  let flagged = unannotated_effects.matches(out)
  assert ! flagged, "already annotated, no suggestion expected"
}

# Requires `xsht check` to reject `source` with the diagnostic `code`.
proc expect_check_code(ctx: TestContext, source: Str, code: Str) [fs, process, error] {
  let src = test.temp_file(ctx, name: "rejected.xsh", contents: bytes.from_text(source))?
  let result = run_check(src)?
  assert ! result.ok, f"expected {code} for {source}"
  assert f"[{code}]" in result.out, f"expected {code} for {source}: {result.out}"
}

# Requires `xsht check` to report none of `codes` for `source`.
proc expect_check_free_of(ctx: TestContext, source: Str, codes: List[Str]) [fs, process, error] {
  let src = test.temp_file(ctx, name: "accepted.xsh", contents: bytes.from_text(source))?
  let result = run_check(src)?
  for code in codes {
    assert f"[{code}]" not in result.out, f"unexpected {code} for {source}: {result.out}"
  }
}

test test_error_propagation_is_allowed_in_value_returning_proc { |ctx|
  expect_check_free_of(
    ctx,
    r"""
proc parse_uint(s: Str, min: Int) [error] -> Int {
  let value = s.parse_int()?
  if value < min {
    return min
  }
  return value
}
""",
    ["check.try-context", "check.effect-violation"],
  )
}

test test_pure_functions_reject_process_time_system_and_identity_calls { |ctx|
  for source in [
    "pure bad() -> Int {\n  let _ = process.list()?\n  return 0\n}\n",
    "pure bad() -> Int {\n  let _ = process.port(1)?\n  return 0\n}\n",
    "pure bad() -> Int {\n  let _ = process.which(\"sh\")?\n  return 0\n}\n",
    "pure bad(command: Command) -> Int {\n  let _ = process.run(command)?\n  return 0\n}\n",
    "pure bad() -> Int {\n  let _ = time.now()\n  return 0\n}\n",
    "pure bad() -> Int {\n  let _ = system.hostname()?\n  return 0\n}\n",
    "pure bad() -> Int {\n  let _ = user.current()?\n  return 0\n}\n",
    "pure bad() -> Int {\n  let _ = group.current()?\n  return 0\n}\n",
  ] {
    expect_check_code(ctx, source, "check.pure-effect")
  }
}

test test_public_callee_effects_are_inferred_without_a_contract { |ctx|
  for source in [
    r"""
##! Public effect boundary.
## Trims a line.
export proc trim_line(value: Str) -> Str {
  return value
}

proc main() [fs, error] -> Result[Str] {
  return Ok(trim_line("hello"))
}
""",
    r"""
##! Public effect boundary.
## Trims a line.
export proc trim_line(value: Str) [] -> Str {
  return value
}

proc main() [fs, error] -> Result[Str] {
  return Ok(trim_line("hello"))
}
""",
  ] {
    expect_check_free_of(ctx, source, ["check.effect-violation"])
  }

  expect_check_code(
    ctx,
    r"""
##! Public effect boundary.
## Reads the clock.
export proc stamp() -> Int {
  let _ = time.now()
  1
}

proc main() [fs, error] -> Result[Int] {
  return Ok(stamp())
}
""",
    "check.effect-violation",
  )
}

test test_method_calls_require_their_effects { |ctx|
  expect_check_code(
    ctx,
    "proc bad(target: Path) [error] -> Result[Str] {\n  return target.read_text()?\n}\n",
    "check.effect-violation",
  )
  expect_check_free_of(
    ctx,
    "proc good(target: Path) [fs, error] -> Result[Str] {\n  return target.read_text()?\n}\n",
    ["check.effect-violation"],
  )
}

test test_http_module_calls_require_the_net_effect { |ctx|
  let body = r"""
  let _ = net.request({method: "GET", url: "https://example.test/"})
  let _ = net.request_many({requests: [{method: "GET", url: "https://example.test/"}]})
  let _ = net.download({url: "https://example.test/file", dest: Path("out")})
  let _ = net.download_many({downloads: [{url: "https://example.test/file", dest: Path("out")}]})
  let _ = net.upload({url: "https://example.test/upload", source: Path("in")})
}
"""
  expect_check_code(ctx, "\nproc bad() [fs] -> Unit {" + body, "check.effect-violation")
  expect_check_free_of(ctx, "\nproc good() [net] -> Unit {" + body, ["check.effect-violation"])
  expect_check_free_of(ctx, "\nproc good() [io] -> Unit {" + body, ["check.effect-violation"])
}

test test_owned_net_jobs_are_typed_and_require_the_net_effect { |ctx|
  let body = r"""
  let job = net.start({method: "GET", url: "https://example.test/"})?
  let _ = job.wait()?
}
"""
  expect_check_code(ctx, "\nproc bad() [error] -> Unit {" + body, "check.effect-violation")
  expect_check_free_of(
    ctx,
    "\nproc good() [net, error] -> Unit {" + body,
    ["check.effect-violation", "check.type-mismatch", "check.unknown-method"],
  )

  let opaque = test.temp_file(ctx, name: "opaque.xsh", contents: b"let job: NetJob = {id: 1}\n")?
  let result = run_check(opaque)?
  assert ! result.ok, "expected an opaque NetJob diagnostic"
  assert "`NetJob` is a runtime-only type" in result.out, result.out
}

test test_value_returning_proc_calls_are_allowed_in_expression_position { |ctx|
  expect_check_free_of(
    ctx,
    r"""
proc load_names(root: Path) -> Result[List[Str]] {
  return ["demo"]
}
let names = load_names(Path("src"))?
""",
    ["check.expr-proc", "check.pure-effect"],
  )
  expect_check_code(
    ctx,
    r"""
proc load_name() -> Result[Str] {
  return "demo"
}
pure bad() -> Result[Str] {
  return load_name()?
}
""",
    "check.pure-effect",
  )
}

test test_dynamic_module_load_and_proc_call_values_check { |ctx|
  expect_check_free_of(
    ctx,
    r"""
let loaded = module.load(Path("package.xsh"))?
let build: Proc = loaded.build
let label: Pure = loaded.label
let name: Str = label.call("demo")
build.call("dest")?
""",
    ["check.type-mismatch", "check.call-target", "check.unknown-module-api"],
  )
  expect_check_code(
    ctx,
    r"""
let loaded = module.load(Path("package.xsh"))?
let build: Proc = loaded.build
let result = build("dest")
""",
    "check.unresolved-call",
  )
  expect_check_code(
    ctx,
    r"""
pure bad(build: Proc) -> Result[Unit] {
  build.call("dest")?
  return Ok()
}
""",
    "check.pure-effect",
  )
}
