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
    contents: b"proc bad() [fs] -> Result[Str] {\n  return fs.read_text(p\"x\")?\n}\n",
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
    contents: b"proc reader() [fs, error] -> Result[Str] {\n  return fs.read_text(p\"x\")?\n}\nproc caller() [fs, error] -> Result[Str] {\n  return reader()?\n}\n",
  )?

  let result = run_check(src)?
  assert result.ok, "superset caller should pass"
}

test test_annotated_proc_not_flagged_by_linter { |ctx|
  let src = test.temp_file(
    ctx,
    name: "t.xsh",
    contents: b"proc main() [fs, error] {\n  let _ = fs.read_text(p\"x\")?\n}\n",
  )?

  let out = run_lint(src)?
  let unannotated_effects = rx"unannotated-effects"
  let flagged = unannotated_effects.matches(out)
  assert ! flagged, "already annotated, no suggestion expected"
}
