# What `xsht check` reported for one program.
type Checked = {status: Status, stderr: Str}

# Runs only the checker over `source`: these programs start processes, or
# would fail at run time.
proc check(ctx: TestContext, source: Str) [fs, process, error] -> Result[Checked] {
  let file = test.temp_file(ctx, name: "checked.xsh", contents: bytes.from_text(source))?
  let checked = run.capture --text "xsht" check $file
  {status: checked.status, stderr: checked.stderr}
}

# Requires the checker to reject `source` with every diagnostic in `codes`.
proc expect_rejected(ctx: TestContext, source: Str, codes: List[Str]) [fs, process, error] {
  let checked = check(ctx, source)?
  assert checked.status.exited_with(2), f"{source}: {checked.stderr}"
  for code in codes {
    assert f"[{code}]" in checked.stderr, f"expected {code} for {source}: {checked.stderr}"
  }
}

# Requires the checker to accept `source`.
proc expect_accepted(ctx: TestContext, source: Str) [fs, process, error] {
  let checked = check(ctx, source)?
  assert checked.status.exited_with(0), f"{source}: {checked.stderr}"
}

test test_checker_accepts_result_unit_proc_expression_calls { |ctx|
  expect_accepted(
    ctx,
    r"""
proc compile(src: Path) -> Result[Unit] {
  return Ok()
}
compile(Path("main.c"))?
""",
  )
}

test test_checker_rejects_command_position_contract_violations { |ctx|
  for case in [
    {
      source: "make -j4\n",
      code: "check.unresolved-proc-command",
    },
    {
      source: "pure helper(src: Str) -> Str { return src }\nhelper hi\n",
      code: "check.command-pure",
    },
    {
      source: "proc compile(src: Path) -> Result[Unit] { return Ok() }\ncompile Path(\"main.c\") ?\n",
      code: "check.proc-command-syntax",
    },
    {
      source: "let b = b\"x\"\nproc compile(src: Path) -> Result[Unit] { return Ok() }\ncompile (b) ?\n",
      code: "check.proc-command-syntax",
    },
    {
      source: "proc bad() -> Result[Unit] { return 1 }\n",
      code: "check.type-mismatch",
    },
    {
      source: "try run.text echo hi\n",
      code: "check.ignored-result",
    },
    {
      source: "proc bad() [process] -> Unit { run.status false ? }\n",
      code: "check.try-context",
    },
    {
      source: "let b = b\"x\"\nrun echo (b) ?\n",
      code: "check.argv-conversion",
    },
    {
      source: "run @([])\n",
      code: "check.run-target",
    },
    {
      source: "proc needs(value: Str) -> Result[Unit] { return Ok() }\nlet b = b\"x\"\nneeds (b) ?\n",
      code: "check.proc-command-syntax",
    },
    {
      source: "proc needs(data: Bytes) -> Result[Unit] { return Ok() }\nneeds abc ?\n",
      code: "check.proc-command-syntax",
    },
    {
      source: "proc bad() -> Result[Str] { print \"x\" }\nbad ?\n",
      code: "check.type-mismatch",
    },
    {
      source: "proc print() -> Result[Unit] { return Ok() }\n",
      code: "check.core-command-shadow",
    },
    {
      source: "pure bad() -> Result[Unit] { let status = run.status false ?\nreturn Ok() }\n",
      code: "check.pure-run",
    },
  ] {
    expect_rejected(ctx, case.source, [case.code])
  }
}

test test_checker_ignored_result_diagnostic_points_at_the_statement { |ctx|
  let checked = check(ctx, "try run.text echo hi\n")?
  assert "err[check.ignored-result]" in checked.stderr, checked.stderr
  assert ":1:1\n" in checked.stderr, checked.stderr
  assert "\n  ^^^^^^^^^^^^^^^^^^^^ " in checked.stderr, checked.stderr
}

test test_checker_allows_proc_expression_splice_to_supply_multiple_arguments { |ctx|
  expect_accepted(
    ctx,
    r"""
proc pair(a: Str, b: Str) -> Result[Unit] {
  return Ok()
}
let parts = ["a", "b"]
pair(@parts)?
""",
  )
}

test test_checker_reports_empty_process_argv { |ctx|
  expect_rejected(
    ctx,
    r"""
let command = process.command_argv("echo", [])
""",
    ["check.process-argv-empty"],
  )
}

test test_checker_handles_spawn_wait_process_handles { |ctx|
  expect_accepted(
    ctx,
    r"""
proc start() [process, error] -> Result[ProcessHandle] {
  return spawn run true ?
}

proc main() [process, error] -> Result[Unit] {
  let h: ProcessHandle = start()?
  let pid: Int = h.pid
  let command: Str = h.command
  let argv: List[Str] = h.argv
  let detached: Bool = h.detached
  let s: Status = wait h?
  let h2: ProcessHandle = spawn run true ?
  h2.cancel()?
  let handles: List[ProcessHandle] = [spawn run true ?, spawn run false ?]
  let statuses: List[Status] = wait handles?
  return Ok()
}
""",
  )
}

test test_checker_rejects_invalid_spawn_wait_shapes { |ctx|
  for case in [
    {
      source: "proc start() [error] -> Result[ProcessHandle] {\n  return spawn run true ?\n}\n",
      code: "check.effect-violation",
    },
    {
      source: "let x = wait 1\n",
      code: "check.wait-target",
    },
    {
      source: "let h = spawn run.text printf ok\n",
      code: "check.spawn-run-kind",
    },
    {
      source: "let h = spawn run printf ok | run cat\n",
      code: "check.spawn-run-shape",
    },
  ] {
    expect_rejected(ctx, case.source, [case.code])
  }
}

test test_checker_handles_foundation_builders_literals_and_context { |ctx|
  expect_accepted(
    ctx,
    r"""
let mode = 0o755
let command = process.command {
  timeout = 30s
  detach = true
  new_session = false
  ignore_hup = true
  run --timeout=1s echo ok
}
let _command = command
""",
  )
}

test test_checker_rejects_foundation_contract_errors { |ctx|
  for case in [
    {
      source: "use process\nlet command = process.command { timeout = 1\nrun true }\n",
      code: "check.type-mismatch",
    },
    {
      source: "let command = process.command { unknown = 1\nrun true }\n",
      code: "check.builder-field",
    },
    {
      source: "let command = process.command { cpu_max = 0\nrun true }\n",
      code: "check.builder-field",
    },
    {
      source: "run --cpumax=0 true\n",
      code: "check.cpumax",
    },
    {
      source: "run true | run --cpumax=80 cat\n",
      code: "check.pipeline-cpumax",
    },
    {
      source: "let command = process.command_argv(\"true\", 1)\n",
      code: "check.type-mismatch",
    },
    {
      source: "let command = process.command_argv(\"true\", [\"true\", {bad: true}])\n",
      code: "check.type-mismatch",
    },
    {
      source: "let command = process.command_argv(\"true\", [\"true\"], cpu_max: 0)\n",
      code: "check.named-arg",
    },
    {
      source: "let status = process.run(\"true\")?\n",
      code: "check.type-mismatch",
    },
    {
      source: "let message = f\"bad {[1, 2]}\"\n",
      code: "check.display-conversion",
    },
    {
      source: "use hash\nlet digest = hash.sha256(1)\n",
      code: "check.type-mismatch",
    },
    {
      source: "use hash\nhash.verify_file(Path(\"archive.tar\"), bad: \"abc\")?\n",
      code: "check.named-arg",
    },
  ] {
    expect_rejected(ctx, case.source, [case.code])
  }
}
