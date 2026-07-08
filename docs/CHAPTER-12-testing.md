# Chapter 12: Testing

Examples prove that a script can run. Native tests prove that the pieces inside
the script behave the way you expect, including temp files, expected failures,
and mocked host calls.

By the end of this chapter, you should know the test file shape, the assertion
style, how to use per-test temp resources, and when to mock DNS or network
operations.

## Write A Native Test

Native XSH tests live under `tests/**/*.xsh` and `showcase/tests/**/*.xsh`
relative to the current directory:

```sh
xsht test
```

The runner discovers top-level `proc test_*` functions:

```xsh
proc test_name() -> Result[Unit] {
  test.eq(1, 1)?
}
```

Tests can also accept `ctx: TestContext` when they need temp resources or host
mocks:

```xsh
proc test_with_context(ctx: TestContext) -> Result[Unit] {
  let file = test.temp_file(ctx, name: "sample", contents: b"ok")?
  let text = fs.read_text(file)?
  test.eq(text, "ok")?
}
```

Test files are module-shaped. Top level may contain `use`, `let`, `type`,
`proc`, `pure`, and `export`. Commands, mutation, and control flow belong
inside test procs so each test can run with a fresh evaluator, fresh capture
buffers, isolated host state, and a per-test temp root.

## Assert With Results

The `test` module returns ordinary `Result[Unit]` values:

```xsh
test.ok(condition)?
test.eq(left, right)?
test.ne(left, right)?
test.contains(text, "needle")?
test.error_kind(result, "missing-field")?
test.fail("unreachable")
test.skip("needs host feature")
```

Assertion failures return structured test failure errors. Skips return
structured test skip errors; the runner reports them separately from failures.

Common trap: keep assertions close to the behavior being tested. If a test
needs a long script setup before one check, move the reusable setup into a proc
or use a cataloged example instead.

## Use Temp Resources

`TestContext` contains the stable test name, source file, and temp root:

```xsh
proc test_temp(ctx: TestContext) -> Result[Unit] {
  let path = test.temp_path(ctx)
  let dir = test.temp_dir(ctx, name: "work")?
  let file = test.temp_file(ctx, name: "input", contents: b"data")?
  test.ne(path, dir)?
  test.ok(file.name.starts_with("input"))?
}
```

Temp roots are cleaned after each test by default. Use `xsht test --keep-temp`
when debugging filesystem state.

## Run Child Scripts

Use `test.run_script` when a test needs to assert whole-script behavior such as
exit status, stdout/stderr, invalid UTF-8 output, script arguments, or
environment-sensitive code:

```xsh
proc test_child_script(ctx: TestContext) -> Result[Unit] {
  let output = test.run_script(
    ctx,
    r"""print ${ARGV[0]}
""",
    ["demo"],
    {MODE: "test"},
  )?

  test.ok(output.success, output.stderr)?
  test.eq(output.status, 0)?
  test.eq(output.stdout, "demo\n")?
}
```

The returned record includes `success`, numeric `status`, lossy text fields
`stdout` and `stderr`, and exact byte fields `stdout_bytes` and `stderr_bytes`.

Use `test.run_xsh` when the test needs to pass flags to the `xsh` executable
before the script path. Use `test.run_xsht_trace` for trace-mode tests:

```xsh
proc test_trace(ctx: TestContext) -> Result[Unit] {
  let output = test.run_xsht_trace(ctx, "run true ?\n", ["--raw"])?

  test.ok(output.success, output.stderr)?
  test.contains(output.stderr, "kind=run.start")?
}
```

## Mock Host Boundaries

Tests can mock DNS and net host calls:

```xsh
proc test_lookup(ctx: TestContext) -> Result[Unit] {
  test.mock(
    ctx,
    "dns.lookup",
    {name: "example.test"},
    Ok([{name: "example.test", record: "A", value: "127.0.0.1", ttl: 60}])
  )?

  let rows = dns.lookup("example.test")?
  test.eq(rows[0].value, "127.0.0.1")?
  let calls = test.calls(ctx, "dns.lookup")
  test.eq(calls.len(), 1)?
}
```

Matchers are partial records compared against normalized call arguments. If an
operation has mocks and none match, the host call returns structured
unmatched-mock errors. Operations without mocks use real host behavior.

Why XSH shines here: tests can cover orchestration logic without depending on
the network, the local machine's DNS state, or permanent filesystem paths.

## Run The Right Set

Useful runner forms:

```sh
xsht test [FILTER]
xsht test --exact tests/xsh/basic.xsh::test_pass
xsht test --list
xsht test --nocapture
xsht test --fail-fast
xsht test --jobs 4
xsht test --examples [FILTER]
xsht test --all [FILTER]
xsht test --cov [FILTER]
xsht test --cov-json target/xsh-cov/root.json [FILTER]
```

Native test IDs are stable names such as
`tests/xsh/net.xsh::test_fetch`. Catalog example IDs are
`examples::hello`, `examples::dns-net`, and so on.

Tests run concurrently by default, capped to a small worker count. Use
`--jobs 1` for serial execution. `--nocapture`, `--fail-fast`, and syscall
tracing run serially so their output and stopping behavior stay predictable.

`--cov` runs matching native and example tests, then prints XSH source line/proc
coverage and standard API coverage derived from raw JSONL traces. `--cov-json`
writes the same coverage data as structured JSON without printing the coverage
report. Use `make cov` for Linux LLVM source coverage of the Rust implementation
plus the aggregated XSH coverage report.

## What You Know Now

Use cataloged examples for user-facing scripts whose stdout, stderr, and exit
status are part of the docs. Use native tests for helper logic, temp resources,
expected errors, mocks, and coverage. The next chapter shows how traces explain
what happened during a real run.
