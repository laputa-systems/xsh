pure xsht_bin() -> Path {
  return p"target/debug/xsht"
}

type CheckResult = {ok: Bool, out: Str}

proc run_check(src: Path) [fs, process, error] -> Result[CheckResult] {
  let err = fp"${src}.err"
  let status: Status = run.status xsht_bin() check $src 2> $err
  let out = err.read_text()?
  return Ok({ok: status.exited_with(0), out})
}

proc run_lint(src: Path) [fs, process] -> Result[Str] {
  let err = fp"${src}.err"
  run.status xsht_bin() lint $src 2> $err
  return err.read_text()
}

proc test_module_call_blocked_by_annotation(ctx: TestContext) [fs, process, error] {
  let src = test.temp_file(ctx, name: "t.xsh", contents: b"proc bad() [fs] {\n  dns.lookup(\"g.com\")\n}\n")?
  let result = run_check(src)?
  test.ok(! result.ok, "expected check failure")?
  test.contains(result.out, "check.effect-violation")?
  test.contains(result.out, "net")?
}

proc test_correct_annotation_passes(ctx: TestContext) [fs, process, error] {
  let src = test.temp_file(
    ctx,
    name: "t.xsh",
    contents: b"proc good() [net, error] {\n  let _ = dns.lookup(\"g.com\")?\n}\n",
  )?

  let result = run_check(src)?
  test.ok(result.ok, "expected clean check")?
}

proc test_io_covers_net(ctx: TestContext) [fs, process, error] {
  let src = test.temp_file(
    ctx,
    name: "t.xsh",
    contents: b"proc good() [io, error] {\n  let _ = dns.lookup(\"g.com\")?\n}\n",
  )?

  let result = run_check(src)?
  test.ok(result.ok, "io should cover net")?
}

proc test_io_does_not_cover_time(ctx: TestContext) [fs, process, error] {
  let src = test.temp_file(ctx, name: "t.xsh", contents: b"proc bad() [io] {\n  let _ = time.now()\n}\n")?
  let result = run_check(src)?
  test.ok(! result.ok, "expected check failure")?
  test.contains(result.out, "check.effect-violation")?
  test.contains(result.out, "time")?
}

proc test_question_mark_requires_error_effect(ctx: TestContext) [fs, process, error] {
  let src = test.temp_file(
    ctx,
    name: "t.xsh",
    contents: b"proc bad() [fs] -> Result[Str] {\n  return fs.read_text(p\"x\")?\n}\n",
  )?

  let result = run_check(src)?
  test.ok(! result.ok, "expected check failure")?
  test.contains(result.out, "check.effect-violation")?
  test.contains(result.out, "error")?
}

proc test_run_form_requires_process_effect(ctx: TestContext) [fs, process, error] {
  let src = test.temp_file(ctx, name: "t.xsh", contents: b"proc bad() [fs] {\n  run echo hello\n}\n")?
  let result = run_check(src)?
  test.ok(! result.ok, "expected check failure")?
  test.contains(result.out, "check.effect-violation")?
  test.contains(result.out, "process")?
}

proc test_unrestricted_proc_unchecked(ctx: TestContext) [fs, process, error] {
  let src = test.temp_file(
    ctx,
    name: "t.xsh",
    contents: b"proc legacy() {\n  let _ = dns.lookup(\"g.com\")\n  run echo hi\n}\n",
  )?

  let result = run_check(src)?
  test.ok(result.ok, "unrestricted proc should pass")?
}

proc test_restricted_cannot_call_unrestricted_proc(ctx: TestContext) [fs, process, error] {
  let src = test.temp_file(
    ctx,
    name: "t.xsh",
    contents: b"proc legacy() {\n  return\n}\nproc restricted() [fs] {\n  legacy()\n}\n",
  )?

  let result = run_check(src)?
  test.ok(! result.ok, "expected check failure")?
  test.contains(result.out, "check.effect-violation")?
}

proc test_proc_to_proc_subset_passes(ctx: TestContext) [fs, process, error] {
  let src = test.temp_file(
    ctx,
    name: "t.xsh",
    contents: b"proc reader() [fs, error] -> Result[Str] {\n  return fs.read_text(p\"x\")?\n}\nproc caller() [fs, error] -> Result[Str] {\n  return reader()?\n}\n",
  )?

  let result = run_check(src)?
  test.ok(result.ok, "superset caller should pass")?
}

proc test_linter_infers_fs_error(ctx: TestContext) [fs, process, error] {
  let src = test.temp_file(ctx, name: "t.xsh", contents: b"proc main() {\n  let _ = fs.read_text(p\"x\")?\n}\n")?
  let out = run_lint(src)?
  test.contains(out, "lint.unannotated-effects")?
  test.contains(out, "[fs, error]")?
}

proc test_linter_infers_net(ctx: TestContext) [fs, process, error] {
  let src = test.temp_file(ctx, name: "t.xsh", contents: b"proc main() {\n  let _ = dns.lookup(\"x.test\")\n}\n")?
  let out = run_lint(src)?
  test.contains(out, "lint.unannotated-effects")?
  test.contains(out, "[net]")?
}

proc test_linter_infers_process_from_run(ctx: TestContext) [fs, process, error] {
  let src = test.temp_file(ctx, name: "t.xsh", contents: b"proc main() {\n  run echo hello\n}\n")?
  let out = run_lint(src)?
  test.contains(out, "lint.unannotated-effects")?
  test.contains(out, "process")?
}

proc test_annotated_proc_not_flagged_by_linter(ctx: TestContext) [fs, process, error] {
  let src = test.temp_file(
    ctx,
    name: "t.xsh",
    contents: b"proc main() [fs, error] {\n  let _ = fs.read_text(p\"x\")?\n}\n",
  )?

  let out = run_lint(src)?
  let unannotated_effects = regex.compile("unannotated-effects")?
  let flagged = unannotated_effects.matches(out)
  test.ok(! flagged, "already annotated, no suggestion expected")?
}
